//! DrainageEvent — where water runs on the tectonic surface.
//!
//! Routes water over the composed surface on a coarse world-wide lattice of
//! nodes, and publishes the channel network of every landmass as reaches with
//! catchment. It moves no ground and its query returns nothing: dissection
//! cuts along what this publishes.
//!
//! # The surface is a function, plus one index
//!
//! The field layers beneath — substrate, tilt, thickening — sum to one
//! function of position and seed, and the ranges come from the thrusting
//! layer's fronts, the one index this deform reads over its window.
//! [`surface_at`] is that sum, evaluated at each node without materialising a
//! tile, which is what lets routing, which needs the ground over a whole
//! basin, happen in `deform`. A layer added beneath drainage either stays a
//! field or publishes what drainage needs, and is added to [`surface_at`], or
//! drainage routes over ground that is not there. `drainage_probe` checks the
//! sum against the composite.
//!
//! # One lattice, one window
//!
//! Nodes lie on one world-wide hex lattice at [`NODE_SPACING`], each a tile
//! centre. A cell routes over its window — its footprint plus one ring — and
//! publishes the nodes it owns. A node's outflow is the true downslope of the
//! water surface at it, read off the six facets its neighbours make with it,
//! and its water splits between the two neighbours bracketing that angle.
//! Steepest descent to one of six neighbours quantises every gentle slope into
//! six directions and runs neighbouring streams in parallel until the ground
//! turns by 60°, which is a comb rather than a network. The outflow is a
//! property of the surface at the node, so two windows containing it agree
//! wherever their filled surfaces agree, and a reach crossing a cell boundary
//! continues on the far side. Catchment is counted within the window, so the
//! largest rivers plateau in discharge past it rather than truncating.
//!
//! Reading the front index deforms the thrusting cells under the window and
//! nothing else beneath. Node elevations are memoised across windows, so each
//! node is evaluated once however many windows contain it.
//!
//! # Closed ground and the sill
//!
//! The flood fills every pit to its rim, so catchment carries through
//! closed ground and out over its rim: a basin swallows no river. Inside
//! the basin the water runs on over the ground itself, not the fill, so a
//! river entering closed ground runs down to the basin's lowest node, the
//! pit, and ends there; the pit's water leaves over the rim's low node,
//! the sill, which is the head of the reach leaving, carrying the whole
//! basin's water. No lake stands over the basin: what it holds is its
//! rivers, run to its floor. Basins are geologically brief: the water leaving cuts the sill,
//! so a young orogen is basin country and an old one is drained through
//! gorges. A plate carries an age, and each basin's sill is cut by that
//! age and the square root of the catchment leaving over it, the growth
//! law a valley's depth follows, no deeper than the basin behind it or the
//! base level beneath it. The ground down the outflow from the sill is
//! graded from the cut sill to the first ground no higher, the breach
//! through the rim, so the river leaves over a lip and falls. Across the
//! drained floor the flood's path from the deepest node to the sill goes a
//! hair under the new fill, so the floor drains as one basin instead of a
//! chain of sub-basins; a pocket off that path is a basin of its own. A
//! basin left shallower than its river's channel drains outright: the
//! river runs through its floor. The window is routed again over the cut
//! ground while a routing cuts any sill, so base levels and the reach
//! leaving each basin agree with every cut, and a sill cut once is not cut
//! again: one cut per basin. A basin spilling past the window has no
//! outlet and no cut. Nothing here moves ground: the envelope is published
//! as the elevation and the breach as a cut beside it, and dissection
//! removes the ground. The water that would stand in a basin, a lake and
//! its shore, is unbuilt.

use std::any::Any;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use common::HexLattice;
use dashmap::DashMap;

use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::lithology::rock_at;
use super::plates::{Coasts, PlateEdgeIndex};
use super::thickening::{plateau_share_of, PLATEAU_RISE};
use super::thrusting::{outlines_of, Outlines};
use super::tilt::tilt_at;
use super::{CellScope, Neighbourhood, TileOutput, TileView, WorldEvent, RING_CLEARANCE};
use crate::lattice::{hex_distance, DIRECTIONS as NEIGHBOURS};
pub use crate::lattice::{node_site, site_at, site_world, NodeKey, NODE_SPACING, NODE_SWING};
pub use crate::tectonic::{aged, YOUNG_SHARE};
use crate::tectonic::PLATE_REACH;
use crate::{hex_to_world, substrate_on};

// ── Constants ───────────────────────────────────────────────────────────────

/// Cell scale, derived: one ring covers a plate's reach from its seed, so a
/// basin that drains a plate's interior to its coast is counted whole.
pub const DRAINAGE_CELL_SCALE: u32 = (PLATE_REACH / RING_CLEARANCE) as u32 + 1;

/// Two levels closer than this are one flat.
const FLAT: f64 = 1e-9;

/// How many times a window is routed again over its cut sills. A drained
/// basin's floor holds sub-basins, each under the same rule, so each pass
/// cuts the sills the pass before uncovered; the ground only falls, so the
/// passes converge, and the cap bounds the cost of a deep nest.
const ROUTING_PASSES: usize = 8;

/// The least depth a basin keeps once its sill is cut, in z-levels: a
/// river's channel. A basin shallower than the channel the river leaving
/// it has cut is no basin, the river runs through its floor, so the sill
/// is cut to the floor instead of leaving a sliver of closed ground.
pub const REMNANT_MIN: f64 = 3.0;

/// How far under the new fill a hump on the flood's path across a basin
/// is cut: a hair, enough that the routing floods it and the basin stays
/// one basin, and too little for a tile to read.
const UNDER: f64 = 1e-6;

// ── Catchment ───────────────────────────────────────────────────────────────

/// Catchment, in nodes, at which a channel begins to cut: the channel head,
/// a hillslope's length below the divide.
///
/// EMPIRICAL: read from `drainage_probe::catchment_distribution`, at which
/// about a third of land nodes carry a channel and the rest are hillslope.
pub const CHANNEL_HEAD: f64 = 4.0;

