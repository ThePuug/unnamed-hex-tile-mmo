//! DrainageEvent — where water runs on the tectonic surface.
//!
//! Routes water over the composed surface on a coarse world-wide lattice of
//! nodes, and publishes the channel network of every landmass as reaches with
//! catchment, and the lakes that closed ground fills. It moves no ground and
//! its query returns nothing: dissection cuts along what this publishes.
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
//! # The sill and the plate's age
//!
//! Lakes are geologically brief: the river leaving a lake cuts its sill, so
//! a young orogen is lake country and an old one is drained through gorges.
//! A plate carries an age, and each spilling lake's sill is cut by that age
//! and the square root of the catchment leaving over it, the growth law a
//! valley's depth follows, no deeper than the basin behind it or the base
//! level beneath it. The ground down the outflow from the sill is graded
//! from the cut sill to the first ground no higher, the breach through the
//! rim, so the river leaves over a lip and falls. Across the drained floor
//! the river runs the flood's path from the deepest node to the sill, and
//! every hump on it goes a hair under the surface, so the floor drains as
//! one lake instead of a chain of sub-lakes; a pocket off that path is a
//! basin of its own. A lake left shallower than its river's channel drains
//! outright: the river runs through the lakebed. The window is routed again
//! over the cut ground while a routing cuts any sill, so shorelines, base
//! levels and the reach leaving each lake agree with every cut, and a sill
//! cut once is not cut again: one cut per basin. A lake spilling past the
//! window has no outlet and no cut. Nothing here moves ground: the envelope
//! is published as the elevation and the breach as a cut beside it, and
//! dissection removes the ground.
//!
//! # The rim between the nodes
//!
//! A range's crest lies between nodes by design, so a lake's rim may too:
//! the nodes can hold a lake hundreds of levels above a pass none of them
//! samples, and its water would hang over ground that runs away beneath it.
//! So each lake deep enough to matter is read on a lattice ten times finer:
//! from its deepest node, the way over the lowest ground reaches every
//! point the water could stand on, and the water is away when it reaches
//! the cell of a node lower than the way's highest point that does not
//! drain back, or the sea. Then the lake spills there: a chain of nodes
//! along the way, each the lowest that keeps the chain joined, is lowered to
//! that height, and the window is routed again. The way is closed when it
//! can go no further under the surface, and the lake stands as the nodes
//! hold it. Either way the points the way reached under the lake's level
//! are its extent, published with it: where its water stands at the tile
//! level. Only the ground the way visits is read, memoised across windows,
//! so a long arm of a basin costs its length and a leak its corridor.

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
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};
use crate::lattice::{hex_distance, DIRECTIONS as NEIGHBOURS};
pub use crate::lattice::{node_tile, NodeKey, NODE_SPACING};
pub use crate::tectonic::{aged, YOUNG_SHARE};
use crate::tectonic::PLATE_REACH;
use crate::{hex_to_world, substrate_on};

// ── Constants ───────────────────────────────────────────────────────────────

/// Cell scale, derived: one ring covers a plate's reach from its seed, so a
/// basin that drains a plate's interior to its coast is counted whole.
pub const DRAINAGE_CELL_SCALE: u32 = (PLATE_REACH / RING_CLEARANCE) as u32 + 1;

/// Two water levels closer than this are one flat.
const FLAT: f64 = 1e-9;

const SIXTY: f64 = std::f64::consts::PI / 3.0;

/// How many times a window is routed again over its cut sills. A drained
/// basin's floor holds sub-basins, each a lake under the same rule, so each
/// pass cuts the sills the pass before uncovered; the ground only falls, so
/// the passes converge, and the cap bounds the cost of a deep nest.
const ROUTING_PASSES: usize = 8;

/// Points per node spacing along each lattice axis of the fine lattice a
/// lake's rim is read on: its pitch resolves a pass the nodes miss, since a
/// range's crest lies between nodes by design.
///
/// EMPIRICAL: ten; at eight the trough joining a lake to its pass ran
/// diagonally between points and the flood went over a saddle instead.
const FINE: i32 = 10;

/// The most fine points one lake's way out is followed over before it is
/// taken as closed: a basin's whole floor under its surface, for a lake as
/// wide as the lattice holds.
const FINE_BUDGET: usize = 12_000;

