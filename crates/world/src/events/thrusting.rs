//! ThrustingEvent — thrust sheets stacked from a convergent plate edge: the
//! ranges.
//!
//! The shortening an edge takes up is accommodated by thrust sheets stacking
//! in sequence from the deformation front, and their structural relief is
//! the ranges. The front is a real object — the mapped line at the edge of
//! every fold-thrust belt — and here it is a convergent edge of the plate
//! graph, read from the motion layer's resolution with the edge's
//! convergence. A tile's place in the family is its distance to the front,
//! which moves one for one with the ground.
//!
//! # Claims
//!
//! Sheets stack across strike one spacing apart, the spacing set by the
//! thickness of the shortened layer and tightened by a sheet's internal
//! shortening, which accumulates with its age in the wedge. A wedge is as
//! many sheets deep as its edge has shortened: a sheet takes up about one
//! spacing of displacement before the next breaks in front of it, so the
//! count is the edge's convergence over a constant, four at full convergence
//! the way the Himalaya stack four between the foreland and Tibet, and the
//! last sheet is partial, since a sheet grows as displacement accumulates on
//! it. Ranges scale with convergence and never switch on: the weakest edge
//! carries one low frontal ridge, which is what foothills are. Each sheet is
//! an asymmetric wedge whose steep forelimb faces the front; the frontal
//! sheet's forelimb rises from the front itself and the last sheet's
//! backlimb rests on the plateau. A belt is one-sided: the wedge stands on
//! the overriding plate and verges toward the plate going under, whose side
//! carries nothing but the scree that comes to rest across the front. A
//! sheet is continuous for its length: a range stands full along its edge,
//! ends against the plate's other edges over one sheet spacing, straight
//! and on the lattice, and its height varies along strike with the ground
//! under it and the front's shape, never with a field finer than a sheet. A
//! sheet's structure stands at a thrust wedge's dips, the forelimb at 50°;
//! its surface does not, since rock steeper than debris rests at sheds, so
//! the cross-section is held under a cone at repose from every point of it:
//! the crest keeps its height, the forelimb becomes scree reaching further
//! from it and buries the trough's near side, the frontal sheet's runs out
//! past the front onto the plate going under, and the backlimb is barely
//! touched. Repose is over the one level a step climbs, so a range is
//! climbed by traverse and crossed at its passes.
//!
//! The retro-wedge at the plateau's far side is not expressed. Unbuilt.
//!
//! # What a tile reads
//!
//! **The plate it stands in, its distance to each of that plate's edges, and
//! no choice among them.** A plate's outline is its edges' chains, and a
//! tile stands in the plate whose outline encloses it. Each convergent edge
//! of that plate gives the tile a wedge by the tile's distance to that edge,
//! and the strongest stands. Distance to each edge is continuous and a choice
//! among edges is not: a tile that took the nearest edge alone would switch
//! wedges along the bisector between two edges of different convergence, a
//! cliff the height of a range. Across an edge the two plates' wedges both
//! reach nothing, so nothing steps.
//!
//! **Every front in reach, past it, whichever plate's.** Scree at rest past
//! a front stands on a plate that does not own the wedge, and around the
//! corner the front ends at it stands on a third. A tile reads the toe of
//! every wedge whose front is within the scree's reach, at its distance to
//! that front and ended against that plate's other edges as the wedge is,
//! so the plates meeting at a corner all read the same toes there. A tile
//! that read only across the edge it stands nearest would step at the
//! corner by the toe's height.
//!
//! **Spacing varies across strike only.** A spacing read from a field that
//! varies along strike moves the n-th sheet n times as far as the field's
//! gradient: the innermost sheets swing faster than the ground.
//!
//! **Height varies along strike with nothing finer than a sheet.** Scaled by
//! a strain octave, ranges broke into lenses a few thousand units long; a
//! sheet runs its whole length.
//!
//! # The window
//!
//! The layer publishes nothing: the ranges are read off the plate graph at
//! every tile. `prepare` gathers the resolved edges of the graph's cells
//! under the cell and its ring into outlines, one per plate, and a query
//! finds the plate its tile stands in and reads that outline. The cell is the
//! graph's, which is sized so one ring holds the whole outline of any plate a
//! tile in the cell stands in.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;

use crate::chains::{join_at_nodes, Segment};
use crate::lattice::{nearest_node, node_world, NodeKey, PATH_SWING};
use crate::tectonic::{edges_of, plate_cell_for, plates_near, PlateId, PLATE_REACH};
use super::plates::{warp, WARP_SWING};
use crate::{hex_to_world, RISE, SEA_MAX_DEPTH};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::motion::{resolve, BoundaryRegime, BoundarySegment, PlateBoundaryIndex};
use super::plates::GRAPH_CELL_SCALE;
use super::thickening::ESCARPMENT;
use super::{gradient_of, CellScope, TileOutput, TileView, WorldEvent};

// ── Sheet geometry ──────────────────────────────────────────────────────────

/// Continental crust thickness in z-levels, on the vertical scale that maps
/// Earth's 4 km abyssal plain to [`SEA_MAX_DEPTH`]: 35 km of crust.
const CRUST_THICKNESS: f64 = SEA_MAX_DEPTH * 35.0 / 4.0;

/// Spacing of sheets across a belt, in world units.
///
/// Thrust spacing scales with the thickness of the layer being shortened, and
/// for basement-cored ranges that layer is the whole crust. So the spacing is
/// the crust's thickness, turned into a horizontal length by the same
/// z-to-world ratio the dips use.
pub const RANGE_SPACING: f64 = CRUST_THICKNESS * RISE;