/// Catchment, in nodes, at which a channel has cut its full share.
///
/// EMPIRICAL: the 99th percentile of catchment in the same census, so a
/// trunk is full and everything else is partial.
pub const CATCHMENT_FULL: f64 = 48.0;

/// The channel head on rock of `erodibility`: the head's catchment over
/// it, so a hard rock needs proportionally more water before its sheet
/// flow gathers into a channel, and a shield's streams begin further from
/// the divide than a shale plain's.
pub fn head_on(erodibility: f64) -> f64 {
    CHANNEL_HEAD / erodibility.max(0.05)
}

/// How far past the channel head on rock of `erodibility` a catchment of
/// `catchment` nodes has grown, as the square root of its share of the way
/// to a full trunk: the way a channel's width and depth grow with
/// discharge. None below the head.
pub fn growth(catchment: f64, erodibility: f64) -> Option<f64> {
    let head = head_on(erodibility);
    if catchment <= head {
        return None;
    }
    Some(((catchment - head) / (CATCHMENT_FULL - head).max(1.0)).min(1.0).sqrt())
}

/// How much of the basin behind it a full trunk cuts its sill by, per unit
/// of its plate's age. Above one, so a plate is drained before it is fully
/// aged, and what an aged plate keeps are the basins that drain little.
pub const SILL_CUT_RATE: f64 = 1.5;

/// The share of the basin behind it a sill is cut by, with `catchment`
/// nodes of water leaving over it on a plate of `age` through rock of
/// `erodibility`: the river's growth past the channel head at
/// [`SILL_CUT_RATE`] per unit of age and erodibility, saturating at the
/// whole. Nothing below the head: a pit that drains little keeps its rim
/// at any age, and a hard sill holds longer.
pub fn sill_share(age: f64, catchment: f64, erodibility: f64) -> f64 {
    growth(catchment, erodibility).map_or(0.0, |g| (SILL_CUT_RATE * age * erodibility * g).min(1.0))
}

/// The share of its height above base level a full trunk on a fully aged
/// plate has removed: the Grand Canyon's 1.6 km cut through a 2.3 km
/// plateau. The rest is the fall that keeps the river flowing.
pub const RELIEF_SHARE_MAX: f64 = 0.7;

/// The share of the height above base level a channel of `catchment` nodes
/// on a plate of `age` has cut through rock of `erodibility`: nothing
/// below the channel head, then growing as the square root of the
/// catchment past it, the way a channel's width does, to the full share;
/// with age, from a young plate's share of it to the whole; and by the
/// rock, the whole in shale and a third of it in basement.
pub fn relief_share(catchment: f64, age: f64, erodibility: f64) -> f64 {
    growth(catchment, erodibility).map_or(0.0, |g| RELIEF_SHARE_MAX * g * aged(age) * erodibility)
}

/// The floor a river has cut to at a node on its own, in z-levels: where
/// the routing cut a sill or a breach, exactly the ground it routed over;
/// else the envelope less its share of the height above base level, which
/// on closed ground is the pit's, so a river runs down into a basin as it
/// runs down to the sea. A river is held up by a harder lip downstream,
/// which [`Routing`] settles over the reach: the floor published is never
/// below the next floor downstream. Dissection cuts to it and never below.
pub fn floor_at(elevation: f64, base: f64, catchment: f64, age: f64, erodibility: f64, cut: f64, sill: bool) -> f64 {
    if sill || cut > 0.0 {
        return elevation - cut;
    }
    elevation - (elevation - base).max(0.0) * relief_share(catchment, age, erodibility)
}

// ── The surface ─────────────────────────────────────────────────────────────

/// What drainage reads beneath it at a position: the surface in z-levels,
/// the age of the plate it stands on, and the erodibility of the rock at
/// the surface.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub surface: f64,
    pub age: f64,
    pub erodibility: f64,
}