/// A point of the fine lattice in world space: the node lattice is linear
/// in its keys, so a fine key is a node key scaled.
pub fn fine_world(key: NodeKey) -> (f64, f64) {
    let o = crate::lattice::node_world((0, 0));
    let (ax, ay) = crate::lattice::node_world((1, 0));
    let (bx, by) = crate::lattice::node_world((0, 1));
    let (u, v) = (key.0 as f64 / FINE as f64, key.1 as f64 / FINE as f64);
    (o.0 + (ax - o.0) * u + (bx - o.0) * v, o.1 + (ay - o.1) * u + (by - o.1) * v)
}

/// The fine point nearest a world position, keyed as [`fine_world`] reads
/// it: the node lattice's rounding at the fine pitch.
pub fn nearest_fine(wx: f64, wy: f64) -> NodeKey {
    let (q, r) = crate::world_to_hex(wx, wy);
    let s = NODE_SPACING as f64 / FINE as f64;
    crate::lattice::hex_round(q as f64 / s, r as f64 / s)
}

/// The least depth a lake keeps once its sill is cut, in z-levels: a
/// river's channel. A lake shallower than the channel the river leaving it
/// has cut is no lake, the river runs through the lakebed, so the sill is
/// cut to the floor instead of leaving a sliver of water the lattice cannot
/// shore.
pub const REMNANT_MIN: f64 = 3.0;

/// How far under its surface a hump on the flood's path across a lake is
/// cut: a hair, enough that the routing floods it and the lake stays one
/// lake, and too little for a tile to read as water.
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
/// aged, and what an aged plate keeps are the lakes that drain little.
///
/// EMPIRICAL: set with `drainage_probe::lakes_in_the_troughs`.
pub const SILL_CUT_RATE: f64 = 1.5;

/// The share of the basin behind it a sill is cut by, with `catchment`
/// nodes of water leaving over it on a plate of `age` through rock of
/// `erodibility`: the river's growth past the channel head at
/// [`SILL_CUT_RATE`] per unit of age and erodibility, saturating at the
/// whole. Nothing below the head: a pit that drains little keeps its lake
/// at any age, and a hard sill holds its lake longer.
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
/// on flooded ground the lakebed, which lies below its base and is not
/// cut; else the envelope less its share of the height above base level.
/// A river is held up by a harder lip downstream, which [`Routing`]
/// settles over the reach: the floor published is never below the next
/// floor downstream. Dissection cuts to it and never below; a channel
/// entering a lake grades to the lake's surface, its base, and never to
/// the bed.
pub fn floor_at(elevation: f64, base: f64, catchment: f64, age: f64, erodibility: f64, cut: f64, sill: bool, flooded: bool) -> f64 {
    if sill || cut > 0.0 {
        return elevation - cut;
    }
    if flooded {
        return elevation;
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


/// The steepest of the six facets around a node on the water surface: the
/// downslope angle in world space, the index of the facet's first edge, and
/// the share of the flow its second edge takes. None where nothing descends.
///
/// A facet is the plane through the node and two consecutive neighbours. Its
/// steepest descent is used when it points into the facet; when it points
/// outside, the flow runs down the nearer edge instead, at that edge's slope.
fn steepest_facet(
    nbrs: &[Option<usize>; 6],
    k: usize,
    surface: &[f64],
) -> Option<(f64, usize, f64)> {
    use std::f64::consts::PI;
    let s = NODE_SPACING as f64;
    let z0 = surface[k];
    let mut best: Option<(f64, f64, usize, f64)> = None;
    for e in 0..6 {
        let (Some(a), Some(b)) = (nbrs[e], nbrs[(e + 1) % 6]) else { continue };
        let (za, zb) = ((surface[a] - z0) / s, (surface[b] - z0) / s);
        let ta = e as f64 * SIXTY;
        let tb = ta + SIXTY;
        // Gradient g of the plane with e_a·g = za and e_b·g = zb.
        let (ca, sa, cb, sb) = (ta.cos(), ta.sin(), tb.cos(), tb.sin());
        let det = ca * sb - sa * cb;
        let gx = (za * sb - zb * sa) / det;
        let gy = (ca * zb - cb * za) / det;
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
            slope = -za;
        } else if rho > SIXTY {
            rho = SIXTY;
            angle = tb;
            slope = -zb;
        }
        if slope <= 0.0 {
            continue;
        }
        if best.map_or(true, |(bs, ..)| slope > bs) {
            best = Some((slope, angle, e, rho / SIXTY));
        }
    }
    best.map(|(_, angle, edge, share)| (angle, edge, share))
}

// ── What a cell publishes ───────────────────────────────────────────────────

/// Where a reach's last node sends its water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminus {
    /// Into another reach: a confluence, or the same channel in the next cell.
    Continues,
    Sea,
    Lake,
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
    /// Ground, in z-levels: the envelope, before any cut.
    pub elevation: f64,
    /// Water: the ground, or the lake surface where the ground is flooded.
    pub surface: f64,
    /// The true downslope at this node, a unit vector in world space. Across a
    /// lake it points along the flood's path to the outlet.
    pub direction: (f64, f64),
    /// Water draining through this one, itself included, in nodes, fractional
    /// because a node's water splits, and never less than at any node above
    /// it on its reach: what its channel is cut by.
    pub catchment: f64,
    /// Base level, in z-levels: the surface of the first lake down the larger
    /// share's path, or sea level. What a river here cuts toward and never
    /// below.
    pub base: f64,
    /// The node taking the larger share of this one's water. None at a sink.
    pub down: Option<NodeKey>,
    /// Index into the cell's lakes when this node is flooded.
    pub lake: Option<usize>,
    /// This node is a lake's outlet: the sill whose height, less its cut, is
    /// the lake's surface. Dissection cuts exactly `cut` here, so the lake
    /// keeps the surface the routing gave it.
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
    /// The node the last one drains to. None at a sink.
    pub joins: Option<NodeKey>,
    pub end: Terminus,
}