/// Half-width of a range: half a spacing. A range is one thrust sheet wide,
/// so neighbouring ranges meet at the trough between them with no floor in
/// between. The flanks are asymmetric, and the steep and graded half-widths
/// sum to the spacing.
pub const RANGE_HALF_WIDTH: f64 = RANGE_SPACING * 0.5;

/// Width of a sheet's steep flank as a share of its graded flank.
///
/// A fold-and-thrust wedge is asymmetric because it is built by thrusts that
/// all verge one way: the forelimb dips 40–60° and the backlimb 10–25°, so the
/// steep flank is roughly a third the width of the graded one at the same
/// height.
const STEEP_FLANK_SHARE: f64 = 0.35;

pub const RANGE_STEEP_HALF_WIDTH: f64 =
    2.0 * RANGE_HALF_WIDTH * STEEP_FLANK_SHARE / (1.0 + STEEP_FLANK_SHARE);
pub const RANGE_GRADED_HALF_WIDTH: f64 = 2.0 * RANGE_HALF_WIDTH / (1.0 + STEEP_FLANK_SHARE);

/// Exponent of the cross-strike amplitude taper of one sheet. A squared taper
/// puts the shoulders where a wedge's are: relief falls away slowly near the
/// crest and steeply at the toe. It has no derivation of its own.
const TAPER_EXPONENT: f64 = 2.0;

/// Dip of a range's forelimb, in degrees: the midpoint of the 40–60° a thrust
/// wedge's forelimb dips at, the same geology [`STEEP_FLANK_SHARE`] is read
/// from. The backlimb then falls out at its own 10–25°, which a test holds.
///
/// The dip is the structure's. The surface stands under repose, below; what a
/// player can climb is movement's to decide.
#[cfg(test)]
const FORELIMB_DIP_DEGREES: f64 = 50.0;

/// tan of the forelimb dip, 50°. A const cannot call `tan`; a test holds
/// the two equal.
const FORELIMB_DIP_TAN: f64 = 1.191_753_592_594_210;

/// Elevation a range reaches at full strength on continental crust: the
/// forelimb dip restated as height over the steep half-width.
///
/// **Height and half-width are coupled, and the coupling is the durable thing
/// here.** A slope is a ratio of two lengths: `dip = atan(rise × RISE /
/// half_width)`. Widening ranges raises this with them, or a range flattens
/// into a ramp.
pub const RANGE_RISE: f64 = RANGE_STEEP_HALF_WIDTH * FORELIMB_DIP_TAN / RISE;

/// The angle loose, angular rock debris comes to rest at, in degrees. A
/// range's structure stands at its dips; its surface stands no steeper than
/// this, since rock steeper sheds and the scree rests here.
#[cfg(test)]
const REPOSE_DEGREES: f64 = 34.0;

/// tan of the repose angle, 34°. A const cannot call `tan`; a test holds
/// the two equal.
const REPOSE_TAN: f64 = 0.674_508_516_842_426_5;

/// Scree's fall per world unit of run, in z-levels: a range's cross-section
/// descends from any point of it no steeper. Over the one level a step
/// climbs, so a range is climbed by traverse.
pub const REPOSE_GRADE: f64 = REPOSE_TAN / RISE;

/// How far scree from a full range's crest reaches, in world units: its
/// height over the repose grade. What a cross-section is read over to hold
/// it under repose.
const SCREE_REACH: f64 = RANGE_RISE / REPOSE_GRADE;

/// Spacing of the points a cross-section is read at to hold it under repose,
/// in world units, on a grid fixed in the across-strike coordinate so the
/// result is continuous. The structure is taken straight between points, so
/// a crest stands under its structure by a chord's error, under a level.
const REPOSE_STEP: f64 = 8.0;

/// Where a scree profile begins, in world units from the front: scree from
/// the frontal crest reaches this far out past the front.
const SCREE_START: f64 = -SCREE_REACH;

/// Total shortening a fold-thrust belt has taken up, as a fraction of its
/// original width: the Zagros 15–25%, the Appalachians near half. A sheet
/// that has been in the wedge for the belt's whole life has shortened by this
/// much, and sits that much closer to its neighbour.
const BELT_SHORTENING: f64 = 0.30;

/// How far into the wedge a sheet has been there for the belt's whole life:
/// two spacings, so the frontal sheet is fresh and the third is fully
/// shortened.
const SHORTENING_REACH: f64 = 2.0 * RANGE_SPACING;

/// Sheets a wedge stacks at full convergence. A crust-scale fold-thrust belt
/// is a handful of major sheets deep before the hinterland is plateau: the
/// Himalaya stack four between the foreland and Tibet.
pub const WEDGE_SHEETS: f64 = 4.0;

/// Convergence at which an edge stacks the full wedge, in the motion
/// field's units.
///
/// EMPIRICAL: the 90th percentile of convergence over convergent edges,
/// measured by `plate_probe`, so a tenth of convergent edges carry the full
/// four sheets and the rest fewer.
pub const CONVERGENCE_FULL: f64 = 0.43;

/// The farthest the last sheet's backlimb can reach from the front, in world
/// units: every sheet at the untightened spacing. Tightening only brings the
/// rim nearer, so this bounds the wedge and a tile beyond it stands on
/// plateau, never on a sheet.
pub const WEDGE_REACH: f64 = WEDGE_SHEETS * RANGE_SPACING;

/// How far from a plate's other edges a wedge's ranges reach full height:
/// one sheet spacing, so a range ends against the edge it is cut off by.
pub const RANGE_END: f64 = RANGE_SPACING;

/// The farthest a plate's outline lies from a position standing in the
/// plate, in world units: the plate's reach from its seed twice over, plus
/// the swing a chain takes off its straight edge, plus the warp the
/// position reads it through. What every reader of outlines has to be
/// able to see from a tile.
pub const OUTLINE_REACH: f64 = 2.0 * PLATE_REACH + PATH_SWING + WARP_SWING;