/// The ground beneath drainage at a position: the layers summed as the
/// functions they are, the substrate from the coasts in reach, the tilt,
/// the ranges and the plateau from the plate outlines in reach, and the
/// cuesta the rock stands as, the plate looked up once for all of them.
/// The surface equals the composed tile's elevation at a tile centre.
pub fn ground_at(wx: f64, wy: f64, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Ground {
    let substrate = substrate_on(wx, wy, coasts, seed);
    let base = substrate + tilt_at(wx, wy, substrate, seed);
    let Some(at) = outlines.at(wx, wy) else {
        return Ground { surface: base, age: 0.0, erodibility: 1.0 };
    };
    let relief = outlines.relief_of(&at).max(0.0);
    let plateau = PLATEAU_RISE * plateau_share_of(at.plate, &at.distances).max(0.0);
    let rock = rock_at(wx, wy, seed, at.plate.id, at.plate.age, substrate, relief);
    Ground { surface: base + relief + plateau + rock.stand, age: at.plate.age, erodibility: rock.erodibility }
}

/// The surface of [`ground_at`] alone.
pub fn surface_at(wx: f64, wy: f64, seed: u64, coasts: &Coasts, outlines: &Outlines) -> f64 {
    ground_at(wx, wy, seed, coasts, outlines).surface
}

// ── Nodes ───────────────────────────────────────────────────────────────────


/// The steepest of the facets around a node on the water surface: the
/// downslope angle in world space, the two neighbours whose facet it lies
/// in, first then second in angular order, and the share of the flow the
/// second takes. None where nothing descends.
///
/// A facet is the plane through the node and two neighbours consecutive in
/// angle around it, as their sites lie. Its steepest descent is used when
/// it points into the facet; when it points outside, the flow runs down
/// the nearer edge instead, at that edge's slope.
fn steepest_facet(
    nbrs: &[Option<usize>; 6],
    k: usize,
    site: &[(f64, f64)],
    surface: &[f64],
) -> Option<(f64, usize, usize, f64)> {
    use std::f64::consts::PI;
    let (x0, y0, z0) = (site[k].0, site[k].1, surface[k]);
    // The neighbours present, by angle around the node.
    let mut around: Vec<(f64, usize, f64, f64)> = nbrs
        .iter()
        .flatten()
        .map(|&m| {
            let (dx, dy) = (site[m].0 - x0, site[m].1 - y0);
            (dy.atan2(dx), m, dx, dy)
        })
        .collect();
    around.sort_by(|a, b| a.0.total_cmp(&b.0));
    let count = around.len();
    if count < 2 {
        return None;
    }
    let mut best: Option<(f64, f64, usize, usize, f64)> = None;
    for e in 0..count {
        let (ta, a, ax, ay) = around[e];
        let (tb, b, bx, by) = around[(e + 1) % count];
        // The facet spans from a round to b; past the last neighbour it
        // wraps to the first, a wedge wider than any other.
        let width = (tb - ta).rem_euclid(2.0 * PI);
        if width <= 0.0 || width >= PI {
            continue;
        }
        let (za, zb) = (surface[a] - z0, surface[b] - z0);
        // Gradient g of the plane with d_a·g = za and d_b·g = zb.
        let det = ax * by - ay * bx;
        if det.abs() < 1e-9 {
            continue;
        }
        let gx = (za * by - zb * ay) / det;
        let gy = (ax * zb - bx * za) / det;
        let mut slope = gx.hypot(gy);
        if slope <= 0.0 {
            continue;
        }
        let mut angle = (-gy).atan2(-gx);
        let mut rho = (angle - ta).rem_euclid(2.0 * PI);
        if rho > PI {
            rho -= 2.0 * PI;
        }
        if rho < 0.0 {
            rho = 0.0;
            angle = ta;
            slope = -za / ax.hypot(ay);
        } else if rho > width {
            rho = width;
            angle = tb;
            slope = -zb / bx.hypot(by);
        }
        if slope <= 0.0 {
            continue;
        }
        if best.map_or(true, |(bs, ..)| slope > bs) {
            best = Some((slope, angle, a, b, rho / width));
        }
    }
    best.map(|(_, angle, a, b, share)| (angle, a, b, share))
}

// ── What a cell publishes ───────────────────────────────────────────────────

/// Where a reach's last node sends its water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminus {
    /// Into another reach: a confluence, or the same channel in the next cell.
    Continues,
    Sea,
    /// At the lowest node of closed ground, the pit: the water gathers
    /// there and leaves over the sill, but the river ends.
    Basin,
    /// Off the edge of the window that routed it. The neighbouring cell's
    /// window reaches further.
    Edge,
}

#[derive(Clone, Debug)]
pub struct DrainageNode {
    pub key: NodeKey,
    pub q: i32,
    pub r: i32,
    pub wx: f64,
    pub wy: f64,
    /// Ground, in z-levels: the envelope.
    pub elevation: f64,
    /// The flood's fill: the ground, or the level closed ground fills to.
    pub surface: f64,
    /// The node lies on closed ground under the fill: its water runs down
    /// the ground to the basin's pit and leaves over the rim from there.
    pub flooded: bool,
    /// The true downslope at this node, a unit vector in world space. Across
    /// closed ground it is the ground's own, down to the pit; at the pit it
    /// points along the flood's path toward the rim.
    pub direction: (f64, f64),
    /// Water draining through this one, itself included, in nodes, fractional
    /// because a node's water splits, and never less than at any node above
    /// it on its reach: what its channel is cut by.
    pub catchment: f64,
    /// Base level, in z-levels: the pit of the first closed ground down the
    /// larger share's path, or sea level. What a river here cuts toward and
    /// never below.
    pub base: f64,
    /// The node taking the larger share of this one's water. None at a sink.
    pub down: Option<NodeKey>,
    /// This node is a basin's outlet: the sill whose height, less its cut,
    /// is the basin's fill. Dissection cuts exactly `cut` here, so the
    /// basin keeps the fill the routing gave it.
    pub sill: bool,
    /// The age of the plate this node stands on, 0 to 1 as
    /// `tectonic::Plate::age`.
    pub age: f64,
    /// The erodibility of the rock at this node, as `lithology::Rock`.
    pub erodibility: f64,
    /// How far below the envelope the river leaving a cut sill has cut the
    /// ground here: the sill's cut at the sill, and along its outflow the
    /// breach through the rim, to where the ground is no higher. Nothing
    /// elsewhere. A floor is never above the envelope less this.
    pub cut: f64,
    /// The floor the river has cut to here, as [`floor_at`] gives it: what
    /// dissection cuts down to at the node, and what the grade of a reach
    /// is read from.
    pub floor: f64,
}

/// A channel: nodes in downstream order, and where the last one drains.
#[derive(Clone, Debug)]
pub struct Reach {
    pub nodes: Vec<NodeKey>,
    /// The node the last one drains to. None at a sink, the pit of closed
    /// ground included: its water leaves over the sill, the river does not.
    pub joins: Option<NodeKey>,
    pub end: Terminus,
}

#[derive(Clone, Debug, Default)]
pub struct DrainageCell {
    pub nodes: HashMap<NodeKey, DrainageNode>,
    pub reaches: Vec<Reach>,
}

#[derive(Default)]
pub struct DrainageIndex {
    pub cells: HashMap<CellId, DrainageCell>,
}

impl DrainageIndex {
    pub fn lattice() -> HexLattice {
        HexLattice::new(DRAINAGE_CELL_SCALE)
    }

    /// The node, from whichever cell owns it. One lookup, never a scan.
    pub fn node(&self, key: NodeKey) -> Option<&DrainageNode> {
        let (q, r) = node_site(key);
        self.cells.get(&Self::lattice().cell_id(q, r))?.nodes.get(&key)
    }

}

impl Neighbourhood<'_, DrainageIndex> {
    /// The node, from whichever cell of the neighbourhood owns it.
    pub fn node(&self, key: NodeKey) -> Option<&DrainageNode> {
        let (q, r) = node_site(key);
        self.entry(DrainageIndex::lattice().cell_id(q, r))?.nodes.get(&key)
    }
}

