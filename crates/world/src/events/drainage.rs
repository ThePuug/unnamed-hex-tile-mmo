//! DrainageEvent — where water runs on the tectonic surface.
//!
//! Routes water over the composed surface on a coarse world-wide lattice of
//! nodes, and publishes the channel network of every landmass as reaches with
//! catchment, and the lakes that closed ground fills. It moves no ground and
//! its query returns nothing: dissection cuts along what this publishes.
//!
//! # The surface is a function
//!
//! Everything beneath this layer is a field, so the surface beneath it is one
//! function of position and seed — [`surface_at`] — evaluated at each node
//! without materialising a tile. That is what lets routing, which needs the
//! ground over a whole basin, happen in `deform`. It binds the stack beneath:
//! a layer added under drainage stays a field, or states its elevation as a
//! function and is added to [`surface_at`], or drainage routes over ground
//! that is not there. `drainage_probe` checks the sum against the composite.
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
//! The deform reads no index, and says so: the cascade stops here instead of
//! deforming four layers of cells under a continent-sized window for nothing.
//! Node elevations are memoised across windows, so each node is evaluated
//! once however many windows contain it.

use std::any::Any;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use common::HexLattice;
use dashmap::DashMap;

use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::tilt::tilt_at;
use super::{CellScope, TileOutput, TileView, WorldEvent};
use crate::orogen_field::{relief_on, BELT_HALF_WIDTH};
use crate::{
    hex_to_world, substrate_elevation_at, world_to_hex, CONTINENT_CELL_SIZE, CONTINENT_JITTER,
    CONTINENT_WARP_AMPLITUDE,
};

// ── Constants ───────────────────────────────────────────────────────────────

/// Nodes across one belt flank. A range's crest has to read as a divide
/// rather than as one line of nodes, and one node spacing is the narrowest
/// valley the layer means to read as a valley.
const NODES_PER_FLANK: f64 = 5.0;

/// Node spacing in tiles, derived from the belt half-width.
pub const NODE_SPACING: i32 = (BELT_HALF_WIDTH / NODES_PER_FLANK) as i32;

/// The furthest land of a continent cell lies from its seed point: the bound
/// the plate layer guarantees.
const CONTINENT_BOUND: f64 =
    CONTINENT_CELL_SIZE * (1.0 + CONTINENT_JITTER) + CONTINENT_WARP_AMPLITUDE;

/// What one ring of cells clears, as a multiple of the cell radius. The
/// framework measures it at `add_event`; it is restated here because the
/// scale below is derived from it and a const cannot call the measurement.
const RING_CLEARANCE: f64 = 1.268;

/// Cell scale, derived: one ring covers the bound a continent cell guarantees
/// for its land, so a basin that fits one continent cell is counted whole.
pub const DRAINAGE_CELL_SCALE: u32 = (CONTINENT_BOUND / RING_CLEARANCE) as u32 + 1;

/// Two water levels closer than this are one flat.
const FLAT: f64 = 1e-9;

/// The six neighbours in angular order, 60° apart from `+x` round to `300°`,
/// so consecutive entries bound one facet.
const NEIGHBOURS: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

const SIXTY: f64 = std::f64::consts::PI / 3.0;

// ── The surface ─────────────────────────────────────────────────────────────

/// The surface beneath drainage at a position, in z-levels: every layer below
/// it, summed as the functions they are. Equal to the composed tile's
/// elevation at a tile centre.
pub fn surface_at(wx: f64, wy: f64, seed: u64) -> f64 {
    let substrate = substrate_elevation_at(wx, wy, seed);
    let base = substrate + tilt_at(wx, wy, substrate, seed);
    base + relief_on(wx, wy, base, seed).max(0.0)
}

// ── Nodes ───────────────────────────────────────────────────────────────────

/// A node's lattice coordinates. Its tile is `(i × NODE_SPACING, j × NODE_SPACING)`.
pub type NodeKey = (i32, i32);

pub fn node_tile(key: NodeKey) -> (i32, i32) {
    (key.0 * NODE_SPACING, key.1 * NODE_SPACING)
}

/// The node nearest a position.
pub fn nearest_node(wx: f64, wy: f64) -> NodeKey {
    let (q, r) = world_to_hex(wx, wy);
    let s = NODE_SPACING as f64;
    hex_round(q as f64 / s, r as f64 / s)
}

fn hex_round(fq: f64, fr: f64) -> (i32, i32) {
    let fs = -fq - fr;
    let (mut q, mut r, s) = (fq.round(), fr.round(), fs.round());
    let (dq, dr, ds) = ((q - fq).abs(), (r - fr).abs(), (s - fs).abs());
    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    (q as i32, r as i32)
}

fn hex_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dq, dr) = (a.0 - b.0, a.1 - b.1);
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

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
    /// Ground, in z-levels.
    pub elevation: f64,
    /// Water: the ground, or the lake surface where the ground is flooded.
    pub surface: f64,
    /// The true downslope at this node, a unit vector in world space. Across a
    /// lake it points along the flood's path to the outlet.
    pub direction: (f64, f64),
    /// Water draining through this one, itself included, in nodes. Fractional
    /// because a node's water splits between two neighbours.
    pub catchment: f64,
    /// The node taking the larger share of this one's water. None at a sink.
    pub down: Option<NodeKey>,
    /// Index into the cell's lakes when this node is flooded.
    pub lake: Option<usize>,
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

struct RoutedLake {
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
    pub lake_of: Vec<Option<usize>>,
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
            });
        }

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
                    direction: self.direction[k],
                    catchment: self.catchment[k],
                    down: self.down[k].map(|d| self.keys[d]),
                    lake: self.lake_of[k].and_then(|id| lake_local.get(&id).copied()),
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
    /// Node elevations, shared by every window that contains a node.
    elevations: DashMap<NodeKey, f64>,
}

impl DrainageEvent {
    pub fn new() -> Self {
        Self { elevations: DashMap::new() }
    }

    fn elevation(&self, key: NodeKey, seed: u64) -> f64 {
        if let Some(e) = self.elevations.get(&key) {
            return *e;
        }
        let (q, r) = node_tile(key);
        let (wx, wy) = hex_to_world(q, r);
        let e = surface_at(wx, wy, seed);
        self.elevations.insert(key, e);
        e
    }

    /// Route the window of `cell`: every node in the cell and its ring.
    pub fn route(&self, lattice: &HexLattice, cell: CellId, seed: u64) -> Routing {
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
        let n = keys.len();
        let index: HashMap<NodeKey, usize> =
            keys.iter().enumerate().map(|(k, &key)| (key, k)).collect();
        let nbrs: Vec<[Option<usize>; 6]> = keys
            .iter()
            .map(|&(i, j)| NEIGHBOURS.map(|(di, dj)| index.get(&(i + di, j + dj)).copied()))
            .collect();
        let elevation: Vec<f64> = keys.iter().map(|&k| self.elevation(k, seed)).collect();

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
            lakes.push(RoutedLake { members, surface: surface[k], outlet });
        }

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
            lake_of,
            index,
            lakes,
            reaches,
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

    /// The surface is read as a function; no index beneath is touched.
    fn reads_below(&self) -> bool { false }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<DrainageIndex>();
    }

    fn deform(&self, scope: &CellScope) {
        let routing = self.route(scope.lattice(), scope.cell(), scope.seed());
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
