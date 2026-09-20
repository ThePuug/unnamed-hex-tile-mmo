//! PlateEvent — the tectonic plates: their edges on the lattice, and the
//! crustal substrate every layer above stands on.
//!
//! # Claims
//!
//! The plate is the primary object. A plate is a Voronoi cell around a seed a
//! continent-width apart, continental or oceanic, and a continent is a
//! connected group of continental plates. Every edge between two plates is
//! drawn on the lattice and published: which two plates, which sides are
//! continental, and the chain. A coast is an edge with one continental side,
//! and there is no coast inside a plate. The substrate is a function of
//! distance to the nearest coast: the sea floor falls to the abyss on one
//! side and the land rises to its freeboard on the other, each over half a
//! plate, and holds past that. Interior relief is a noise height scaled by
//! the rise, so it varies the interior and never moves the shoreline.
//!
//! **Every edge is read through a warp.** A tile measures its distance to a
//! chain not from where it stands but from where a smooth displacement
//! field carries it, so every outline a reader measures, the coasts, the
//! fronts, the plateau's edges, bends the same way at once and never at a
//! corner: the bays and headlands are in the field, and the chains stay
//! the straight-run edges the graph draws. Every reader of a chain reads
//! through the warp and nothing else does: a field has its own
//! coordinates and a node its own position. The warp stretches a distance
//! by its gradient, and a range's slope is a distance's slope, so a
//! range reads steeper where the field squeezes and gentler where it
//! pulls; the amplitudes are the dial. It never folds: a fold would draw
//! a coast twice, and a test holds the field short of one.
//!
//! # The window
//!
//! `deform` publishes the edges whose midpoints lie in the cell: every plate
//! whose ground can reach the cell is gathered, its polygon read for its
//! edges, and each edge kept once by its pair. `prepare` gathers the coasts
//! of the cell and its ring into buckets. A query reads the nearest coast
//! within the substrate's reach, and with none in reach the plate the tile
//! lies in says whether the flat ground is land or abyss. The cell scale
//! derives from how far an edge's chain can lie from the edge's midpoint,
//! since a cell has to hold the whole chain of any edge it or its ring owns.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;

use crate::chains::{join_at_nodes, Segment, SegmentGrid};
use crate::lattice::{node_world, NODE_SPACING};
use crate::noise::simplex_2d;
use crate::tectonic::{edges_of, plate_at, plates_near, Edge, PlateId, PLATE_SPACING};
use crate::{hex_to_world, world_to_hex, CONTINENT_MAX_RISE, CONTINENT_RISE_EXPONENT, SEA_MAX_DEPTH, SHELF_EXPONENT};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::thrusting::OUTLINE_REACH;
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};

/// How far from a coast the substrate reaches its full height or depth:
/// half a plate, so a plate's interior is flat and its margin is the ramp.
pub const COAST_REACH: f64 = 0.5 * PLATE_SPACING;

/// The farthest any node of an edge's chain lies from the edge's midpoint,
/// in world units.
///
/// EMPIRICAL, measured by `edge_reach_is_bounded` over thousands of edges:
/// half the longest edge plus the widest the lattice path swings from the
/// straight line, with margin.
pub const EDGE_REACH: f64 = 10_000.0;

/// The farthest an edge's influence reaches from its midpoint: its chain,
/// the substrate's ramp from the coast the chain draws, and the warp a
/// tile reads it through.
pub const EDGE_INFLUENCE: f64 = EDGE_REACH + COAST_REACH + WARP_SWING;

/// Cell scale of every layer that publishes or reads the plate graph: one
/// ring holds every edge whose chain or shore reaches a tile of the cell, and
/// the whole outline of any plate a tile of the cell stands in, which the
/// ranges and the plateau read.
pub const GRAPH_CELL_SCALE: u32 = {
    let reach = if OUTLINE_REACH > EDGE_INFLUENCE { OUTLINE_REACH } else { EDGE_INFLUENCE };
    (reach / RING_CLEARANCE) as u32 + 1
};

/// Bucket of the coast grid: two node spacings, so a chain segment spans a
/// bucket or two and a search from the shore stops within a ring.
const COAST_BUCKET: f64 = 2.0 * NODE_SPACING as f64;