impl CellIndex for DrainageIndex {
    type Cell = DrainageCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }
}

impl EventIndex for DrainageIndex {
    fn source_scale(&self) -> u32 { DRAINAGE_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids
            .iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|c| c.nodes.values().map(|n| (n.q, n.r)))
            .collect()
    }

    /// Downstream: the one tile this node's water goes to next.
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)> {
        site_at(q, r)
            .and_then(|key| self.node(key))
            .and_then(|n| n.down)
            .map(|d| vec![node_site(d)])
            .unwrap_or_default()
    }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

// ── Routing ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Below sea level: a sink.
    Sea,
    /// A land node with a neighbour outside the window: an exit.
    Edge,
    Land,
    /// Closed ground under the flood's fill: its water runs down the
    /// ground to the basin's pit, and leaves over the rim from there.
    Basin,
}

/// Min-heap entry for the flood, ordered by level then node so the flood is
/// the same whatever the heap does with equal keys.
#[derive(PartialEq)]
struct Pending {
    level: f64,
    node: usize,
}

impl Eq for Pending {}

impl Ord for Pending {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .level
            .partial_cmp(&self.level)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A basin as the flood fills it: its flooded nodes at one fill, and the
/// rim node its water leaves over, or none when the spill lies beyond the
/// window.
struct RoutedBasin {
    members: Vec<usize>,
    surface: f64,
    outlet: Option<usize>,
}

struct RoutedReach {
    nodes: Vec<usize>,
    end: Terminus,
}

/// Everything routed over one window. `deform` publishes the owned subset;
/// the probes read the whole to compare neighbouring windows.
pub struct Routing {
    pub keys: Vec<NodeKey>,
    pub owned: Vec<bool>,
    /// The envelope at each node, before any cut.
    pub elevation: Vec<f64>,
    /// The flood's fill at each node: the cut ground, or the level closed
    /// ground fills to.
    pub surface: Vec<f64>,
    pub kind: Vec<Kind>,
    /// The true downslope at each node, a unit vector; zero at a sink.
    pub direction: Vec<(f64, f64)>,
    /// Where each node's water goes, with the share each receiver takes. At
    /// most two entries, and they sum to one.
    pub flow: Vec<Vec<(usize, f64)>>,
    /// The receiver of the larger share.
    pub down: Vec<Option<usize>>,
    pub catchment: Vec<f64>,
    /// The catchment each node's channel is cut by: its own, or the most at
    /// any node above it on its reach.
    pub carried: Vec<f64>,
    /// Base level at each node: the fill of the first closed ground down
    /// the larger share's path, a cut node's ground, or sea level.
    pub base: Vec<f64>,
    pub basin_of: Vec<Option<usize>>,
    /// The node each one was flooded from: the flood's path, which across
    /// closed ground is the lowest route to the rim.
    pub parent: Vec<Option<usize>>,
    /// The plate's age at each node.
    pub age: Vec<f64>,
    /// The erodibility of the rock at each node.
    pub erodibility: Vec<f64>,
    /// The cut below the envelope at each node, as `DrainageNode::cut`.
    pub cut: Vec<f64>,
    /// The floor at each node, as `DrainageNode::floor`: its own, held up
    /// by any harder lip downstream.
    pub floor: Vec<f64>,
    index: HashMap<NodeKey, usize>,
    basins: Vec<RoutedBasin>,
    reaches: Vec<RoutedReach>,
}

impl Routing {
    pub fn index_of(&self, key: NodeKey) -> Option<usize> {
        self.index.get(&key).copied()
    }

    /// The cell's own share of the window: its nodes and its runs of every
    /// reach. Every basin in the window says which node is its sill, not
    /// only the owned ones: a sill is owned by whichever cell holds the rim
    /// node, and that cell may own no flooded node of the basin it drains.
    pub fn owned_cell(&self) -> DrainageCell {
        let sills: HashSet<usize> = self.basins.iter().filter_map(|b| b.outlet).collect();
        let mut nodes = HashMap::new();
        for k in 0..self.keys.len() {
            if !self.owned[k] || matches!(self.kind[k], Kind::Sea | Kind::Edge) {
                continue;
            }
            let (q, r) = node_site(self.keys[k]);
            let (wx, wy) = hex_to_world(q, r);
            nodes.insert(
                self.keys[k],
                DrainageNode {
                    key: self.keys[k],
                    q,
                    r,
                    wx,
                    wy,
                    elevation: self.elevation[k],
                    surface: self.surface[k],
                    flooded: self.kind[k] == Kind::Basin,
                    direction: self.direction[k],
                    catchment: self.carried[k],
                    base: self.base[k],
                    down: self.down[k].map(|d| self.keys[d]),
                    sill: sills.contains(&k),
                    age: self.age[k],
                    erodibility: self.erodibility[k],
                    cut: self.cut[k],
                    floor: self.floor[k],
                },
            );
        }

        let mut reaches = Vec::new();
        for reach in &self.reaches {
            let mut run: Vec<usize> = Vec::new();
            for (pos, &k) in reach.nodes.iter().enumerate() {
                if self.owned[k] {
                    run.push(k);
                }
                let last = pos + 1 == reach.nodes.len();
                if (!self.owned[k] || last) && !run.is_empty() {
                    let tail = *run.last().unwrap();
                    let end = if last && tail == k { reach.end } else { Terminus::Continues };
                    // The pit's water leaves over the sill, but the river
                    // ends at the pit: nothing is drawn from it.
                    let joins = if end == Terminus::Basin { None } else { self.down[tail].map(|d| self.keys[d]) };
                    reaches.push(Reach {
                        nodes: run.iter().map(|&m| self.keys[m]).collect(),
                        joins,
                        end,
                    });
                    run.clear();
                }
            }
        }

        DrainageCell { nodes, reaches }
    }
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct DrainageEvent {
    /// The ground at each node, shared by every window that contains it.
    nodes: DashMap<NodeKey, Ground>,
}

impl DrainageEvent {
    pub fn new() -> Self {
        Self { nodes: DashMap::new() }
    }