// ── Outlines ────────────────────────────────────────────────────────────────

/// The distance past which no reader of an outline tells one distance
/// from another: the longer of a range's end and the escarpment's run,
/// the reaches the wedges and the plateau saturate over. An edge farther
/// than this is read by a bound on its distance, never by its chain.
pub const OUTLINE_FAR: f64 = if ESCARPMENT > RANGE_END { ESCARPMENT } else { RANGE_END };

/// One edge of a plate's outline, as that plate reads it: its chain facing
/// into the plate, and the convergence share it carries where the plate is
/// continental and overrides on it, else none.
pub struct EdgeOutline {
    pub neighbour: PlateId,
    pub segments: Vec<Segment>,
    pub converge: f64,
    /// The wedge's cross-section under repose, as `scree_profile` reads it,
    /// empty where the edge carries no wedge.
    pub scree: Vec<f64>,
    /// The chain's bounding box, `[x0, y0, x1, y1]`: what a reader across
    /// the plate asks before measuring the chain.
    pub bounds: [f64; 4],
    /// The straight edge the chain is drawn on, `[x0, y0, x1, y1]`: the
    /// chain lies within the lattice path's swing of it.
    pub line: [f64; 4],
}

impl EdgeOutline {
    /// Distance from a position to the edge, and the side of its nearest
    /// segment, positive inside the plate: a walk of the chain.
    pub fn distance(&self, x: f64, y: f64) -> (f64, f64) {
        let mut best = (f64::MAX, 0.0);
        for s in &self.segments {
            let (d, side) = s.distance(x, y);
            if d < best.0 { best = (d, side) }
        }
        best
    }

    /// A lower bound on the distance from a position to the edge, without
    /// the chain: the farther of its distance to the chain's bounding box
    /// and its distance to the straight edge less the swing a chain takes
    /// off it.
    pub fn at_least(&self, x: f64, y: f64) -> f64 {
        let [x0, y0, x1, y1] = self.line;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let off_line = (x - x0 - t * dx).hypot(y - y0 - t * dy) - PATH_SWING;
        box_distance(self.bounds, x, y).max(off_line)
    }
}

/// Distance from a position to a box `[x0, y0, x1, y1]`, zero inside it.
fn box_distance(bounds: [f64; 4], x: f64, y: f64) -> f64 {
    let [x0, y0, x1, y1] = bounds;
    let dx = (x0 - x).max(x - x1).max(0.0);
    let dy = (y0 - y).max(y - y1).max(0.0);
    dx.hypot(dy)
}

/// A plate as its outline: every edge's chain facing inward, with what each
/// edge carries.
pub struct PlateOutline {
    pub id: PlateId,
    pub continental: bool,
    /// The plate's age, as `tectonic::Plate::age`.
    pub age: f64,
    pub edges: Vec<EdgeOutline>,
}

impl PlateOutline {
    /// Distance from a position to each edge, and whether the position lies
    /// inside the outline: on the inner side of the nearest segment, or on
    /// it. A chain runs through tile centres and lattice nodes, and a
    /// position on it stands in both its plates, which read the same ground
    /// there; in neither, it would read none. An edge that carries no
    /// wedge and stands past [`OUTLINE_FAR`] by its bound is read as that
    /// bound, never nearer than the truth and past every reach a reader
    /// tells distances apart over; its chain is walked only when the bound
    /// leaves it able to be the nearest edge.
    pub fn distances(&self, x: f64, y: f64) -> (Vec<f64>, bool) {
        let n = self.edges.len();
        let mut out = vec![0.0; n];
        let mut bounded = vec![false; n];
        let mut nearest = (f64::MAX, 0.0);
        for (i, e) in self.edges.iter().enumerate() {
            let bound = e.at_least(x, y);
            if e.converge <= 0.0 && bound >= OUTLINE_FAR {
                out[i] = bound;
                bounded[i] = true;
                continue;
            }
            let (d, side) = e.distance(x, y);
            if d < nearest.0 { nearest = (d, side) }
            out[i] = d;
        }
        for (i, e) in self.edges.iter().enumerate() {
            if bounded[i] && out[i] < nearest.0 {
                let (d, side) = e.distance(x, y);
                if d < nearest.0 { nearest = (d, side) }
                out[i] = d;
            }
        }
        (out, nearest.1 >= 0.0)
    }

    /// Distance to the nearest edge the plate does not override on.
    pub fn quiet_distance(&self, distances: &[f64]) -> f64 {
        self.edges
            .iter()
            .zip(distances)
            .filter(|(e, _)| e.converge <= 0.0)
            .map(|(_, d)| *d)
            .fold(f64::MAX, f64::min)
    }
}

/// Where a position stands: the plate whose outline holds it, its distance
/// to each of that plate's edges, and the position the outline was read
/// at, the tile's carried by the warp.
pub struct Standing<'a> {
    pub plate: &'a PlateOutline,
    pub distances: Vec<f64>,
    pub x: f64,
    pub y: f64,
}

/// How many reads [`Outlines::at`] remembers before it forgets them all:
/// a tile is read by the three layers standing on the outlines in turn,
/// and a few thousand tiles are in flight at once, so the memo needs to
/// hold that many and no more.
const RECENT_READS: usize = 16_384;

/// The outlines of every plate a set of tiles can stand in, built from the
/// resolved edges the motion layer published, read through the warp.
pub struct Outlines {
    plates: HashMap<PlateId, PlateOutline>,
    /// Every edge that carries a wedge, as its plate, its index in that
    /// plate's outline and its chain's bounding box: what a reader of the
    /// scree at rest past the fronts walks, the box here so a front out of
    /// reach costs no lookup.
    fronts: Vec<(PlateId, usize, [f64; 4])>,
    seed: u64,
    /// The last reads by position: the plate found, its distances, and the
    /// warped position. The layers above thrusting read one tile in turn,
    /// so the second and third reads of it are lookups.
    recent: DashMap<(u64, u64), (PlateId, Vec<f64>, f64, f64)>,
}

