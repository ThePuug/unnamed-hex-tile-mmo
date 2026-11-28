//! PlateEvent — the tectonic plates: their edges on the lattice, and the
//! crustal substrate every layer above stands on.
//!
//! # Claims
//!
//! The plate is the primary object. A plate is a Voronoi cell around a seed,
//! continental where a continent site claims it or oceanic, and a continent
//! is one site's claim: a compact cluster. Every edge between two plates is
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
use crate::lattice::{node_world, CHAIN_SWING, NODE_SPACING};
use crate::noise::simplex_2d;
use crate::tectonic::{edges_of, plate_at, plates_near, Edge, PlateId, PLATE_REACH, PLATE_SPACING};
use crate::{hex_to_world, sea_floor, smoothstep, world_to_hex, Shelf, CONTINENT_MAX_RISE, CONTINENT_RISE_EXPONENT, SLOPE_WIDTH};
use super::index::{CellId, CellIndex, IndexRegistry, Located};
use super::thrusting::OUTLINE_REACH;
use super::{CellScope, TileOutput, TileView, WorldEvent, RING_CLEARANCE};

/// How far from a coast the substrate reaches its full height or depth:
/// half a plate, so a plate's interior is flat and its margin is the ramp.
pub const COAST_REACH: f64 = 0.5 * PLATE_SPACING;

/// The farthest any node of an edge's chain lies from the edge's midpoint,
/// in world units: half the edge, which the plate reach bounds since both
/// its corners lie within it of either plate's seed, and the chain's swing
/// off the edge.
pub const EDGE_REACH: f64 = PLATE_REACH + CHAIN_SWING;

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

/// The farthest the warp can carry a position along either axis, by the
/// noise's bound rather than its usual range: what a reach that must hold
/// without exception allows for.
pub const WARP_REACH: f64 = crate::noise::SIMPLEX_BOUND * WARP_SWING;

/// The farthest the warp carries two adjacent tiles apart, one world unit
/// from each other: the step, and √2 times the most each axis's
/// displacement changes over it, the octaves' amplitudes over their
/// wavelengths times 16.6, which bounds `simplex_2d`'s gradient (three
/// corners, each at most 70 · max(t⁴ + 8t³r²) at t = ½ − r²).
pub const WARP_STEP: f64 =
    1.0 + std::f64::consts::SQRT_2 * 16.6 * (WARP_OCTAVES[0].1 / WARP_OCTAVES[0].0 + WARP_OCTAVES[1].1 / WARP_OCTAVES[1].0);

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

impl CellIndex for PlateEdgeIndex {
    type Cell = Vec<Edge>;
    type Feature = Edge;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }

    fn features(entry: &Self::Cell) -> impl Iterator<Item = &Edge> {
        entry.iter()
    }
}