#[derive(Clone, Debug)]
pub struct Lake {
    /// The flooded nodes this cell owns. A lake crossing a cell boundary is
    /// published by every cell that owns part of it, at the same surface.
    pub nodes: Vec<NodeKey>,
    pub surface: f64,
    /// The rim node water leaves over. None when the spill lies beyond the
    /// window: a lake with no outlet.
    pub outlet: Option<NodeKey>,
    /// The fine points the lake's water stands over, what the fine lattice
    /// reached under the surface from the deepest node, keyed as
    /// [`nearest_fine`] keys them: the lake's extent at the tile level,
    /// which the flood holds to. Empty when the lake was not read on it.
    pub extent: Vec<NodeKey>,
}

#[derive(Clone, Debug, Default)]
pub struct DrainageCell {
    pub nodes: HashMap<NodeKey, DrainageNode>,
    pub reaches: Vec<Reach>,
    pub lakes: Vec<Lake>,
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
        let (q, r) = node_tile(key);
        self.cells.get(&Self::lattice().cell_id(q, r))?.nodes.get(&key)
    }

    /// The published cells among `cell_ids`: what a reader gathers over its
    /// footprint and ring.
    pub fn cells_in(&self, cell_ids: &[CellId]) -> Vec<&DrainageCell> {
        cell_ids.iter().filter_map(|id| self.cells.get(id)).collect()
    }
}

impl CellIndex for DrainageIndex {
    type Cell = DrainageCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
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
        if q % NODE_SPACING != 0 || r % NODE_SPACING != 0 {
            return Vec::new();
        }
        self.node((q / NODE_SPACING, r / NODE_SPACING))
            .and_then(|n| n.down)
            .map(|d| vec![node_tile(d)])
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
    /// Land under a lake surface.
    Lake,
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

/// Min-heap entry for the fine flood, as [`Pending`] keyed by point.
#[derive(PartialEq)]
struct FinePending {
    level: f64,
    point: NodeKey,
}

impl Eq for FinePending {}

impl Ord for FinePending {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .level
            .partial_cmp(&self.level)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.point.cmp(&self.point))
    }
}

impl PartialOrd for FinePending {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct RoutedLake {
    members: Vec<usize>,
    surface: f64,
    outlet: Option<usize>,
    extent: Vec<NodeKey>,
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
    /// Base level at each node: the first lake down the larger share's
    /// path, or sea level.
    pub base: Vec<f64>,
    pub lake_of: Vec<Option<usize>>,
    /// The node each one was flooded from: the flood's path, which across a
    /// lake is the lowest route from any flooded node to the outlet.
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
    lakes: Vec<RoutedLake>,
    reaches: Vec<RoutedReach>,
}

impl Routing {
    pub fn index_of(&self, key: NodeKey) -> Option<usize> {
        self.index.get(&key).copied()
    }