impl Outlines {
    /// Outlines from resolved edges: each edge joins both its plates'
    /// outlines, facing each, carrying its convergence for the plate that
    /// overrides on it. Corners join across each whole outline, so a position
    /// nearest a plate corner reads the corner's side.
    pub fn new<'a>(resolved: impl IntoIterator<Item = &'a BoundarySegment>, seed: u64) -> Self {
        let mut plates: HashMap<PlateId, PlateOutline> = HashMap::new();
        for s in resolved {
            let e = &s.edge;
            let over = if s.regime() == BoundaryRegime::Convergent { Some(s.overriding().id) } else { None };
            let converge = (s.convergence / CONVERGENCE_FULL).clamp(0.0, 1.0);
            for (plate, other) in [(&e.a, &e.b), (&e.b, &e.a)] {
                let left = Segment::is_left((e.x0, e.y0), (e.x1, e.y1), (plate.wx, plate.wy));
                let segments: Vec<Segment> = e
                    .chain
                    .windows(2)
                    .map(|w| Segment::along(node_world(w[0]), node_world(w[1]), left))
                    .collect();
                let bounds = e.chain.iter().map(|n| node_world(*n)).fold(
                    [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
                    |b, (x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
                );
                let outline = plates.entry(plate.id).or_insert_with(|| PlateOutline {
                    id: plate.id,
                    continental: plate.continental,
                    age: plate.age,
                    edges: Vec::new(),
                });
                // An oceanic plate carries no wedge: its crust has nothing to
                // shorten into sheets, and an island arc is volcanism. Unbuilt.
                let carries = over == Some(plate.id) && plate.continental;
                outline.edges.push(EdgeOutline {
                    neighbour: other.id,
                    segments,
                    converge: if carries { converge } else { 0.0 },
                    scree: if carries { scree_profile(sheets_of(converge)) } else { Vec::new() },
                    bounds,
                    line: [e.x0, e.y0, e.x1, e.y1],
                });
            }
        }
        for outline in plates.values_mut() {
            let mut all: Vec<Segment> = outline.edges.iter().flat_map(|e| e.segments.iter().copied()).collect();
            let nodes: Vec<(NodeKey, NodeKey)> = all
                .iter()
                .map(|s| (nearest_node(s.x0, s.y0), nearest_node(s.x1, s.y1)))
                .collect();
            join_at_nodes(&mut all, &nodes);
            let mut k = 0;
            for e in &mut outline.edges {
                for s in &mut e.segments {
                    *s = all[k];
                    k += 1;
                }
            }
        }
        let mut fronts: Vec<(PlateId, usize, [f64; 4])> = plates
            .values()
            .flat_map(|p| p.edges.iter().enumerate().filter(|(_, e)| e.converge > 0.0).map(move |(k, e)| (p.id, k, e.bounds)))
            .collect();
        fronts.sort_unstable_by_key(|f| (f.0, f.1));
        Self { plates, fronts, seed, recent: DashMap::new() }
    }

    /// The outlines of every plate within reach of a square box, built from
    /// the plate graph directly: what a view or a probe builds once.
    pub fn in_box(cx: f64, cy: f64, half: f64, seed: u64) -> Self {
        let radius = half * std::f64::consts::SQRT_2 + OUTLINE_REACH;
        let mut seen: HashSet<(PlateId, PlateId)> = HashSet::new();
        let mut resolved = Vec::new();
        for p in plates_near(cx, cy, radius, seed) {
            for e in edges_of(p.id, seed) {
                if seen.insert(e.ids()) {
                    resolved.push(resolve(&e, seed));
                }
            }
        }
        Self::new(&resolved, seed)
    }

    pub fn plate(&self, id: PlateId) -> Option<&PlateOutline> {
        self.plates.get(&id)
    }

    pub fn plates(&self) -> impl Iterator<Item = &PlateOutline> {
        self.plates.values()
    }

    /// The plate a position stands in, by its outline, with the position's
    /// distance to each of its edges, all read at the position the warp
    /// carries it to. The plate of the position's own lattice cell first,
    /// then its neighbours, then theirs: the outline decides, so the first
    /// guess need not be the nearest seed's, only usually right, and the
    /// cell costs no hash where the seed contest costs a score of them. A
    /// chain swings off its straight edge, so a position near an edge can
    /// stand across it, and where two corners lie within a swing of each
    /// other it can stand across two.
    pub fn at(&self, x: f64, y: f64) -> Option<Standing<'_>> {
        let key = (x.to_bits(), y.to_bits());
        if let Some(hit) = self.recent.get(&key) {
            let (id, distances, x, y) = &*hit;
            return self.plates.get(id).map(|plate| Standing { plate, distances: distances.clone(), x: *x, y: *y });
        }
        let found = self.find(x, y);
        if let Some(at) = &found {
            if self.recent.len() >= RECENT_READS {
                self.recent.clear();
            }
            self.recent.insert(key, (at.plate.id, at.distances.clone(), at.x, at.y));
        }
        found
    }

    /// [`Outlines::at`] without the memo.
    fn find(&self, x: f64, y: f64) -> Option<Standing<'_>> {
        let (x, y) = warp(x, y, self.seed);
        let home = plate_cell_for(x, y);
        let first = self.plates.get(&home)?;
        let (distances, inside) = first.distances(x, y);
        if inside { return Some(Standing { plate: first, distances, x, y }) }
        let mut tried = vec![home];
        for e in &first.edges {
            let Some(p) = self.plates.get(&e.neighbour) else { continue };
            tried.push(p.id);
            let (distances, inside) = p.distances(x, y);
            if inside { return Some(Standing { plate: p, distances, x, y }) }
        }
        for e in &first.edges {
            let Some(p) = self.plates.get(&e.neighbour) else { continue };
            for e in &p.edges {
                if tried.contains(&e.neighbour) { continue }
                let Some(p) = self.plates.get(&e.neighbour) else { continue };
                tried.push(p.id);
                let (distances, inside) = p.distances(x, y);
                if inside { return Some(Standing { plate: p, distances, x, y }) }
            }
        }
        None
    }
    /// Elevation the ranges add at a position, in z-levels: the strongest
    /// of the wedges of the plate the position stands in and the toes of
    /// scree that come to rest on it from its neighbours' wedges. Nothing
    /// on an oceanic plate, whose edges carry no convergence, but the toes.
    pub fn relief(&self, x: f64, y: f64) -> f64 {
        self.at(x, y).map_or(0.0, |at| self.relief_of(&at))
    }

    /// [`Outlines::relief`] where [`Outlines::at`] found a position
    /// standing, so a caller reading several layers at one position looks
    /// the plate up once.
    pub fn relief_of(&self, at: &Standing) -> f64 {
        RANGE_RISE * Self::wedges_of(at.plate, &at.distances).max(self.toes_on(at.plate, at.x, at.y))
    }

    /// The strongest wedge of the convergent edges of a plate at a position
    /// standing in it, as a share of a range's rise: each ending against
    /// every other edge of the plate. A wedge ends against a convergent edge
    /// as much as a quiet one: past the corner that edge's own wedge climbs
    /// from nothing, and the plate across it stands at nothing but scree.
    fn wedges_of(plate: &PlateOutline, distances: &[f64]) -> f64 {
        let mut best = 0.0f64;
        for (i, (e, d)) in plate.edges.iter().zip(distances).enumerate() {
            if e.converge <= 0.0 { continue }
            let family = scree_at(&e.scree, *d);
            if family <= 0.0 { continue }
            let others = distances.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, d)| *d).fold(f64::MAX, f64::min);
            best = best.max(family * smoothstep(others / RANGE_END));
        }
        best
    }

    /// The highest toe of scree at rest at a position standing outside the
    /// plate whose wedge it is, as a share of a range's rise: every wedge of
    /// every other plate in reach, read past its front at the position's
    /// distance to its edge, and ended against that plate's other edges as
    /// the wedge is. Read from every front in reach, not only across the
    /// edge the position is nearest, so the plates meeting at a corner all
    /// read the same toes around it and nothing steps.
    fn toes_on(&self, plate: &PlateOutline, x: f64, y: f64) -> f64 {
        let mut best = 0.0f64;
        for &(id, k, bounds) in &self.fronts {
            if id == plate.id || box_distance(bounds, x, y) >= SCREE_REACH { continue }
            let p = &self.plates[&id];
            let front = &p.edges[k];
            let family = scree_at(&front.scree, -front.distance(x, y).0);
            if family <= 0.0 { continue }
            let others = p
                .edges
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .map(|(_, o)| if o.at_least(x, y) >= RANGE_END { RANGE_END } else { o.distance(x, y).0 })
                .fold(f64::MAX, f64::min);
            best = best.max(family * smoothstep(others / RANGE_END));
        }
        best
    }
}