    /// The ground at a node.
    fn ground(&self, key: NodeKey, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Ground {
        if let Some(g) = self.nodes.get(&key) {
            return *g;
        }
        let (q, r) = node_site(key);
        let (wx, wy) = hex_to_world(q, r);
        let g = ground_at(wx, wy, seed, coasts, outlines);
        self.nodes.insert(key, g);
        g
    }

    /// Route the window of `cell`: every node in the cell and its ring, on
    /// the surface the fronts in reach complete; then again over the ground
    /// with the breaches, while a routing cuts any sill, so base levels and
    /// the reach leaving each basin agree with every cut sill.
    pub fn route(&self, lattice: &HexLattice, cell: CellId, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Routing {
        // ── Nodes in the window, in key order ──
        let centre = lattice.cell_center(cell);
        let reach = 3 * lattice.radius as i32 + 1;
        let window: HashSet<CellId> = lattice.cells_within_distance(cell, 1).into_iter().collect();
        let s = NODE_SPACING;
        let (i0, i1) = ((centre.0 - reach).div_euclid(s), (centre.0 + reach).div_euclid(s) + 1);
        let (j0, j1) = ((centre.1 - reach).div_euclid(s), (centre.1 + reach).div_euclid(s) + 1);

        let mut keys: Vec<NodeKey> = Vec::new();
        let mut owned: Vec<bool> = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                let tile = node_site((i, j));
                if hex_distance(tile, centre) > reach {
                    continue;
                }
                let c = lattice.cell_id(tile.0, tile.1);
                if !window.contains(&c) {
                    continue;
                }
                keys.push((i, j));
                owned.push(c == cell);
            }
        }
        let index: HashMap<NodeKey, usize> =
            keys.iter().enumerate().map(|(k, &key)| (key, k)).collect();
        let nbrs: Vec<[Option<usize>; 6]> = keys
            .iter()
            .map(|&(i, j)| NEIGHBOURS.map(|(di, dj)| index.get(&(i + di, j + dj)).copied()))
            .collect();
        let grounds: Vec<Ground> = keys.iter().map(|&k| self.ground(k, seed, coasts, outlines)).collect();
        let elevation: Vec<f64> = grounds.iter().map(|g| g.surface).collect();
        let age: Vec<f64> = grounds.iter().map(|g| g.age).collect();
        let erodibility: Vec<f64> = grounds.iter().map(|g| g.erodibility).collect();

        let mut ground = elevation.clone();
        let mut routing = Self::route_over(keys.clone(), owned.clone(), index.clone(), &nbrs, ground.clone());
        let mut cut_sills: HashSet<usize> = HashSet::new();
        for _ in 0..ROUTING_PASSES {
            let Some(breached) = Self::breach(&routing, &mut cut_sills, &age, &erodibility) else { break };
            ground = breached;
            routing = Self::route_over(keys.clone(), owned.clone(), index.clone(), &nbrs, ground.clone());
        }
        routing.cut = elevation.iter().zip(&ground).map(|(e, g)| e - g).collect();
        // A cut node is a fixed floor, so base levels are read again with
        // the cuts known.
        routing.base = Self::base_levels(&routing.down, &routing.kind, &routing.cut, &ground);
        routing.elevation = elevation;
        routing.age = age;
        routing.erodibility = erodibility;
        routing.floor = Self::floors(&routing);
        routing
    }

    /// The floor at every node: its own by [`floor_at`], and never below
    /// the floor of the land node its water goes to next, so a river above
    /// a harder lip is held up to the lip and its floor never rises along
    /// a reach. Walked from each node to the sea once, memoised.
    fn floors(routing: &Routing) -> Vec<f64> {
        let n = routing.keys.len();
        let sills: HashSet<usize> = routing.basins.iter().filter_map(|b| b.outlet).collect();
        let own: Vec<f64> = (0..n)
            .map(|k| {
                floor_at(
                    routing.elevation[k],
                    routing.base[k],
                    routing.carried[k],
                    routing.age[k],
                    routing.erodibility[k],
                    routing.cut[k],
                    sills.contains(&k),
                )
            })
            .collect();
        let mut floor: Vec<Option<f64>> = vec![None; n];
        for k in 0..n {
            if floor[k].is_some() {
                continue;
            }
            let mut path = vec![k];
            let mut level = f64::NEG_INFINITY;
            loop {
                let cur = *path.last().unwrap();
                if let Some(f) = floor[cur] {
                    level = f;
                    path.pop();
                    break;
                }
                // The lip rule runs down a reach, and a reach ends at the
                // pit: the pit's own link, to the sill its water leaves
                // over, is not walked.
                match routing.down[cur] {
                    Some(d) if matches!(routing.kind[d], Kind::Land | Kind::Basin) && !(routing.kind[cur] == Kind::Basin && routing.kind[d] != Kind::Basin) => path.push(d),
                    _ => break,
                }
            }
            for &p in path.iter().rev() {
                level = own[p].max(level);
                floor[p] = Some(level);
            }
        }
        floor.into_iter().zip(own).map(|(f, o)| f.unwrap_or(o)).collect()
    }

