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

use std::any::Any;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use common::HexLattice;
use dashmap::DashMap;

use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::plates::{Coasts, PlateEdgeIndex};
use super::thickening::thickening_on;
use super::thrusting::{outlines_of, Outlines};
use super::tilt::tilt_at;
use super::{CellScope, TileOutput, TileView, WorldEvent};
use crate::lattice::{hex_distance, DIRECTIONS as NEIGHBOURS};
pub use crate::lattice::{node_tile, NodeKey, NODE_SPACING};
use crate::tectonic::PLATE_REACH;
use crate::{hex_to_world, substrate_on};

// ── Constants ───────────────────────────────────────────────────────────────


/// What one ring of cells clears, as a multiple of the cell radius. The
/// framework measures it at `add_event`; it is restated here because the
/// scale below is derived from it and a const cannot call the measurement.
const RING_CLEARANCE: f64 = 1.268;

/// Cell scale, derived: one ring covers a plate's reach from its seed, so a
/// basin that drains a plate's interior to its coast is counted whole.
pub const DRAINAGE_CELL_SCALE: u32 = (PLATE_REACH / RING_CLEARANCE) as u32 + 1;

/// Two water levels closer than this are one flat.
const FLAT: f64 = 1e-9;

const SIXTY: f64 = std::f64::consts::PI / 3.0;

// ── The surface ─────────────────────────────────────────────────────────────

/// The surface beneath drainage at a position, in z-levels: the layers
/// summed as the functions they are, the substrate from the coasts in reach,
/// the tilt, the ranges and the plateau from the plate outlines in reach.
/// Equal to the composed tile's elevation at a tile centre.
pub fn surface_at(wx: f64, wy: f64, seed: u64, coasts: &Coasts, outlines: &Outlines) -> f64 {
    let substrate = substrate_on(wx, wy, coasts, seed);
    let base = substrate + tilt_at(wx, wy, substrate, seed);
    base + outlines.relief(wx, wy).max(0.0) + thickening_on(wx, wy, outlines).max(0.0)
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
    /// Base level, in z-levels: the surface of the first lake down the larger
    /// share's path, or sea level. What a river here cuts toward and never
    /// below.
    pub base: f64,
    /// The node taking the larger share of this one's water. None at a sink.
    pub down: Option<NodeKey>,
    /// Index into the cell's lakes when this node is flooded.
    pub lake: Option<usize>,
    /// This node is a lake's outlet: the sill whose height is the lake's
    /// surface. A valley cut here would drain the lake, so dissection cuts
    /// none.
    pub sill: bool,
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
    /// Base level at each node: the first lake down the larger share's
    /// path, or sea level.
    pub base: Vec<f64>,
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
                    base: self.base[k],
                    down: self.down[k].map(|d| self.keys[d]),
                    lake: self.lake_of[k].and_then(|id| lake_local.get(&id).copied()),
                    sill: sills.contains(&k),
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

    fn elevation(&self, key: NodeKey, seed: u64, coasts: &Coasts, outlines: &Outlines) -> f64 {
        if let Some(e) = self.elevations.get(&key) {
            return *e;
        }
        let (q, r) = node_tile(key);
        let (wx, wy) = hex_to_world(q, r);
        let e = surface_at(wx, wy, seed, coasts, outlines);
        self.elevations.insert(key, e);
        e
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
        let n = keys.len();
        let index: HashMap<NodeKey, usize> =
            keys.iter().enumerate().map(|(k, &key)| (key, k)).collect();
        let nbrs: Vec<[Option<usize>; 6]> = keys
            .iter()
            .map(|&(i, j)| NEIGHBOURS.map(|(di, dj)| index.get(&(i + di, j + dj)).copied()))
            .collect();
        let elevation: Vec<f64> = keys.iter().map(|&k| self.elevation(k, seed, coasts, outlines)).collect();

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

        // ── Base level: the first lake down each node's larger share, else
        //    the sea; water leaving the window is read as reaching the sea ──
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
        let base: Vec<f64> = base.into_iter().map(|b| b.unwrap_or(0.0)).collect();

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
            base,
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