    /// The cell's own share of the window: its nodes, its runs of every
    /// reach, and its part of every lake.
    pub fn owned_cell(&self) -> DrainageCell {
        let mut lake_local: HashMap<usize, usize> = HashMap::new();
        let mut lakes = Vec::new();
        for (id, lake) in self.lakes.iter().enumerate() {
            let nodes: Vec<NodeKey> = lake
                .members
                .iter()
                .filter(|&&m| self.owned[m])
                .map(|&m| self.keys[m])
                .collect();
            if nodes.is_empty() {
                continue;
            }
            lake_local.insert(id, lakes.len());
            lakes.push(Lake {
                nodes,
                surface: lake.surface,
                outlet: lake.outlet.map(|o| self.keys[o]),
                extent: lake.extent.clone(),
            });
        }

        // Every lake in the window, not only the owned ones: an outlet is
        // owned by whichever cell holds the rim node, and that cell may own
        // no flooded node of the lake it drains.
        let sills: HashSet<usize> = self.lakes.iter().filter_map(|l| l.outlet).collect();
        let mut nodes = HashMap::new();
        for k in 0..self.keys.len() {
            if !self.owned[k] || matches!(self.kind[k], Kind::Sea | Kind::Edge) {
                continue;
            }
            let (q, r) = node_tile(self.keys[k]);
            let (wx, wy) = hex_to_world(q, r);
            let lake = self.lake_of[k].and_then(|id| lake_local.get(&id).copied());
            let sill = sills.contains(&k);
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
                    direction: self.direction[k],
                    catchment: self.carried[k],
                    base: self.base[k],
                    down: self.down[k].map(|d| self.keys[d]),
                    lake,
                    sill,
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
                    reaches.push(Reach {
                        nodes: run.iter().map(|&m| self.keys[m]).collect(),
                        joins: self.down[tail].map(|d| self.keys[d]),
                        end,
                    });
                    run.clear();
                }
            }
        }

        DrainageCell { nodes, reaches, lakes }
    }
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct DrainageEvent {
    /// The ground at each node, shared by every window that contains it.
    nodes: DashMap<NodeKey, Ground>,
    /// The envelope at points of the fine lattice, shared by every window.
    fine: DashMap<NodeKey, f64>,
}

impl DrainageEvent {
    pub fn new() -> Self {
        Self { nodes: DashMap::new(), fine: DashMap::new() }
    }

    /// The ground at a node.
    fn ground(&self, key: NodeKey, seed: u64, coasts: &Coasts, outlines: &Outlines) -> Ground {
        if let Some(g) = self.nodes.get(&key) {
            return *g;
        }
        let (q, r) = node_tile(key);
        let (wx, wy) = hex_to_world(q, r);
        let g = ground_at(wx, wy, seed, coasts, outlines);
        self.nodes.insert(key, g);
        g
    }

    /// The envelope at a point of the fine lattice, memoised.
    fn fine_ground(&self, key: NodeKey, seed: u64, coasts: &Coasts, outlines: &Outlines) -> f64 {
        if let Some(h) = self.fine.get(&key) {
            return *h;
        }
        let (x, y) = fine_world(key);
        let h = ground_at(x, y, seed, coasts, outlines).surface;
        self.fine.insert(key, h);
        h
    }