/// An edge lies where its chain runs, which the straight edge it is drawn
/// on lies within.
impl Located for Edge {
    fn bounds(&self) -> [f64; 4] {
        self.chain.iter().map(|&n| node_world(n)).chain([(self.x0, self.y0), (self.x1, self.y1)]).fold(
            [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
            |b, (x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
        )
    }
}

// ── Coasts ──────────────────────────────────────────────────────────────────

/// The coasts a set of tiles can see: every coast chain's segments, facing
/// the land, bucketed for a nearest search, read through the warp; and
/// each coast's own shelf, by its margin and its continent's plate.
pub struct Coasts {
    grid: SegmentGrid,
    /// The coast each segment is a piece of, and each coast's shelf.
    coast_of: Vec<u32>,
    shelves: Vec<Shelf>,
    seed: u64,
}

/// How much further from a position than its nearest coast another coast
/// may lie and still shape the sea floor there, in world units. A choice
/// between two coasts would step the floor along the line between them, so
/// a floor is every coast's this near, each by how near.
const SHELF_BLEND: f64 = 300.0;

impl Coasts {
    /// The coasts among a set of edges.
    pub fn new<'a>(edges: impl IntoIterator<Item = &'a Edge>, seed: u64) -> Self {
        let mut segments = Vec::new();
        let mut nodes = Vec::new();
        let (mut coast_of, mut shelves) = (Vec::new(), Vec::new());
        for e in edges.into_iter().filter(|e| e.is_coast()) {
            let land = if e.a.continental { &e.a } else { &e.b };
            // One side per chain, read off the straight edge, so every step
            // of the chain faces the land the edge does.
            let left = Segment::is_left((e.x0, e.y0), (e.x1, e.y1), (land.wx, land.wy));
            for w in e.chain.windows(2) {
                segments.push(Segment::along(node_world(w[0]), node_world(w[1]), left));
                nodes.push((w[0], w[1]));
                coast_of.push(shelves.len() as u32);
            }
            // The margin is the edge's own motion, as the belts read it.
            let closing = super::motion::resolve(e, seed).convergence / super::thrusting::CONVERGENCE_FULL;
            shelves.push(Shelf::of(closing, land.age));
        }
        join_at_nodes(&mut segments, &nodes);
        Self { grid: SegmentGrid::new(segments, COAST_BUCKET), coast_of, shelves, seed }
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
    /// reach of the position the warp carries it to. The coast's side
    /// decides within [`CHAIN_SWING`] of it; past that the plate the
    /// position lies in decides, which agrees with every sound coast and
    /// keeps land within two swings of a continental plate where chains
    /// overlap and a coast's side reads wrong.
    pub fn shore(&self, x: f64, y: f64) -> (f64, bool) {
        let (x, y) = warp(x, y, self.seed);
        self.shore_warped(x, y)
    }

    fn shore_warped(&self, x: f64, y: f64) -> (f64, bool) {
        match self.grid.nearest(x, y, COAST_REACH) {
            Some(n) => {
                let land = if n.distance <= CHAIN_SWING { n.side > 0.0 } else { plate_at(x, y, self.seed).continental };
                ((n.distance / COAST_REACH).clamp(0.0, 1.0), land)
            }
            None => (1.0, plate_at(x, y, self.seed).continental),
        }
    }

    /// How deep the sea floor lies at a position `distance` off its
    /// nearest coast, in z-levels ([`sea_floor`]): by the shelf of every
    /// coast within [`SHELF_BLEND`] of as near, each weighing the more the
    /// nearer it is to the nearest, so the floor passes from one coast's
    /// to the next's round a plate's corner and never steps. Past every
    /// shelf's foot it is the abyss whichever coast is asked.
    fn floor(&self, x: f64, y: f64, distance: f64) -> f64 {
        if distance >= Shelf::WIDEST + SLOPE_WIDTH {
            return sea_floor(distance, Shelf::of(0.0, 1.0));
        }
        // The nearest each coast comes: a handful of coasts at most.
        let mut near: Vec<(u32, f64)> = Vec::new();
        self.grid.each_near(x, y, distance + SHELF_BLEND, |segment, d| {
            let coast = self.coast_of[segment];
            match near.iter_mut().find(|(c, _)| *c == coast) {
                Some(found) => found.1 = found.1.min(d),
                None => near.push((coast, d)),
            }
        });
        let (mut shoreface, mut width, mut weight) = (0.0, 0.0, 0.0);
        for (coast, d) in near {
            let w = (1.0 - (d - distance) / SHELF_BLEND).max(0.0).powi(2);
            let shelf = self.shelves[coast as usize];
            shoreface += w * shelf.shoreface;
            width += w * shelf.width;
            weight += w;
        }
        // The nearest coast weighs whole, so the weight is never nothing.
        sea_floor(distance, Shelf { shoreface: shoreface / weight, width: width / weight })
    }

    pub fn segments(&self) -> &[Segment] {
        self.grid.segments()
    }
}

/// Every coast a cell's tiles can see, from the edges the plate layer
/// published under the cell and its ring: what a layer standing on the
/// substrate builds once in `prepare`.
pub fn coasts_of(scope: &CellScope) -> Coasts {
    let edges = scope.read::<PlateEdgeIndex>();
    Coasts::new(edges.iter().flat_map(|idx| idx.features()), scope.seed())
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
/// scaled by that rise; on the sea side it is the sea floor that far off
/// the shore, by the shelves of the coasts it lies off ([`Coasts::floor`]),
/// which reaches the abyssal depth well inside the reach. Both sides leave
/// the datum at a slope, so the shoreline is one line however coarsely the
/// ground is read.
pub fn substrate_on(wx: f64, wy: f64, coasts: &Coasts, seed: u64) -> f64 {
    let (x, y) = warp(wx, wy, coasts.seed);
    let (frac, land) = coasts.shore_warped(x, y);
    if land {
        let rise = CONTINENT_MAX_RISE * frac.powf(CONTINENT_RISE_EXPONENT);
        rise * (1.0 + RELIEF_SHARE * interior_relief(wx, wy, seed))
    } else {
        -coasts.floor(x, y, frac * COAST_REACH)
    }
}

/// The substrate at a position, building the coasts around it first.
/// Measurement only: a probe or a test asking about one position. Every
/// reader with many positions builds [`Coasts`] once.
pub fn substrate_elevation_at(wx: f64, wy: f64, seed: u64) -> f64 {
    substrate_on(wx, wy, &Coasts::in_box(wx, wy, 0.0, seed), seed)
}

/// Substrate elevation at which what stands on land stands whole, in
/// z-levels: the tilt's lean and the cuestas. The substrate's land
/// elevation p10 is 8.7 z, so saturating here gives nine tenths of all land
/// the whole and confines the taper to the beach band.
///
/// It **saturates** deliberately. A share proportional to the substrate
/// elevation would put the maximum in the interior and nothing at every
/// coast, which is a dome. Only the coastal band tapers.
pub const SHORE_FULL_ELEVATION: f64 = 9.0;

/// How much of what stands on land stands at `substrate`: nothing at and
/// below sea level, the whole from [`SHORE_FULL_ELEVATION`] up.
pub fn shore_gate(substrate: f64) -> f64 {
    smoothstep(substrate / SHORE_FULL_ELEVATION)
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
        Box::new(coasts_of(scope))
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
                    _ => assert!(z >= CONTINENT_MAX_RISE * (1.0 - RELIEF_SHARE) - 1e-9 || (z + crate::SEA_MAX_DEPTH).abs() < 1e-9),
                }
                assert!(z >= -crate::SEA_MAX_DEPTH - 1e-9 && z <= CONTINENT_MAX_RISE * (1.0 + RELIEF_SHARE) + 1e-9, "substrate {z} out of range");
            }
        }
        assert!(land > 100 && sea > 100, "{land} land, {sea} sea samples near a coast");
    }

    /// Whatever a coast's margin, its sea floor never rises further out,
    /// and a margin closing lies as deep as a quiet one at every distance
    /// or deeper: its shore is steeper and its shelf's edge nearer.
    #[test]
    fn a_sea_floor_falls_away_and_a_closing_margins_sooner() {
        for age in [0.0, 0.5, 1.0] {
            for (quieter, closing) in [(0.0, 0.5), (0.5, 1.0)] {
                let (wide, narrow) = (Shelf::of(quieter, age), Shelf::of(closing, age));
                let mut last = (0.0, 0.0);
                for step in 0..700 {
                    let d = step as f64 * 5.0;
                    let now = (sea_floor(d, wide), sea_floor(d, narrow));
                    assert!(now.0 >= last.0 - 1e-9 && now.1 >= last.1 - 1e-9, "a floor rose at {d}");
                    assert!(now.1 >= now.0 - 1e-9, "the closing margin lies shallower at {d}: {} against {}", now.1, now.0);
                    last = now;
                }
                assert!((last.0 - crate::SEA_MAX_DEPTH).abs() < 1e-6 && (last.1 - crate::SEA_MAX_DEPTH).abs() < 1e-6, "both reach the abyss");
            }
        }
    }

    /// The sea floor never steps: two positions a couple of units apart
    /// lie within what the steepest floor falls in that distance, round
    /// the corners where coasts of different margins meet as anywhere. A
    /// floor chosen from the nearer of two coasts would step by the whole
    /// difference between them along the line between.
    #[test]
    fn the_sea_floor_never_steps_between_coasts() {
        let coasts = Coasts::in_box(0.0, 0.0, 30_000.0, S);
        let mut sea = 0;
        for i in 0..150 {
            for j in 0..150 {
                let (x, y) = (-30_000.0 + i as f64 * 400.0, -30_000.0 + j as f64 * 400.0);
                let here = substrate_on(x, y, &coasts, S);
                if here >= 0.0 {
                    continue;
                }
                sea += 1;
                for (dx, dy) in [(2.0, 0.0), (0.0, 2.0)] {
                    let there = substrate_on(x + dx, y + dy, &coasts, S);
                    assert!(there >= 0.0 || (there - here).abs() < 12.0, "the floor steps {} at ({x}, {y})", there - here);
                }
            }
        }
        assert!(sea > 1_000, "{sea} sea samples");
    }

    /// The gate saturates. What stands on land scaled with the substrate's
    /// elevation would be a dome, and this is the assertion that separates
    /// the two.
    #[test]
    fn gate_saturates_above_the_beach() {
        assert_eq!(shore_gate(-10.0), 0.0);
        assert_eq!(shore_gate(0.0), 0.0);
        assert!(shore_gate(SHORE_FULL_ELEVATION * 0.5) > 0.3);
        assert_eq!(shore_gate(SHORE_FULL_ELEVATION), 1.0);
        // Saturated: the interior stands whole however high it is.
        assert_eq!(shore_gate(20.0), 1.0);
        assert_eq!(shore_gate(45.0), 1.0);
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