    /// The ground with every spilling basin's sill cut and the breach its
    /// outflow has made: the sill by its plate's age and the catchment
    /// leaving over it, no deeper than the basin behind it or the base level
    /// below it; every hump on the flood's path across the basin cut to the
    /// sill; and the ground down the outflow graded from the sill to the
    /// first node no higher. A sill in `cut_sills`, cut by an earlier pass,
    /// is not cut again and every sill cut joins it: one cut per basin, so a
    /// pass cuts only the sills of the sub-basins the pass before uncovered.
    /// None when no sill is cut.
    fn breach(routing: &Routing, cut_sills: &mut HashSet<usize>, age: &[f64], erodibility: &[f64]) -> Option<Vec<f64>> {
        let mut ground = routing.elevation.clone();
        let mut cut_any = false;
        for (id, basin) in routing.basins.iter().enumerate() {
            let Some(sill) = basin.outlet.filter(|s| !cut_sills.contains(s)) else { continue };
            let floor = basin.members.iter().map(|&m| routing.elevation[m]).fold(f64::MAX, f64::min);
            let room = (basin.surface - floor).min(basin.surface - routing.base[sill]);
            let cut = room * sill_share(age[sill], routing.catchment[sill], erodibility[sill]);
            if cut <= FLAT {
                continue;
            }
            let cut = if room - cut < REMNANT_MIN { room } else { cut };
            let target = basin.surface - cut;
            // The flood's path from the deepest node back to the sill, walked
            // within the basin: at a tied rim it leaves by the other rim
            // node. A hump on it goes a hair under the fill, so the basin
            // stays one basin with its sill where the flood entered. A basin
            // cut to its floor is no basin: its bed is cut level with the
            // sill, a flat the river crosses, not a sliver a hair deep.
            let under = if target > floor + FLAT { UNDER } else { 0.0 };
            let deepest = basin.members.iter().copied().min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b]));
            let mut cur = deepest;
            while let Some(k) = cur.filter(|&k| routing.basin_of[k] == Some(id)) {
                if ground[k] > target - under {
                    ground[k] = target - under;
                }
                cur = routing.parent[k];
            }
            // Beyond the sill the breach grades down to the first ground no
            // higher than the cut sill, so the river leaves over a lip and
            // falls, and a flat floor at the fill never stands past it.
            ground[sill] = target;
            cut_sills.insert(sill);
            let mut ramp = Vec::new();
            let mut cur = routing.down[sill];
            while let Some(k) = cur.filter(|&k| ground[k] > target) {
                ramp.push(k);
                cur = routing.down[k];
            }
            let end = cur.map_or(target, |e| ground[e]);
            for (i, &k) in ramp.iter().enumerate() {
                ground[k] = ground[k].min(target + (end - target) * (i + 1) as f64 / (ramp.len() + 1) as f64);
            }
            cut_any = true;
        }
        cut_any.then_some(ground)
    }

    /// Base level at each node: what a river there cuts toward and never
    /// below. Down each node's larger share, the first fixed level: the
    /// pit of closed ground, the lowest node the river reaching it runs
    /// down to; a cut node's ground, since the breach holds its floor and
    /// a tributary joining it can cut no lower; or the sea; water leaving
    /// the window is read as reaching the sea. A node's own floor is not
    /// its base.
    fn base_levels(down: &[Option<usize>], kind: &[Kind], cut: &[f64], ground: &[f64]) -> Vec<f64> {
        let n = down.len();
        let mut base: Vec<Option<f64>> = vec![None; n];
        for k in 0..n {
            if base[k].is_some() {
                continue;
            }
            let mut path = vec![k];
            let level = loop {
                let cur = *path.last().unwrap();
                if cur != k && cut[cur] > 0.0 {
                    // Its own base lies further down: it is not on this path.
                    path.pop();
                    break ground[cur];
                }
                if let Some(b) = base[cur] {
                    break b;
                }
                // A flooded node whose water leaves closed ground is the
                // pit; the others run on down to it.
                if kind[cur] == Kind::Basin && down[cur].map_or(true, |d| kind[d] != Kind::Basin) {
                    break ground[cur];
                }
                match down[cur] {
                    Some(d) if !matches!(kind[d], Kind::Sea | Kind::Edge) => path.push(d),
                    _ => break 0.0,
                }
            };
            for p in path {
                base[p] = Some(level);
            }
        }
        base.into_iter().map(|b| b.unwrap_or(0.0)).collect()
    }

    /// Route the window's nodes over `elevation`: the flood, every node's
    /// outflow, catchment, basins, base level and reaches. Publishes no
    /// age, no cut and no floor; `route` sets them.
    fn route_over(
        keys: Vec<NodeKey>,
        owned: Vec<bool>,
        index: HashMap<NodeKey, usize>,
        nbrs: &[[Option<usize>; 6]],
        elevation: Vec<f64>,
    ) -> Routing {
        let n = keys.len();

        let mut kind: Vec<Kind> = (0..n)
            .map(|k| {
                if elevation[k] < 0.0 {
                    Kind::Sea
                } else if nbrs[k].iter().any(|m| m.is_none()) {
                    Kind::Edge
                } else {
                    Kind::Land
                }
            })
            .collect();

        // ── Priority flood: the sea and the window's edge are the sinks ──
        let mut surface = elevation.clone();
        let mut parent: Vec<Option<usize>> = vec![None; n];
        let mut visited = vec![false; n];
        let mut heap = BinaryHeap::new();
        for k in 0..n {
            if kind[k] != Kind::Land {
                visited[k] = true;
                heap.push(Pending { level: elevation[k], node: k });
            }
        }
        while let Some(Pending { level, node }) = heap.pop() {
            for m in nbrs[node].into_iter().flatten() {
                if visited[m] {
                    continue;
                }
                visited[m] = true;
                let lvl = elevation[m].max(level);
                surface[m] = lvl;
                parent[m] = Some(node);
                heap.push(Pending { level: lvl, node: m });
            }
        }
        for k in 0..n {
            if kind[k] == Kind::Land && surface[k] > elevation[k] + FLAT {
                kind[k] = Kind::Basin;
            }
        }

        // ── Outflow: the true downslope of the water surface, split between the
        //    two neighbours bracketing it; flats follow the flood ──
        let site: Vec<(f64, f64)> = keys.iter().map(|&k| site_world(k)).collect();
        let unit = |from: usize, to: usize| -> (f64, f64) {
            let (dx, dy) = (site[to].0 - site[from].0, site[to].1 - site[from].1);
            let len = dx.hypot(dy);
            (dx / len, dy / len)
        };
        let mut direction: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
        let mut flow: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for k in 0..n {
            match kind[k] {
                Kind::Sea | Kind::Edge => {}
                Kind::Basin => {
                    // Over closed ground the water runs down the ground
                    // itself: a neighbour lower than a flooded node is under
                    // the same fill, so this never leaves the basin. Where
                    // none is lower the node is the pit, and its water
                    // leaves over the rim node the flood came in by, the
                    // sill: one link, so the flow never loops through the
                    // basin.
                    let lower = |m: usize| elevation[m] < elevation[k] - FLAT;
                    let facet = steepest_facet(&nbrs[k], k, &site, &elevation);
                    match facet {
                        Some((angle, a, b, share_next)) if lower(a) || lower(b) => {
                            direction[k] = (angle.cos(), angle.sin());
                            match (lower(a), lower(b)) {
                                (true, true) => {
                                    flow[k].push((a, 1.0 - share_next));
                                    flow[k].push((b, share_next));
                                }
                                (true, false) => flow[k].push((a, 1.0)),
                                _ => flow[k].push((b, 1.0)),
                            }
                        }
                        _ => {
                            let lowest = nbrs[k].into_iter().flatten().filter(|&m| lower(m)).min_by(|&a, &b| elevation[a].total_cmp(&elevation[b]));
                            match lowest {
                                Some(m) => {
                                    direction[k] = unit(k, m);
                                    flow[k].push((m, 1.0));
                                }
                                None => {
                                    let mut exit = parent[k];
                                    while let Some(x) = exit.filter(|&x| kind[x] == Kind::Basin) {
                                        exit = parent[x];
                                    }
                                    if let Some(p) = parent[k] {
                                        direction[k] = unit(k, p);
                                    }
                                    if let Some(x) = exit.filter(|&x| kind[x] != Kind::Edge) {
                                        flow[k].push((x, 1.0));
                                    }
                                }
                            }
                        }
                    }
                }
                Kind::Land => {
                    let facet = steepest_facet(&nbrs[k], k, &site, &surface);
                    match facet {
                        Some((angle, a, b, share_next)) => {
                            direction[k] = (angle.cos(), angle.sin());
                            // Only a lower neighbour may receive: the facet
                            // descends between them, but a share sent uphill
                            // would loop.
                            let lower = |m: usize| surface[m] < surface[k] - FLAT;
                            match (lower(a), lower(b)) {
                                (true, true) => {
                                    flow[k].push((a, 1.0 - share_next));
                                    flow[k].push((b, share_next));
                                }
                                (true, false) => flow[k].push((a, 1.0)),
                                (false, true) => flow[k].push((b, 1.0)),
                                (false, false) => {
                                    if let Some(p) = parent[k] {
                                        flow[k].push((p, 1.0));
                                    }
                                }
                            }
                        }
                        None => {
                            if let Some(p) = parent[k] {
                                direction[k] = unit(k, p);
                                flow[k].push((p, 1.0));
                            }
                        }
                    }
                }
            }
        }
        let down: Vec<Option<usize>> = flow
            .iter()
            .map(|f| {
                f.iter()
                    .fold(None, |best: Option<(usize, f64)>, &(m, s)| match best {
                        Some((_, bs)) if bs >= s => best,
                        _ => Some((m, s)),
                    })
                    .map(|(m, _)| m)
            })
            .collect();

        // ── Catchment: every node counts itself, then hands its water down ──
        let mut remaining = vec![0u32; n];
        for f in &flow {
            for &(d, _) in f {
                remaining[d] += 1;
            }
        }
        let mut catchment = vec![1.0f64; n];
        let mut queue: VecDeque<usize> = (0..n).filter(|&k| remaining[k] == 0).collect();
        while let Some(k) = queue.pop_front() {
            for &(d, share) in &flow[k] {
                catchment[d] += share * catchment[k];
                remaining[d] -= 1;
                if remaining[d] == 0 {
                    queue.push_back(d);
                }
            }
        }

        // ── Basins: flooded nodes at one fill, connected ──
        let mut basin_of: Vec<Option<usize>> = vec![None; n];
        let mut basins: Vec<RoutedBasin> = Vec::new();
        for k in 0..n {
            if kind[k] != Kind::Basin || basin_of[k].is_some() {
                continue;
            }
            let id = basins.len();
            let mut members = Vec::new();
            let mut stack = vec![k];
            basin_of[k] = Some(id);
            while let Some(a) = stack.pop() {
                members.push(a);
                for m in nbrs[a].into_iter().flatten() {
                    if kind[m] == Kind::Basin && basin_of[m].is_none() && (surface[m] - surface[a]).abs() < FLAT {
                        basin_of[m] = Some(id);
                        stack.push(m);
                    }
                }
            }
            members.sort_unstable();
            let outlet = members
                .iter()
                .filter_map(|&a| parent[a])
                .find(|&p| kind[p] != Kind::Basin)
                .filter(|&p| kind[p] != Kind::Edge);
            basins.push(RoutedBasin { members, surface: surface[k], outlet });
        }

        let base = Self::base_levels(&down, &kind, &vec![0.0; n], &elevation);

        // ── Reaches: a head is a source, a confluence, or a basin's sill;
        //    a reach runs through closed ground to its pit and ends there ──
        let pit = |k: usize| kind[k] == Kind::Basin && down[k].map_or(true, |d| kind[d] != Kind::Basin);
        let mut inflow = vec![0u32; n];
        let mut from_pit = vec![false; n];
        for k in 0..n {
            if let Some(d) = down[k] {
                match kind[k] {
                    Kind::Basin if pit(k) => from_pit[d] = true,
                    Kind::Land | Kind::Basin => inflow[d] += 1,
                    _ => {}
                }
            }
        }
        let head = |k: usize| matches!(kind[k], Kind::Land | Kind::Basin) && (inflow[k] != 1 || from_pit[k]);
        let mut reaches = Vec::new();
        for k in 0..n {
            if !head(k) {
                continue;
            }
            let mut nodes = vec![k];
            let mut cur = k;
            let end = loop {
                if pit(cur) {
                    break Terminus::Basin;
                }
                let Some(d) = down[cur] else { break Terminus::Edge };
                match kind[d] {
                    Kind::Sea => break Terminus::Sea,
                    Kind::Edge => break Terminus::Edge,
                    Kind::Land | Kind::Basin => {
                        if head(d) {
                            break Terminus::Continues;
                        }
                        nodes.push(d);
                        cur = d;
                    }
                }
            };
            reaches.push(RoutedReach { nodes, end });
        }
        // ── The catchment a reach's channel is cut by never falls along it:
        //    water that split away at a node cut the channel above, and the
        //    river below is one river, so a channel is cut by the most water
        //    that has run down it ──
        let mut carried = catchment.clone();
        for reach in &reaches {
            let mut most = 0.0f64;
            for &k in &reach.nodes {
                most = most.max(catchment[k]);
                carried[k] = most;
            }
        }

        Routing {
            keys,
            owned,
            elevation,
            surface,
            kind,
            direction,
            flow,
            down,
            catchment,
            carried,
            base,
            basin_of,
            parent,
            index,
            basins,
            reaches,
            age: vec![0.0; n],
            erodibility: vec![1.0; n],
            cut: vec![0.0; n],
            floor: vec![0.0; n],
        }
    }
}