// ── The warp ────────────────────────────────────────────────────────────────

/// The warp's octaves, each a wavelength and an amplitude in world units:
/// a gulf's and a bay's. The amplitude over the wavelength is what the
/// warp stretches a distance by.
pub const WARP_OCTAVES: [(f64, f64); 2] = [(8_000.0, 700.0), (2_500.0, 180.0)];

/// The farthest the warp carries a position: the amplitudes summed. Every
/// reach that folds a chain grows by it.
pub const WARP_SWING: f64 = WARP_OCTAVES[0].1 + WARP_OCTAVES[1].1;

const WARP_SEED_X: u64 = 0x7761_7270_0000_0001;
const WARP_SEED_Y: u64 = 0x7761_7270_0000_0002;

/// The position a tile at `(wx, wy)` reads its edges at: the tile carried
/// by the warp.
pub fn warp(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    for (k, (wavelength, amplitude)) in WARP_OCTAVES.iter().enumerate() {
        let k = k as u64;
        dx += amplitude * simplex_2d(wx / wavelength, wy / wavelength, seed ^ WARP_SEED_X ^ k);
        dy += amplitude * simplex_2d(wx / wavelength, wy / wavelength, seed ^ WARP_SEED_Y ^ k);
    }
    (wx + dx, wy + dy)
}

/// The warp's Jacobian at a position, by finite differences:
/// `[[dpx/dx, dpx/dy], [dpy/dx, dpy/dy]]`.
fn jacobian(wx: f64, wy: f64, seed: u64) -> [[f64; 2]; 2] {
    let h = 0.5;
    let (ex, ey) = warp(wx + h, wy, seed);
    let (wx0, wy0) = warp(wx - h, wy, seed);
    let (nx, ny) = warp(wx, wy + h, seed);
    let (sx, sy) = warp(wx, wy - h, seed);
    [[(ex - wx0) / (2.0 * h), (nx - sx) / (2.0 * h)], [(ey - wy0) / (2.0 * h), (ny - sy) / (2.0 * h)]]
}

/// The most and the least the warp stretches a distance at a position, as
/// factors: the singular values of its Jacobian. A range there reads its
/// slope multiplied by something between them.
pub fn stretch_at(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let [[a, b], [c, d]] = jacobian(wx, wy, seed);
    let s1 = (a * a + b * b + c * c + d * d) / 2.0;
    let s2 = ((a * a + b * b - c * c - d * d).powi(2) / 4.0 + (a * c + b * d).powi(2)).sqrt();
    ((s1 + s2).sqrt(), (s1 - s2).max(0.0).sqrt())
}

/// The position the warp carries to `(wx, wy)`, to within a hair: where a
/// tile stands to read a chain point there, which is where a view draws
/// it. Newton's walk on the warp, which the no-fold bound keeps
/// invertible.
pub fn unwarp(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let (mut x, mut y) = (wx, wy);
    for _ in 0..8 {
        let (px, py) = warp(x, y, seed);
        let (rx, ry) = (px - wx, py - wy);
        if rx.hypot(ry) < 1e-6 { break }
        let [[a, b], [c, d]] = jacobian(x, y, seed);
        let det = a * d - b * c;
        if det.abs() < 1e-9 { break }
        x -= (d * rx - b * ry) / det;
        y -= (a * ry - c * rx) / det;
    }
    (x, y)
}

/// Interior relief as a share of the continental rise: enough to vary the
/// interior, never enough to reach the datum.
const RELIEF_SHARE: f64 = 0.3;

/// Wavelength of the interior relief's longer octave, in world units; the
/// shorter is a third of it.
const RELIEF_WAVELENGTH: f64 = 4_000.0;

const RELIEF_SEED: u64 = 0x5265_6C69_6566_5F5F; // "Relief__"


// ── The index ───────────────────────────────────────────────────────────────

/// The edges of the plate graph each cell owns: those whose midpoint lies in
/// it.
#[derive(Default)]
pub struct PlateEdgeIndex {
    pub cells: HashMap<CellId, Vec<Edge>>,
}