pub fn smoothstep(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Sheets an edge stacks for a convergence share: the count is the
/// shortening over a constant, partial at the end.
pub fn sheets_of(converge: f64) -> f64 {
    converge.clamp(0.0, 1.0) * WEDGE_SHEETS
}

/// Sheets in from the front at a distance: one over the local spacing,
/// accumulated. Spacing is [`RANGE_SPACING`] at the front and tightens by
/// [`BELT_SHORTENING`] over [`SHORTENING_REACH`], the internal shortening a
/// sheet accumulates with its age in the wedge; deeper than that a sheet
/// has been in the wedge for the belt's whole life. Accumulating keeps
/// neighbouring sheets a local spacing apart; a coordinate scaled by the
/// local spacing moves the n-th sheet n times as far as the spacing changed.
pub fn sheets_in(room: f64) -> f64 {
    let aged = |d: f64| {
        -(1.0 - BELT_SHORTENING * d / SHORTENING_REACH).ln() * SHORTENING_REACH
            / (RANGE_SPACING * BELT_SHORTENING)
    };
    if room <= SHORTENING_REACH {
        aged(room)
    } else {
        aged(SHORTENING_REACH) + (room - SHORTENING_REACH) / (RANGE_SPACING * (1.0 - BELT_SHORTENING))
    }
}

/// Distance from the front at which a wedge of `sheets` ends: the inverse
/// of [`sheets_in`].
pub fn rim_of(sheets: f64) -> f64 {
    let at_reach = sheets_in(SHORTENING_REACH);
    if sheets <= at_reach {
        SHORTENING_REACH / BELT_SHORTENING
            * (1.0 - (-sheets * RANGE_SPACING * BELT_SHORTENING / SHORTENING_REACH).exp())
    } else {
        SHORTENING_REACH + (sheets - at_reach) * RANGE_SPACING * (1.0 - BELT_SHORTENING)
    }
}

/// One sheet's cross-section at a signed distance from its crest, positive
/// toward the front: the steep flank faces the front, and the taper reaches
/// zero at the trough it shares with the next sheet.
fn wedge(d: f64) -> f64 {
    let half = if d >= 0.0 { RANGE_STEEP_HALF_WIDTH } else { RANGE_GRADED_HALF_WIDTH };
    let u = d.abs() / half;
    if u >= 1.0 { 0.0 } else { (1.0 - u * u).powf(TAPER_EXPONENT) }
}

/// The wedge across a belt, in [0, 1] of a range's rise, at a coordinate
/// that runs from the frontal sheet's crest toward the front: `sheets`
/// sheets one spacing apart, each meeting the next at a trough, every steep
/// flank facing the front, the last sheet standing at its partial share and
/// nothing past it. A caller with a tighter local spacing hands a coordinate
/// stretched by the ratio.
fn range_family(across: f64, sheets: f64) -> f64 {
    let k = (-across / RANGE_SPACING).round();
    let mut h = 0.0;
    for j in [k - 1.0, k, k + 1.0] {
        if j < 0.0 { continue }
        let weight = (sheets - j).clamp(0.0, 1.0);
        if weight <= 0.0 { continue }
        h += weight * wedge(across + j * RANGE_SPACING);
    }
    h.min(1.0)
}

/// The wedge's share of a range's rise at a distance `d` from the front, as
/// the structure stands: the family at the coordinate the sheets' tightening
/// puts that distance at.
fn family_at(d: f64, sheets: f64) -> f64 {
    range_family(RANGE_STEEP_HALF_WIDTH - RANGE_SPACING * sheets_in(d), sheets)
}

/// A wedge's cross-section with its surface held under repose, read every
/// [`REPOSE_STEP`] from [`SCREE_START`]: the structure on that grid,
/// dilated by a cone at the repose grade, so every value is the highest of
/// any grid point's share less the grade times the distance to it. Two
/// sweeps do it exactly: forward, a value is at least the one before it
/// less a step's grade, and back the same. Read straight between points,
/// the surface is continuous, a chord of a slope no steeper than repose is
/// no steeper, and a crest stands under its structure by a chord's error.
fn scree_profile(sheets: f64) -> Vec<f64> {
    let n = ((WEDGE_REACH + 2.0 * SCREE_REACH) / REPOSE_STEP).ceil() as usize + 2;
    let mut h: Vec<f64> = (0..n).map(|i| family_at(SCREE_START + i as f64 * REPOSE_STEP, sheets)).collect();
    let fall = REPOSE_GRADE / RANGE_RISE * REPOSE_STEP;
    for i in 1..n {
        h[i] = h[i].max(h[i - 1] - fall);
    }
    for i in (0..n - 1).rev() {
        h[i] = h[i].max(h[i + 1] - fall);
    }
    h
}

/// The wedge's share at a distance `d` from the front with its surface
/// held under repose, read from its [`scree_profile`] straight between the
/// points; nothing past either end of it.
fn scree_at(profile: &[f64], d: f64) -> f64 {
    let x = (d - SCREE_START) / REPOSE_STEP;
    if x < 0.0 {
        return 0.0;
    }
    let i = x.floor() as usize;
    let Some(&h0) = profile.get(i) else { return 0.0 };
    let h1 = profile.get(i + 1).copied().unwrap_or(0.0);
    h0 + (h1 - h0) * (x - i as f64)
}

/// The wedge's share at a distance from the front with its surface held
/// under repose, as [`scree_at`] reads it from a profile built for the
/// call: what the tests hold, and what an edge's profile is.
#[cfg(test)]
fn family_at_repose(d: f64, sheets: f64) -> f64 {
    scree_at(&scree_profile(sheets), d)
}

// ── The event ───────────────────────────────────────────────────────────────

pub struct ThrustingEvent;

impl ThrustingEvent {
    pub fn new() -> Self { ThrustingEvent }
}

impl Default for ThrustingEvent {
    fn default() -> Self { Self::new() }
}

/// The outlines each cell of the graph lattice reads, published by the
/// thrusting layer and read by every layer standing on the outlines, so
/// one tile's plate is found once for all of them.
#[derive(Default)]
pub struct OutlineIndex {
    cells: HashMap<CellId, Arc<Outlines>>,
}

impl CellIndex for OutlineIndex {
    type Cell = Arc<Outlines>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }
}