    /// Where a lake really spills, read on the fine lattice: from every
    /// node of it outward, since a ridge the nodes never sampled can part
    /// one member's basin from the rest and the water stands in both, the
    /// way over the lowest ground reaches every point the water could
    /// stand on, and the water is away when it
    /// reaches the cell of a node lower than the way's highest point that
    /// does not drain back into the lake, or the sea. The height of that
    /// highest point is the spill; the way is closed when it can go no
    /// further under the surface, and then the lake stands as the nodes
    /// hold it. Only the ground the way visits is read, so a long arm of
    /// the basin costs its length and a leak costs its corridor.
    ///
    /// Returns the leak, if any: the spill and the nodes whose cells the
    /// way crosses to it; and the extent, the fine points reached under
    /// the spill or, closed, under the surface: where the lake's water
    /// stands at the tile level. None for the extent when the way was too
    /// long to follow.
    fn fine_spill(
        &self,
        routing: &Routing,
        id: usize,
        seed: u64,
        coasts: &Coasts,
        outlines: &Outlines,
    ) -> (Option<(f64, Vec<usize>)>, Option<Vec<NodeKey>>) {
        let lake = &routing.lakes[id];
        if lake.members.is_empty() { return (None, None) }
        let mut reached: HashMap<NodeKey, f64> = HashMap::new();
        let mut parent: HashMap<NodeKey, NodeKey> = HashMap::new();
        let mut heap: BinaryHeap<FinePending> = BinaryHeap::new();
        for &m in &lake.members {
            let (i, j) = routing.keys[m];
            reached.insert((i * FINE, j * FINE), routing.elevation[m]);
            heap.push(FinePending { level: routing.elevation[m], point: (i * FINE, j * FINE) });
        }
        let drains_back = |mut k: usize| loop {
            if routing.lake_of[k] == Some(id) {
                return true;
            }
            match routing.down[k] {
                Some(d) if !matches!(routing.kind[d], Kind::Sea | Kind::Edge) => k = d,
                _ => return false,
            }
        };
        let extent = |reached: &HashMap<NodeKey, f64>, under: f64| -> Vec<NodeKey> {
            reached.iter().filter(|(_, &l)| l < under - FLAT).map(|(&p, _)| p).collect()
        };
        while let Some(FinePending { level, point }) = heap.pop() {
            if level > lake.surface - FLAT {
                return (None, Some(extent(&reached, lake.surface)));
            }
            if reached.len() > FINE_BUDGET {
                return (None, None);
            }
            if reached.get(&point).is_some_and(|&r| r < level) {
                continue;
            }
            let (x, y) = fine_world(point);
            let node = routing.index_of(crate::lattice::nearest_node(x, y));
            let away = match node {
                None => true,
                Some(k) => routing.lake_of[k] != Some(id) && (routing.kind[k] == Kind::Sea || (routing.elevation[k] <= level && !drains_back(k))),
            };
            if away {
                let mut crossed: Vec<usize> = Vec::new();
                let mut cur = point;
                loop {
                    let (x, y) = fine_world(cur);
                    if let Some(k) = routing.index_of(crate::lattice::nearest_node(x, y)) {
                        if routing.lake_of[k] != Some(id) && !crossed.contains(&k) {
                            crossed.push(k);
                        }
                    }
                    let Some(&p) = parent.get(&cur) else { break };
                    cur = p;
                }
                crossed.reverse();
                return (Some((level, crossed)), Some(extent(&reached, level)));
            }
            for (di, dj) in NEIGHBOURS {
                let n = (point.0 + di, point.1 + dj);
                let g = self.fine_ground(n, seed, coasts, outlines);
                let via = g.max(level);
                if reached.get(&n).is_some_and(|&r| r <= via) {
                    continue;
                }
                reached.insert(n, via);
                parent.insert(n, point);
                heap.push(FinePending { level: via, point: n });
            }
        }
        (None, Some(extent(&reached, lake.surface)))
    }