impl PlateEdgeIndex {
    pub fn edges_in(&self, cell_ids: &[CellId]) -> Vec<Edge> {
        cell_ids
            .iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter().cloned())
            .collect()
    }
}

impl CellIndex for PlateEdgeIndex {
    type Cell = Vec<Edge>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl EventIndex for PlateEdgeIndex {
    fn source_scale(&self) -> u32 { GRAPH_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        self.edges_in(cell_ids)
            .iter()
            .map(|e| { let (x, y) = e.mid(); world_to_hex(x, y) })
            .collect()
    }

    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

// ── Coasts ──────────────────────────────────────────────────────────────────

/// The coasts a set of tiles can see: every coast chain's segments, facing
/// the land, bucketed for a nearest search, read through the warp.
pub struct Coasts {
    grid: SegmentGrid,
    seed: u64,
}

impl Coasts {
    /// The coasts among a set of edges.
    pub fn new(edges: &[Edge], seed: u64) -> Self {
        let mut segments = Vec::new();
        let mut nodes = Vec::new();
        for e in edges.iter().filter(|e| e.is_coast()) {
            let land = if e.a.continental { &e.a } else { &e.b };
            // One side per chain, read off the straight edge, so every step
            // of the chain faces the land the edge does.
            let left = Segment::is_left((e.x0, e.y0), (e.x1, e.y1), (land.wx, land.wy));
            for w in e.chain.windows(2) {
                segments.push(Segment::along(node_world(w[0]), node_world(w[1]), left));
                nodes.push((w[0], w[1]));
            }
        }
        join_at_nodes(&mut segments, &nodes);
        Self { grid: SegmentGrid::new(segments, COAST_BUCKET), seed }
    }

    /// The coasts within reach of every position in a square box: what a
    /// view or a probe builds once, since the event reads them from the index.
    pub fn in_box(cx: f64, cy: f64, half: f64, seed: u64) -> Self {
        let radius = half * std::f64::consts::SQRT_2 + COAST_REACH + WARP_SWING;
        let mut seen: HashSet<(PlateId, PlateId)> = HashSet::new();
        let mut edges = Vec::new();
        for p in plates_near(cx, cy, radius, seed) {
            for e in edges_of(p.id, seed) {
                if seen.insert(e.ids()) {
                    edges.push(e);
                }
            }
        }
        Self::new(&edges, seed)
    }

    /// How far up the substrate's ramp a position stands, as a share of
    /// [`COAST_REACH`], and whether on the land side: the nearest coast in
    /// reach of the position the warp carries it to, and past the reach of
    /// every coast the plate it lies in decides which flat it is on.
    pub fn shore(&self, x: f64, y: f64) -> (f64, bool) {
        let (x, y) = warp(x, y, self.seed);
        match self.grid.nearest(x, y, COAST_REACH) {
            Some(n) => ((n.distance / COAST_REACH).clamp(0.0, 1.0), n.side > 0.0),
            None => (1.0, plate_at(x, y, self.seed).continental),
        }
    }

    pub fn segments(&self) -> &[Segment] {
        self.grid.segments()
    }
}

// ── The substrate ───────────────────────────────────────────────────────────

/// Interior relief at a position, in [−1, 1]: two octaves.
fn interior_relief(wx: f64, wy: f64, seed: u64) -> f64 {
    let long = simplex_2d(wx / RELIEF_WAVELENGTH, wy / RELIEF_WAVELENGTH, seed ^ RELIEF_SEED);
    let short = simplex_2d(3.0 * wx / RELIEF_WAVELENGTH, 3.0 * wy / RELIEF_WAVELENGTH, seed ^ RELIEF_SEED ^ 1);
    (0.7 * long + 0.3 * short).clamp(-1.0, 1.0)
}

/// The substrate at a position given the coasts in reach, in z-levels.
///
/// On the land side the ground rises from the shoreline to the continental
/// freeboard over [`COAST_REACH`] and holds, carrying the interior relief
/// scaled by that rise; on the sea side it falls to the abyssal depth over
/// the same reach. The shelf exponent holds the near-shore floor shallow and
/// the land branch takes its reciprocal, so neither side flattens at the
/// datum.
pub fn substrate_on(wx: f64, wy: f64, coasts: &Coasts, seed: u64) -> f64 {
    let (frac, land) = coasts.shore(wx, wy);
    if land {
        let rise = CONTINENT_MAX_RISE * frac.powf(CONTINENT_RISE_EXPONENT);
        rise * (1.0 + RELIEF_SHARE * interior_relief(wx, wy, seed))
    } else {
        -SEA_MAX_DEPTH * frac.powf(SHELF_EXPONENT)
    }
}

/// The substrate at a position, building the coasts around it first.
/// Measurement only: a probe or a test asking about one position. Every
/// reader with many positions builds [`Coasts`] once.
pub fn substrate_elevation_at(wx: f64, wy: f64, seed: u64) -> f64 {
    substrate_on(wx, wy, &Coasts::in_box(wx, wy, 0.0, seed), seed)
}

// ── The event ───────────────────────────────────────────────────────────────

/// The plate layer, with a memo of every plate's edges it has computed: a
/// plate's edges are a pure function of its id and the seed, and each plate
/// is asked for by every cell its edges can reach, a few dozen times.
pub struct PlateEvent {
    edges: DashMap<PlateId, Arc<Vec<Edge>>>,
}

impl PlateEvent {
    pub fn new() -> Self { PlateEvent { edges: DashMap::new() } }

    fn edges_of_plate(&self, id: PlateId, seed: u64) -> Arc<Vec<Edge>> {
        self.edges.entry(id).or_insert_with(|| Arc::new(edges_of(id, seed))).clone()
    }

    /// The edges a cell owns: those of every plate in reach whose midpoint
    /// lies in the cell, each once, in pair order.
    pub fn edges_of_cell(&self, lattice: &common::HexLattice, cell: CellId, seed: u64) -> Vec<Edge> {
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let mut seen: HashSet<(PlateId, PlateId)> = HashSet::new();
        let mut edges: Vec<Edge> = Vec::new();
        for p in plates_near(cx, cy, lattice.radius as f64, seed) {
            for e in self.edges_of_plate(p.id, seed).iter() {
                let (mx, my) = e.mid();
                let (q, r) = world_to_hex(mx, my);
                if lattice.cell_id(q, r) != cell || !seen.insert(e.ids()) { continue }
                edges.push(e.clone());
            }
        }
        edges.sort_by_key(|e| e.ids());
        edges
    }
}

impl Default for PlateEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for PlateEvent {
    fn name(&self) -> &str { "plates" }
    fn scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// An edge reaches as far as its chain does from its midpoint, and the
    /// shore's ramp beyond that.
    fn max_influence(&self) -> u32 { EDGE_INFLUENCE as u32 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<PlateEdgeIndex>();
    }

    fn deform(&self, scope: &CellScope) {
        let edges = self.edges_of_cell(scope.lattice(), scope.cell(), scope.seed());
        scope.publish::<PlateEdgeIndex>(edges);
    }

    /// Every coast a tile in this cell can see: the cell's and its ring's.
    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        let cells = scope.lattice().cells_within_distance(scope.cell(), 1);
        let edges = scope
            .read::<PlateEdgeIndex>()
            .map(|idx| idx.edges_in(&cells))
            .unwrap_or_default();
        Box::new(Coasts::new(&edges, scope.seed()))
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        cell: &(dyn Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        let coasts = cell.downcast_ref::<Coasts>()?;
        let (wx, wy) = hex_to_world(q, r);
        Some(TileOutput { elevation_delta: substrate_on(wx, wy, coasts, seed), ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tectonic::PLATE_REACH;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// Every node of every edge's chain lies within the stated reach of the
    /// edge's midpoint, and the reach is not slack.
    #[test]
    fn edge_reach_is_bounded() {
        let mut worst: f64 = 0.0;
        for cq in -12..=12 {
            for cr in -12..=12 {
                for e in edges_of((cq, cr), S) {
                    let (mx, my) = e.mid();
                    for n in &e.chain {
                        let (x, y) = node_world(*n);
                        worst = worst.max((x - mx).hypot(y - my));
                    }
                }
            }
        }
        assert!(worst <= EDGE_REACH, "a chain node {worst:.0} from its edge's midpoint, past EDGE_REACH");
        assert!(worst > 0.5 * EDGE_REACH, "EDGE_REACH is slack: the farthest node is {worst:.0}");
        assert!(EDGE_REACH < 2.0 * PLATE_REACH);
    }

    /// The substrate is continuous through the shoreline, sea on the sea
    /// side, land on the land side, and holds its extremes past the reach.
    #[test]
    fn substrate_crosses_the_datum_at_the_coast() {
        let (cx, cy) = (0.0, 0.0);
        let coasts = Coasts::in_box(cx, cy, 30_000.0, S);
        assert!(!coasts.segments().is_empty(), "no coast within 30,000 of the origin");
        let mut land = 0;
        let mut sea = 0;
        for i in 0..120 {
            for j in 0..120 {
                let (x, y) = (cx - 30_000.0 + i as f64 * 500.0, cy - 30_000.0 + j as f64 * 500.0);
                let z = substrate_on(x, y, &coasts, S);
                match coasts.shore(x, y) {
                    (d, true) if d < 1.0 => { assert!(z >= 0.0, "land below the datum {z} at {d:.2} of the reach"); land += 1 }
                    (d, false) if d < 1.0 => { assert!(z <= 0.0, "sea above the datum {z} at {d:.2} of the reach"); sea += 1 }
                    _ => assert!(z >= CONTINENT_MAX_RISE * (1.0 - RELIEF_SHARE) - 1e-9 || (z + SEA_MAX_DEPTH).abs() < 1e-9),
                }
                assert!(z >= -SEA_MAX_DEPTH - 1e-9 && z <= CONTINENT_MAX_RISE * (1.0 + RELIEF_SHARE) + 1e-9, "substrate {z} out of range");
            }
        }
        assert!(land > 100 && sea > 100, "{land} land, {sea} sea samples near a coast");
    }

    /// The warp carries no position past its swing, never folds, and its
    /// inverse lands within a hair.
    #[test]
    fn the_warp_is_bounded_and_invertible() {
        let (mut farthest, mut stretch, mut shrink) = (0.0f64, 0.0f64, f64::MAX);
        for i in 0..200 {
            for j in 0..200 {
                let (x, y) = (i as f64 * 137.0 - 13_000.0, j as f64 * 113.0 - 11_000.0);
                let (px, py) = warp(x, y, S);
                farthest = farthest.max((px - x).hypot(py - y));
                let (most, least) = stretch_at(x, y, S);
                stretch = stretch.max(most);
                shrink = shrink.min(least);
                let (ux, uy) = unwarp(px, py, S);
                assert!((ux - x).hypot(uy - y) < 0.01, "unwarp misses by {} at ({x}, {y})", (ux - x).hypot(uy - y));
            }
        }
        assert!(farthest <= WARP_SWING, "the warp carried a position {farthest:.0}, past WARP_SWING");
        assert!(farthest > 0.5 * WARP_SWING, "WARP_SWING is slack: the farthest carry is {farthest:.0}");
        assert!(shrink > 0.15, "the warp folds: a distance shrinks to {shrink:.3} of itself");
        assert!(stretch < 3.0, "the warp stretches a distance {stretch:.2} times");
    }

    /// The edges a cell owns are exactly those whose midpoints lie in it,
    /// and two cells never own the same edge.
    #[test]
    fn cells_own_edges_by_midpoint() {
        let lattice = common::HexLattice::new(GRAPH_CELL_SCALE);
        let home = lattice.cell_id(0, 0);
        let mut owned: HashSet<(PlateId, PlateId)> = HashSet::new();
        let event = PlateEvent::new();
        for cell in lattice.cells_within_distance(home, 1) {
            for e in event.edges_of_cell(&lattice, cell, S) {
                let (mx, my) = e.mid();
                let (q, r) = world_to_hex(mx, my);
                assert_eq!(lattice.cell_id(q, r), cell);
                assert!(owned.insert(e.ids()), "edge {:?} owned twice", e.ids());
            }
        }
        assert!(owned.len() > 10, "{} edges over seven cells", owned.len());
    }
}