impl EventIndex for OutlineIndex {
    fn source_scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// Nothing stands at a tile: the outlines are the plate graph's.
    fn tiles(&self, _cell_ids: &[CellId]) -> Vec<(i32, i32)> { Vec::new() }

    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

/// The outlines a cell of the graph lattice reads: the entry the thrusting
/// layer published for it. What thrusting itself and every layer above at
/// the same scale ask in `prepare`.
pub fn outlines_for(scope: &CellScope) -> Arc<Outlines> {
    scope
        .read::<OutlineIndex>()
        .and_then(|idx| idx.entry(scope.cell()).cloned())
        .unwrap_or_else(|| Arc::new(Outlines::new(&[], scope.seed())))
}

/// The outlines of every plate a cell's tiles can stand in: the resolved
/// edges of the graph's cells under the cell and its ring.
pub fn outlines_of(scope: &CellScope) -> Outlines {
    let resolved = scope.read::<PlateBoundaryIndex>();
    Outlines::new(resolved.iter().flat_map(|idx| idx.entries().flatten()), scope.seed())
}

impl WorldEvent for ThrustingEvent {
    fn name(&self) -> &str { "thrusting" }
    fn scale(&self) -> u32 { GRAPH_CELL_SCALE }

    /// Nothing originates here.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<OutlineIndex>();
    }