impl Default for DrainageEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldEvent for DrainageEvent {
    fn name(&self) -> &str { "drainage" }
    fn scale(&self) -> u32 { DRAINAGE_CELL_SCALE }

    /// A node publishes its own downstream link, which reaches one spacing
    /// and the swing of both sites.
    fn max_influence(&self) -> u32 { (NODE_SPACING as f64 + 2.0 * NODE_SWING).ceil() as u32 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<DrainageIndex>();
    }

    /// The one index read beneath: the thrusting fronts over this cell's
    /// window, which complete the surface the nodes are evaluated on. The
    /// cells read are the fronts' cells under the window plus one ring, so a
    /// node's nearest front is in reach from every window that holds the
    /// node, and its elevation is the same in all of them.
    fn deform(&self, scope: &CellScope) {
        let outlines = outlines_of(scope);
        let edges = scope.read::<PlateEdgeIndex>();
        let coasts = Coasts::new(edges.iter().flat_map(|idx| idx.entries().flatten()), scope.seed());
        let routing = self.route(scope.lattice(), scope.cell(), scope.seed(), &coasts, &outlines);
        scope.publish::<DrainageIndex>(routing.owned_cell());
    }

    /// Nothing per tile. Every visible consequence is cut by dissection or
    /// drawn by the water layer, both reading the index.
    fn query(
        &self,
        _q: i32,
        _r: i32,
        _below: &TileView,
        _cell: &(dyn Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The share starts at the head, grows with catchment, and saturates.
    #[test]
    fn share_starts_at_the_head_and_saturates() {
        assert_eq!(relief_share(CHANNEL_HEAD, 1.0, 1.0), 0.0);
        assert_eq!(relief_share(1.0, 1.0, 1.0), 0.0);
        let mut last = 0.0;
        for i in 0..200 {
            let a = CHANNEL_HEAD + i as f64 * 0.5;
            let g = relief_share(a, 1.0, 1.0);
            assert!(g >= last && g <= RELIEF_SHARE_MAX, "share {g} at {a}");
            last = g;
        }
        assert!((relief_share(CATCHMENT_FULL, 1.0, 1.0) - RELIEF_SHARE_MAX).abs() < 1e-12);
        assert_eq!(relief_share(10.0 * CATCHMENT_FULL, 1.0, 1.0), RELIEF_SHARE_MAX);
    }

    /// The share grows with the plate's age, from a young plate's share of
    /// the full cut to the whole of it, at every catchment past the head.
    #[test]
    fn share_grows_with_age() {
        for c in [CHANNEL_HEAD + 1.0, CATCHMENT_FULL / 2.0, CATCHMENT_FULL] {
            let (young, old) = (relief_share(c, 0.0, 1.0), relief_share(c, 1.0, 1.0));
            assert!(young > 0.0 && young < old, "share {young} young, {old} old at {c}");
            assert!((young - YOUNG_SHARE * old).abs() < 1e-12);
            let mut last = young;
            for i in 1..=10 {
                let s = relief_share(c, i as f64 / 10.0, 1.0);
                assert!(s >= last, "share falls with age at {c}");
                last = s;
            }
        }
    }

    /// Hard rock moves the channel head out and cuts less past it: at a
    /// catchment shale channels, basement does not; at a trunk's, basement
    /// has cut a third of shale's share.
    #[test]
    fn hard_rock_channels_later_and_cuts_less() {
        assert!(head_on(0.3) > head_on(1.0));
        assert!(growth(CHANNEL_HEAD + 1.0, 1.0).is_some() && growth(CHANNEL_HEAD + 1.0, 0.3).is_none());
        let (soft, hard) = (relief_share(10.0 * CATCHMENT_FULL, 1.0, 1.0), relief_share(10.0 * CATCHMENT_FULL, 1.0, 0.3));
        assert!((hard - 0.3 * soft).abs() < 1e-12, "hard {hard} for soft {soft}");
        assert!(sill_share(1.0, CATCHMENT_FULL, 0.3) < sill_share(1.0, CATCHMENT_FULL, 1.0));
    }

    /// A node's floor is the routed ground where a sill or a breach was
    /// cut, and the envelope less its share of the height above base
    /// elsewhere, never below base.
    #[test]
    fn the_floor_is_the_cut_or_the_share() {
        assert_eq!(floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 7.0, true), 13.0);
        assert_eq!(floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 7.0, false), 13.0);
        let floor = floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 0.0, false);
        assert!((floor - (20.0 - 15.0 * RELIEF_SHARE_MAX)).abs() < 1e-12);
        assert_eq!(floor_at(20.0, 5.0, CHANNEL_HEAD, 1.0, 1.0, 0.0, false), 20.0);
        assert_eq!(floor_at(3.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 0.0, false), 3.0);
        let hard = floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 0.3, 0.0, false);
        assert!(hard > floor, "hard rock cut as deep as shale");
    }
}