    /// Lower each lake's rim to where it really spills, read on the fine
    /// lattice, so a lake the nodes hold above a pass they do not sample
    /// falls to it and its water meets land. Along the way out, each cell
    /// crossed puts one node in a chain lowered to the spill: the lowest of
    /// the cell's node and its neighbours that keeps the chain joined, so a
    /// pass through a peak's corner is cut beside the peak and not through
    /// it. The chain ends beside the node the water is away at, which is
    /// lower already. Each lake's extent on the fine lattice is kept by its
    /// deepest node for publication. True when any rim was lowered; the
    /// window is then routed again.
    fn resolve_rims(
        &self,
        routing: &Routing,
        nbrs: &[[Option<usize>; 6]],
        ground: &mut [f64],
        held: &mut HashSet<(NodeKey, u64)>,
        extents: &mut HashMap<NodeKey, Vec<NodeKey>>,
        seed: u64,
        coasts: &Coasts,
        outlines: &Outlines,
    ) -> bool {
        let mut leaked = false;
        for (id, lake) in routing.lakes.iter().enumerate() {
            let Some(deepest) = lake.members.iter().copied().min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b])) else { continue };
            // A lake shallower than a channel hangs no deeper than that.
            if lake.surface - routing.elevation[deepest] < REMNANT_MIN {
                continue;
            }
            // A lake the fine lattice already held, same nodes at the same
            // surface, holds still: the envelope has not moved.
            let sign = (routing.keys[deepest], lake.surface.to_bits());
            if held.contains(&sign) {
                continue;
            }
            let (leak, extent) = self.fine_spill(routing, id, seed, coasts, outlines);
            match extent {
                Some(extent) => {
                    extents.insert(routing.keys[deepest], extent);
                }
                None => {
                    extents.remove(&routing.keys[deepest]);
                }
            }
            let Some((spill, crossed)) = leak else {
                held.insert(sign);
                continue;
            };
            let mut last: Option<usize> = None;
            for &k in &crossed {
                let joined = |n: usize| match last {
                    None => nbrs[n].into_iter().flatten().any(|m| routing.lake_of[m] == Some(id)),
                    Some(p) => p == n || nbrs[p].into_iter().flatten().any(|m| m == n),
                };
                let pick = std::iter::once(k)
                    .chain(nbrs[k].into_iter().flatten())
                    .filter(|&n| routing.lake_of[n] != Some(id) && joined(n))
                    .min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b]))
                    .unwrap_or(k);
                if ground[pick] > spill {
                    ground[pick] = spill;
                    leaked = true;
                }
                last = Some(pick);
            }
        }
        leaked
    }

    /// Route the window of `cell`: every node in the cell and its ring, on
    /// the surface the fronts in reach complete; then again over the ground
    /// with the breaches, while a routing cuts any sill, so shorelines, base
    /// levels and the reach leaving each lake agree with every cut sill.
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
                let tile = (i * s, j * s);
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
        let mut held: HashSet<(NodeKey, u64)> = HashSet::new();
        let mut extents: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        for _ in 0..ROUTING_PASSES {
            // Sills are cut first; the rims of the lakes that survive are
            // then read on the fine lattice, and a lake that falls is cut
            // no further: one cut per basin.
            let next = match Self::breach(&routing, &mut cut_sills, &age, &erodibility) {
                Some(breached) => breached,
                None => {
                    let mut next = ground.clone();
                    if !self.resolve_rims(&routing, &nbrs, &mut next, &mut held, &mut extents, seed, coasts, outlines) {
                        break;
                    }
                    next
                }
            };
            ground = next;
            routing = Self::route_over(keys.clone(), owned.clone(), index.clone(), &nbrs, ground.clone());
        }
        // A lake the fine lattice read stands over the extent it found. One
        // the last routing made, past the passes or by a breach, is read
        // now, its leak left unresolved: a stale extent under its deepest
        // node's key would be a smaller lake's, and its water would stand
        // short of its nodes.
        let mut fresh: Vec<(usize, Option<Vec<NodeKey>>)> = Vec::new();
        for (id, lake) in routing.lakes.iter().enumerate() {
            let Some(deepest) = lake.members.iter().copied().min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b])) else { continue };
            if lake.surface - routing.elevation[deepest] < REMNANT_MIN || held.contains(&(routing.keys[deepest], lake.surface.to_bits())) {
                continue;
            }
            fresh.push((id, self.fine_spill(&routing, id, seed, coasts, outlines).1));
        }
        for (id, extent) in fresh {
            let deepest = routing.lakes[id].members.iter().copied().min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b])).unwrap();
            match extent {
                Some(e) => { extents.insert(routing.keys[deepest], e); }
                None => { extents.remove(&routing.keys[deepest]); }
            }
        }
        for lake in routing.lakes.iter_mut() {
            let Some(deepest) = lake.members.iter().copied().min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b])) else { continue };
            lake.extent = extents.get(&routing.keys[deepest]).cloned().unwrap_or_default();
        }
        routing.cut = elevation.iter().zip(&ground).map(|(e, g)| e - g).collect();
        // A cut node is a fixed floor, so base levels are read again with
        // the cuts known.
        routing.base = Self::base_levels(&routing.down, &routing.kind, &routing.lake_of, &routing.lakes, &routing.cut, &ground);
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
        let sills: HashSet<usize> = routing.lakes.iter().filter_map(|l| l.outlet).collect();
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
                    routing.lake_of[k].is_some(),
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
                match routing.down[cur] {
                    Some(d) if routing.kind[d] == Kind::Land && routing.lake_of[cur].is_none() => path.push(d),
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

    /// The ground with every spilling lake's sill cut and the breach its
    /// outflow has made: the sill by its plate's age and the catchment
    /// leaving over it, no deeper than the basin behind it or the base level
    /// below it; every hump on the flood's path across the lake cut to the
    /// sill; and the ground down the outflow graded from the sill to the
    /// first node no higher. A sill in `cut_sills`, cut by an earlier pass,
    /// is not cut again and every sill cut joins it: one cut per basin, so a
    /// pass cuts only the sills of the sub-basins the pass before uncovered.
    /// None when no sill is cut.
    fn breach(routing: &Routing, cut_sills: &mut HashSet<usize>, age: &[f64], erodibility: &[f64]) -> Option<Vec<f64>> {
        let mut ground = routing.elevation.clone();
        let mut cut_any = false;
        for (id, lake) in routing.lakes.iter().enumerate() {
            let Some(sill) = lake.outlet.filter(|s| !cut_sills.contains(s)) else { continue };
            let floor = lake.members.iter().map(|&m| routing.elevation[m]).fold(f64::MAX, f64::min);
            let room = (lake.surface - floor).min(lake.surface - routing.base[sill]);
            let cut = room * sill_share(age[sill], routing.catchment[sill], erodibility[sill]);
            if cut <= FLAT {
                continue;
            }
            let cut = if room - cut < REMNANT_MIN { room } else { cut };
            let target = lake.surface - cut;
            // The flood's path from the deepest node back to the sill, walked
            // within the lake: at a tied rim it leaves by the other rim node.
            // A hump on it goes a hair under the surface, so the lake stays
            // one lake with its sill where the flood entered. A lake cut to
            // its floor is no lake: its bed is cut level with the sill, a
            // flat the river crosses, not a sliver a hair deep that no tile
            // reads as water but the routing floods.
            let under = if target > floor + FLAT { UNDER } else { 0.0 };
            let deepest = lake.members.iter().copied().min_by(|&a, &b| routing.elevation[a].total_cmp(&routing.elevation[b]));
            let mut cur = deepest;
            while let Some(k) = cur.filter(|&k| routing.lake_of[k] == Some(id)) {
                if ground[k] > target - under {
                    ground[k] = target - under;
                }
                cur = routing.parent[k];
            }
            // Beyond the sill the breach grades down to the first ground no
            // higher than the cut sill, so the river leaves over a lip and
            // falls, and a flat floor at lake level never stands past it.
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
    /// below. Down each node's larger share, the first fixed floor: a
    /// flooded node's lake surface, a cut node's ground, since the breach
    /// or the lowered rim holds its floor and a tributary joining it can
    /// cut no lower, or the sea; water leaving the window is read as
    /// reaching the sea. A node's own floor is not its base.
    fn base_levels(
        down: &[Option<usize>],
        kind: &[Kind],
        lake_of: &[Option<usize>],
        lakes: &[RoutedLake],
        cut: &[f64],
        ground: &[f64],
    ) -> Vec<f64> {
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
                if let Some(id) = lake_of[cur] {
                    break lakes[id].surface;
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
    /// outflow, catchment, lakes, base level and reaches. Publishes no age
    /// and no cut; `route` sets both.
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
                kind[k] = Kind::Lake;
            }
        }

        // ── Outflow: the true downslope of the water surface, split between the
        //    two neighbours bracketing it; flats follow the flood ──
        let unit = |from: usize, to: usize| -> (f64, f64) {
            let (fq, fr) = node_tile(keys[from]);
            let (tq, tr) = node_tile(keys[to]);
            let (fx, fy) = hex_to_world(fq, fr);
            let (tx, ty) = hex_to_world(tq, tr);
            let (dx, dy) = (tx - fx, ty - fy);
            let len = dx.hypot(dy);
            (dx / len, dy / len)
        };
        let mut direction: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
        let mut flow: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for k in 0..n {
            match kind[k] {
                Kind::Sea | Kind::Edge => {}
                Kind::Lake => {
                    if let Some(p) = parent[k] {
                        direction[k] = unit(k, p);
                        flow[k].push((p, 1.0));
                    }
                }
                Kind::Land => {
                    let facet = steepest_facet(&nbrs[k], k, &surface);
                    match facet {
                        Some((angle, edge, share_next)) => {
                            direction[k] = (angle.cos(), angle.sin());
                            let a = nbrs[k][edge].unwrap();
                            let b = nbrs[k][(edge + 1) % 6].unwrap();
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

        // ── Lakes: flooded nodes at one level, connected ──
        let mut lake_of: Vec<Option<usize>> = vec![None; n];
        let mut lakes: Vec<RoutedLake> = Vec::new();
        for k in 0..n {
            if kind[k] != Kind::Lake || lake_of[k].is_some() {
                continue;
            }
            let id = lakes.len();
            let mut members = Vec::new();
            let mut stack = vec![k];
            lake_of[k] = Some(id);
            while let Some(a) = stack.pop() {
                members.push(a);
                for m in nbrs[a].into_iter().flatten() {
                    if kind[m] == Kind::Lake
                        && lake_of[m].is_none()
                        && (surface[m] - surface[a]).abs() < FLAT
                    {
                        lake_of[m] = Some(id);
                        stack.push(m);
                    }
                }
            }
            members.sort_unstable();
            let outlet = members
                .iter()
                .filter_map(|&a| parent[a])
                .find(|&p| kind[p] != Kind::Lake)
                .filter(|&p| kind[p] != Kind::Edge);
            lakes.push(RoutedLake { members, surface: surface[k], outlet, extent: Vec::new() });
        }

        let base = Self::base_levels(&down, &kind, &lake_of, &lakes, &vec![0.0; n], &elevation);

        // ── Reaches: a head is a source, a confluence, or a lake outlet ──
        let mut land_in = vec![0u32; n];
        let mut lake_in = vec![false; n];
        for k in 0..n {
            if let Some(d) = down[k] {
                match kind[k] {
                    Kind::Land => land_in[d] += 1,
                    Kind::Lake => lake_in[d] = true,
                    _ => {}
                }
            }
        }
        let head = |k: usize| kind[k] == Kind::Land && (land_in[k] != 1 || lake_in[k]);
        let mut reaches = Vec::new();
        for k in 0..n {
            if !head(k) {
                continue;
            }
            let mut nodes = vec![k];
            let mut cur = k;
            let end = loop {
                let Some(d) = down[cur] else { break Terminus::Edge };
                match kind[d] {
                    Kind::Sea => break Terminus::Sea,
                    Kind::Edge => break Terminus::Edge,
                    Kind::Lake => break Terminus::Lake,
                    Kind::Land => {
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

        // ── The river across each lake: the flood's path from the deepest
        //    flooded node to the sill, a reach so the throat to the sill and
        //    any ridge between flooded nodes the lattice did not sample are
        //    cut, and a lake is one water at the tile level as at the node ──
        for (id, lake) in lakes.iter().enumerate() {
            let Some(deepest) = lake.members.iter().copied().min_by(|&a, &b| elevation[a].total_cmp(&elevation[b])) else { continue };
            let mut nodes = vec![deepest];
            let mut cur = deepest;
            while let Some(p) = parent[cur].filter(|&p| lake_of[p] == Some(id)) {
                nodes.push(p);
                cur = p;
            }
            let end = match parent[cur].map(|p| kind[p]) {
                Some(Kind::Land) => Terminus::Continues,
                Some(Kind::Sea) => Terminus::Sea,
                Some(Kind::Lake) => Terminus::Lake,
                Some(Kind::Edge) | None => Terminus::Edge,
            };
            reaches.push(RoutedReach { nodes, end });
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
            lake_of,
            parent,
            index,
            lakes,
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

    /// A node publishes its own downstream link, which reaches one spacing.
    fn max_influence(&self) -> u32 { NODE_SPACING as u32 }

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
        let edge_cells = scope.source_cells::<PlateEdgeIndex>();
        let coasts = Coasts::new(
            &scope.read::<PlateEdgeIndex>().map(|idx| idx.edges_in(&edge_cells)).unwrap_or_default(),
            scope.seed(),
        );
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
    /// has cut a third of shale's share; a hard sill holds its lake longer.
    #[test]
    fn hard_rock_channels_later_and_cuts_less() {
        assert!(head_on(0.3) > head_on(1.0));
        assert!(growth(CHANNEL_HEAD + 1.0, 1.0).is_some() && growth(CHANNEL_HEAD + 1.0, 0.3).is_none());
        let (soft, hard) = (relief_share(10.0 * CATCHMENT_FULL, 1.0, 1.0), relief_share(10.0 * CATCHMENT_FULL, 1.0, 0.3));
        assert!((hard - 0.3 * soft).abs() < 1e-12, "hard {hard} for soft {soft}");
        assert!(sill_share(1.0, CATCHMENT_FULL, 0.3) < sill_share(1.0, CATCHMENT_FULL, 1.0));
    }

    /// A node's floor is the routed ground where a sill or a breach was
    /// cut, the lakebed on flooded ground, and the envelope less its share
    /// of the height above base elsewhere, never below base.
    #[test]
    fn the_floor_is_the_cut_the_bed_or_the_share() {
        assert_eq!(floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 7.0, true, false), 13.0);
        assert_eq!(floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 7.0, false, false), 13.0);
        assert_eq!(floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 0.0, false, true), 20.0);
        let floor = floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 0.0, false, false);
        assert!((floor - (20.0 - 15.0 * RELIEF_SHARE_MAX)).abs() < 1e-12);
        assert_eq!(floor_at(20.0, 5.0, CHANNEL_HEAD, 1.0, 1.0, 0.0, false, false), 20.0);
        assert_eq!(floor_at(3.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, 0.0, false, false), 3.0);
        let hard = floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 0.3, 0.0, false, false);
        assert!(hard > floor, "hard rock cut as deep as shale");
    }
}