    /// Nothing to place: the ranges are read off the plate graph. The
    /// cell's outlines are published for every layer that reads them.
    fn deform(&self, scope: &CellScope) {
        scope.publish::<OutlineIndex>(Arc::new(outlines_of(scope)));
    }

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(outlines_for(scope))
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        cell: &(dyn Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let outlines = cell.downcast_ref::<Arc<Outlines>>()?;
        let (wx, wy) = hex_to_world(q, r);
        let rise = outlines.relief(wx, wy);
        if rise <= 0.0 { return None }
        let gradient = gradient_of(wx, wy, rise, |x, y| outlines.relief(x, y));
        Some(TileOutput { elevation_delta: rise, gradient, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::plates::{stretch_at, unwarp};
    use crate::tectonic::plate_at;

    const S: u64 = 0x9E3779B97F4A7C15;

    /// The home continent's centre, among its collision fronts and active
    /// margins, where fronts carry scree to the front and past it.
    const BELT: (f64, f64) = (98_000.0, -2_000.0);

    /// A range stands at a thrust wedge's structural dips: the forelimb at the
    /// dip it is built from, and the backlimb inside the 10–25° the same
    /// geology states.
    #[test]
    fn range_stands_at_its_structural_dips() {
        assert!((FORELIMB_DIP_TAN - FORELIMB_DIP_DEGREES.to_radians().tan()).abs() < 1e-12);
        let forelimb = (RANGE_RISE * RISE / RANGE_STEEP_HALF_WIDTH).atan().to_degrees();
        let backlimb = (RANGE_RISE * RISE / RANGE_GRADED_HALF_WIDTH).atan().to_degrees();
        assert!((forelimb - FORELIMB_DIP_DEGREES).abs() < 1e-9, "forelimb dips {forelimb}");
        assert!((10.0..=25.0).contains(&backlimb), "backlimb dips {backlimb}, outside 10–25°");
    }

    /// A range's surface stands no steeper than repose anywhere across the
    /// belt, though its structure does, and every crest keeps its height:
    /// the forelimb becomes scree from the crest down.
    #[test]
    fn a_range_surface_stands_under_repose() {
        assert!((REPOSE_TAN - REPOSE_DEGREES.to_radians().tan()).abs() < 1e-12);
        let structure = |d: f64| RANGE_RISE * family_at(d, WEDGE_SHEETS);
        let surface = |d: f64| RANGE_RISE * family_at_repose(d, WEDGE_SHEETS);
        let (mut steepest_structure, mut steepest_surface) = (0.0f64, 0.0f64);
        let mut d = -200.0;
        while d < WEDGE_REACH + 600.0 {
            steepest_structure = steepest_structure.max((structure(d + 1.0) - structure(d)).abs());
            steepest_surface = steepest_surface.max((surface(d + 1.0) - surface(d)).abs());
            assert!(surface(d) >= structure(d) - 1.0, "scree under the structure by more than a chord's error at {d}");
            d += 1.0;
        }
        assert!(steepest_structure > REPOSE_GRADE, "the structure never exceeded repose: {steepest_structure}");
        assert!(steepest_surface <= REPOSE_GRADE + 1e-6, "the surface exceeds repose: {steepest_surface} over {REPOSE_GRADE}");
        // The crests: wherever the structure stands highest, the surface
        // stands within a chord's error of it.
        let mut d = 0.0;
        let (mut top, mut at) = (0.0f64, 0.0);
        while d < WEDGE_REACH {
            if structure(d) > top { top = structure(d); at = d; }
            d += 1.0;
        }
        assert!((top - RANGE_RISE).abs() < 0.1, "no crest stands full: {top}");
        let lost = top - surface(at);
        assert!(lost >= -1e-9 && lost <= 1.0, "a crest lowered by repose by {lost} z");
    }

    /// Neighbouring sheets meet at the trough: the steep and graded
    /// half-widths sum to one spacing, the family is a share, a crest sits at
    /// the origin, the frontal sheet's forelimb reaches the ground one steep
    /// half-width toward the front, and the last sheet stands at its partial
    /// share with nothing past it.
    #[test]
    fn sheets_meet_at_the_trough() {
        assert!((RANGE_STEEP_HALF_WIDTH + RANGE_GRADED_HALF_WIDTH - RANGE_SPACING).abs() < 1e-9);
        for across in [0.0, 350.0, 700.0, 1_400.0, 2_100.0, 3_000.0, -3_000.0] {
            let h = range_family(across, WEDGE_SHEETS);
            assert!((0.0..=1.0).contains(&h), "family height {h} at {across}");
        }
        assert!((range_family(0.0, WEDGE_SHEETS) - 1.0).abs() < 1e-9);
        assert!(range_family(RANGE_STEEP_HALF_WIDTH, WEDGE_SHEETS).abs() < 1e-9, "the front is one steep half-width out");
        assert!(range_family(-RANGE_GRADED_HALF_WIDTH, WEDGE_SHEETS).abs() < 1e-9, "the trough is one graded half-width in");
        assert!((range_family(-(WEDGE_SHEETS - 1.0) * RANGE_SPACING, WEDGE_SHEETS) - 1.0).abs() < 1e-9, "the last crest stands full");
        assert!(range_family(-WEDGE_SHEETS * RANGE_SPACING, WEDGE_SHEETS).abs() < 1e-9, "the last backlimb rests on the plateau");
        assert!((range_family(-2.0 * RANGE_SPACING, 2.5) - 0.5).abs() < 1e-9, "a half sheet stands at half height");
        assert!(range_family(-3.0 * RANGE_SPACING, 2.5).abs() < 1e-9, "nothing stands past a partial wedge");
    }

    /// Sheets count up from the front at the thickness-set spacing, tighten
    /// with age to the belt's shortening, hold that spacing past the
    /// shortening reach, and the rim is where the count says.
    #[test]
    fn sheets_tighten_with_age() {
        assert!(sheets_in(0.0).abs() < 1e-12);
        let local = |d: f64| 1.0 / (sheets_in(d + 0.5) - sheets_in(d - 0.5));
        assert!((local(0.5) - RANGE_SPACING).abs() < 1.0, "front spacing {}", local(0.5));
        let aged = RANGE_SPACING * (1.0 - BELT_SHORTENING);
        assert!((local(SHORTENING_REACH + 100.0) - aged).abs() < 1.0);
        let mut last = local(0.5);
        for i in 1..=40 {
            let s = local(i as f64 * SHORTENING_REACH / 20.0);
            assert!(s <= last + 1e-6, "spacing widened to {s} from {last}");
            last = s;
        }
        for sheets in [0.5, 1.0, 2.5, WEDGE_SHEETS] {
            let rim = rim_of(sheets);
            assert!((sheets_in(rim) - sheets).abs() < 1e-9, "rim of {sheets} sheets at {rim} holds {}", sheets_in(rim));
        }
        assert!(rim_of(WEDGE_SHEETS) <= WEDGE_REACH, "the full wedge outreaches WEDGE_REACH");
    }

    /// Every chain node lies within the stated swing of the straight edge it
    /// draws, and the swing is not slack.
    #[test]
    fn chains_stay_near_their_edges() {
        let mut worst: f64 = 0.0;
        for cq in -12..=12 {
            for cr in -12..=12 {
                for e in edges_of((cq, cr), S) {
                    let line = Segment::along((e.x0, e.y0), (e.x1, e.y1), true);
                    for n in &e.chain {
                        let (x, y) = node_world(*n);
                        worst = worst.max(line.distance(x, y).0);
                    }
                }
            }
        }
        assert!(worst <= PATH_SWING, "a chain node {worst:.0} off its edge, past PATH_SWING");
        assert!(worst > 0.5 * PATH_SWING, "PATH_SWING is slack for the edges: the farthest node is {worst:.0}");
    }

    /// The ranges' relief is continuous across every front and around every
    /// corner of one: the frontal scree comes to rest across the front, and
    /// no step stands anywhere steeper than the scree and a range's end
    /// together can make, stretched by the warp the position reads it
    /// through. Every position is where a tile stands to read the point
    /// on the chain: the chain's point unwarped.
    #[test]
    fn scree_comes_to_rest_across_the_front() {
        let outlines = Outlines::in_box(BELT.0, BELT.1, 30_000.0, S);
        let steepest = REPOSE_GRADE + RANGE_RISE * 1.5 / RANGE_END;
        let steepest_at = |x: f64, y: f64| steepest * stretch_at(x, y, S).0;
        let (mut fronts, mut toes) = (0, 0);
        for plate in outlines.plates() {
            for e in plate.edges.iter().filter(|e| e.converge > 0.0) {
                let s = e.segments[e.segments.len() / 2];
                let (mx, my) = s.mid();
                // Only fronts the box holds both sides of.
                if (mx - BELT.0).abs() > 25_000.0 || (my - BELT.1).abs() > 25_000.0 { continue }
                fronts += 1;
                // Across the front along the plate-facing normal, finely:
                // scree that reaches the front rests past it too.
                let (mx, my) = unwarp(mx, my, S);
                let at = |t: f64| outlines.relief(mx + s.nx * t, my + s.ny * t);
                if scree_at(&e.scree, -10.0) > 0.0 {
                    toes += 1;
                    assert!(at(-20.0) > 0.0, "no scree rests across the front of {:?}", plate.id);
                }
                let step = 2.0;
                let mut t = -400.0;
                while t < 400.0 {
                    let jump = (at(t + step) - at(t)).abs();
                    let bound = steepest_at(mx + s.nx * t, my + s.ny * t) * step;
                    assert!(jump <= bound + 1e-6, "a step of {jump} across the front of {:?} at {t}", plate.id);
                    t += step;
                }
                // Around the corner the front ends at, where the toe caps,
                // coarsely: a step there is a range's end or a toe, tens of
                // levels, not a chord's error.
                let (cx, cy) = unwarp(e.segments[0].x0, e.segments[0].y0, S);
                let step = 6.0;
                let n = (240.0 / step) as i32;
                for i in -n..=n {
                    for j in -n..=n {
                        let (x, y) = (cx + i as f64 * step, cy + j as f64 * step);
                        let here = outlines.relief(x, y);
                        let east = outlines.relief(x + step, y);
                        let north = outlines.relief(x, y + step);
                        let jump = (east - here).abs().max((north - here).abs());
                        let bound = steepest_at(x, y) * step;
                        assert!(jump <= bound + 1e-6, "a step of {jump} at a corner of {:?}, {i} {j} from it", plate.id);
                    }
                }
            }
        }
        assert!(fronts > 0 && toes > 0, "{fronts} fronts in the box, {toes} with scree at the front");
    }

    /// Every position stands in exactly the plate whose outline encloses it,
    /// found from the nearest seed's plate or one of its neighbours.
    #[test]
    fn every_position_stands_in_one_plate() {
        let outlines = Outlines::in_box(0.0, 0.0, 20_000.0, S);
        let mut swapped = 0;
        for i in 0..80 {
            for j in 0..80 {
                let (x, y) = (i as f64 * 500.0 - 20_000.0, j as f64 * 500.0 - 20_000.0);
                let at = outlines.at(x, y).expect("a position with no plate");
                if at.plate.id != plate_at(at.x, at.y, S).id { swapped += 1 }
                let inside: Vec<PlateId> = outlines.plates().filter(|p| p.distances(at.x, at.y).1).map(|p| p.id).collect();
                assert_eq!(inside.len(), 1, "position ({x}, {y}) inside {inside:?}");
                assert_eq!(inside[0], at.plate.id);
            }
        }
        assert!(swapped > 0, "no position stood across a chain from its seed's plate");
    }
}
