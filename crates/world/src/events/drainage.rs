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
//! # Closed ground
//!
//! The flood fills every pit to its rim, so water routes over closed
//! ground toward the rim and out, and catchment carries through: a basin
//! swallows no river. But the flooded nodes carry no channel and no water:
//! a reach ends where it enters closed ground, and the rim's low node is
//! the head of the reach leaving, carrying the whole basin's water. What
//! stands in a basin, a lake and the sill its outlet cuts, is unbuilt: the
//! design that read each basin on a finer lattice to find its rim and its
//! shore scanned the basin's floor, and is gone.

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

/// Two levels closer than this are one flat.
const FLAT: f64 = 1e-9;

const SIXTY: f64 = std::f64::consts::PI / 3.0;

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

/// The floor a river has cut to at a node on its own, in z-levels: on
/// closed ground the ground itself, which lies under its base and carries
/// no channel; else the envelope less its share of the height above base
/// level. A river is held up by a harder lip downstream, which [`Routing`]
/// settles over the reach: the floor published is never below the next
/// floor downstream. Dissection cuts to it and never below.
pub fn floor_at(elevation: f64, base: f64, catchment: f64, age: f64, erodibility: f64, flooded: bool) -> f64 {
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
    /// Into closed ground, which carries no channel.
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
    /// The node lies on closed ground under the fill: it is routed over
    /// toward the rim and carries no channel.
    pub flooded: bool,
    /// The true downslope at this node, a unit vector in world space. Across
    /// closed ground it points along the flood's path to the rim.
    pub direction: (f64, f64),
    /// Water draining through this one, itself included, in nodes, fractional
    /// because a node's water splits, and never less than at any node above
    /// it on its reach: what its channel is cut by.
    pub catchment: f64,
    /// Base level, in z-levels: the fill of the first closed ground down
    /// the larger share's path, or sea level. What a river here cuts toward
    /// and never below.
    pub base: f64,
    /// The node taking the larger share of this one's water. None at a sink.
    pub down: Option<NodeKey>,
    /// The age of the plate this node stands on, 0 to 1 as
    /// `tectonic::Plate::age`.
    pub age: f64,
    /// The erodibility of the rock at this node, as `lithology::Rock`.
    pub erodibility: f64,
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
    /// Closed ground under the flood's fill: routed over the fill toward
    /// the rim, carrying no channel.
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

struct RoutedReach {
    nodes: Vec<usize>,
    end: Terminus,
}

/// Everything routed over one window. `deform` publishes the owned subset;
/// the probes read the whole to compare neighbouring windows.
pub struct Routing {
    pub keys: Vec<NodeKey>,
    pub owned: Vec<bool>,
    /// The envelope at each node.
    pub elevation: Vec<f64>,
    /// The flood's fill at each node: the envelope, or the level closed
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
    /// the larger share's path, or sea level.
    pub base: Vec<f64>,
    /// The node each one was flooded from: the flood's path, which across
    /// closed ground is the lowest route to the rim.
    pub parent: Vec<Option<usize>>,
    /// The plate's age at each node.
    pub age: Vec<f64>,
    /// The erodibility of the rock at each node.
    pub erodibility: Vec<f64>,
    /// The floor at each node, as `DrainageNode::floor`: its own, held up
    /// by any harder lip downstream.
    pub floor: Vec<f64>,
    index: HashMap<NodeKey, usize>,
    reaches: Vec<RoutedReach>,
}

impl Routing {
    pub fn index_of(&self, key: NodeKey) -> Option<usize> {
        self.index.get(&key).copied()
    }

    /// The cell's own share of the window: its nodes and its runs of every
    /// reach.
    pub fn owned_cell(&self) -> DrainageCell {
        let mut nodes = HashMap::new();
        for k in 0..self.keys.len() {
            if !self.owned[k] || matches!(self.kind[k], Kind::Sea | Kind::Edge) {
                continue;
            }
            let (q, r) = node_tile(self.keys[k]);
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
                    age: self.age[k],
                    erodibility: self.erodibility[k],
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
        let (q, r) = node_tile(key);
        let (wx, wy) = hex_to_world(q, r);
        let g = ground_at(wx, wy, seed, coasts, outlines);
        self.nodes.insert(key, g);
        g
    }

    /// Route the window of `cell`: every node in the cell and its ring, on
    /// the surface the fronts in reach complete.
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
        let mut routing = Self::route_over(keys, owned, index, &nbrs, elevation);
        routing.age = grounds.iter().map(|g| g.age).collect();
        routing.erodibility = grounds.iter().map(|g| g.erodibility).collect();
        routing.floor = Self::floors(&routing);
        routing
    }

    /// The floor at every node: its own by [`floor_at`], and never below
    /// the floor of the land node its water goes to next, so a river above
    /// a harder lip is held up to the lip and its floor never rises along
    /// a reach. Walked from each node to the sea once, memoised.
    fn floors(routing: &Routing) -> Vec<f64> {
        let n = routing.keys.len();
        let own: Vec<f64> = (0..n)
            .map(|k| {
                floor_at(
                    routing.elevation[k],
                    routing.base[k],
                    routing.carried[k],
                    routing.age[k],
                    routing.erodibility[k],
                    routing.kind[k] == Kind::Basin,
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
                    Some(d) if routing.kind[d] == Kind::Land && routing.kind[cur] != Kind::Basin => path.push(d),
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

    /// Base level at each node: what a river there cuts toward and never
    /// below. Down each node's larger share, the first fixed level: the
    /// fill of closed ground, since a river reaching it has nothing to cut
    /// toward below the rim; or the sea; water leaving the window is read
    /// as reaching the sea.
    fn base_levels(down: &[Option<usize>], kind: &[Kind], surface: &[f64]) -> Vec<f64> {
        let n = down.len();
        let mut base: Vec<Option<f64>> = vec![None; n];
        for k in 0..n {
            if base[k].is_some() {
                continue;
            }
            let mut path = vec![k];
            let level = loop {
                let cur = *path.last().unwrap();
                if let Some(b) = base[cur] {
                    break b;
                }
                if kind[cur] == Kind::Basin {
                    break surface[cur];
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
    /// outflow, catchment, base level and reaches. Publishes no age and no
    /// floor; `route` sets both.
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
                Kind::Basin => {
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

        let base = Self::base_levels(&down, &kind, &surface);

        // ── Reaches: a head is a source, a confluence, or a basin's rim ──
        let mut land_in = vec![0u32; n];
        let mut basin_in = vec![false; n];
        for k in 0..n {
            if let Some(d) = down[k] {
                match kind[k] {
                    Kind::Land => land_in[d] += 1,
                    Kind::Basin => basin_in[d] = true,
                    _ => {}
                }
            }
        }
        let head = |k: usize| kind[k] == Kind::Land && (land_in[k] != 1 || basin_in[k]);
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
                    Kind::Basin => break Terminus::Basin,
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
            parent,
            index,
            reaches,
            age: vec![0.0; n],
            erodibility: vec![1.0; n],
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
    /// has cut a third of shale's share.
    #[test]
    fn hard_rock_channels_later_and_cuts_less() {
        assert!(head_on(0.3) > head_on(1.0));
        assert!(growth(CHANNEL_HEAD + 1.0, 1.0).is_some() && growth(CHANNEL_HEAD + 1.0, 0.3).is_none());
        let (soft, hard) = (relief_share(10.0 * CATCHMENT_FULL, 1.0, 1.0), relief_share(10.0 * CATCHMENT_FULL, 1.0, 0.3));
        assert!((hard - 0.3 * soft).abs() < 1e-12, "hard {hard} for soft {soft}");
    }

    /// A node's floor is the ground on closed ground, and the envelope less
    /// its share of the height above base elsewhere, never below base.
    #[test]
    fn the_floor_is_the_ground_or_the_share() {
        assert_eq!(floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, true), 20.0);
        let floor = floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, false);
        assert!((floor - (20.0 - 15.0 * RELIEF_SHARE_MAX)).abs() < 1e-12);
        assert_eq!(floor_at(20.0, 5.0, CHANNEL_HEAD, 1.0, 1.0, false), 20.0);
        assert_eq!(floor_at(3.0, 5.0, CATCHMENT_FULL, 1.0, 1.0, false), 3.0);
        let hard = floor_at(20.0, 5.0, CATCHMENT_FULL, 1.0, 0.3, false);
        assert!(hard > floor, "hard rock cut as deep as shale");
    }
}
