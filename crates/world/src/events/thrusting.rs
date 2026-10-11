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
//! sheet is continuous for its length as a structure: a range runs its whole
//! edge and ends against the plate's other edges over one sheet spacing,
//! straight and on the lattice. Its height rises and falls along strike, a
//! sheet plunging from culminations to saddles above a floor at no
//! wavelength shorter than two sheet spacings, so no sheet breaks into
//! lenses and a typical sheet stands near its full height; neighbouring
//! sheets culminate out of step, so a belt is a chain of culminations and
//! saddles, not ridges rising and falling together. A sheet's structure
//! stands at a thrust wedge's dips, each flank falling straight from the
//! crest, the forelimb at 50°; its surface does not, since rock steeper
//! than debris rests at sheds, so the cross-section is held under a cone at
//! repose from every point of it: the forelimb becomes scree reaching
//! further from the crest and buries the trough's near side, the frontal
//! sheet's runs out past the front onto the plate going under, and the
//! backlimb is barely touched. Under a lower crest the scree is shorter,
//! never gentler. Repose is over the one level a step climbs, so a range
//! is climbed by traverse and crossed at its passes. The crest is a
//! hillslope's, rounded to the curvature its height erodes at
//! ([`crate::hillslope`]): sharp at a culmination, broad in a saddle.
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
//! **Height plunges along strike no faster than two sheet spacings.**
//! Scaled by a strain octave, ranges broke into lenses a few thousand units
//! long, and scaled by a field's whole amplitude, the typical range stood at
//! a fraction of its height; the plunge keeps above a floor, near full
//! height along most of a sheet, and is read along the straight edge, whose
//! coordinate is continuous where the chain's bends are not.
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
use crate::noise::{hash_channel, simplex_2d};
use crate::lattice::{nearest_node, node_world, NodeKey, PATH_SWING};
use crate::tectonic::{edges_of, plate_cell_for, plates_near, PlateId, PLATE_REACH};
use super::plates::{warp, WARP_SWING};
use crate::{hex_to_world, smoothstep, RISE, SEA_MAX_DEPTH};
use crate::hillslope::{crest_curvature, erosion_rate};
use super::index::{CellId, CellIndex, IndexRegistry};
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
pub const SCREE_REACH: f64 = RANGE_RISE / REPOSE_GRADE;

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
    /// The wedge's cross-section under repose, sheet by sheet at each
    /// plunge, none where the edge carries no wedge.
    pub faces: Option<Faces>,
    /// The chain's bounding box, `[x0, y0, x1, y1]`: what a reader across
    /// the plate asks before measuring the chain.
    pub bounds: [f64; 4],
    /// The straight edge the chain is drawn on, `[x0, y0, x1, y1]`: the
    /// chain lies within the lattice path's swing of it.
    pub line: [f64; 4],
}

impl EdgeOutline {
    /// How far along the edge's strike a position lies, in world units
    /// from the start of its straight edge: the position's foot on that
    /// line. The chain is a lattice path within a swing of the line, and the
    /// distance along it to the foot on its nearest segment leaps across
    /// the inside of every bend, by a thousand units a sheet in; the line's
    /// is continuous everywhere, the strike exactly. What every field along
    /// a range is read in.
    pub fn along(&self, x: f64, y: f64) -> f64 {
        let [x0, y0, x1, y1] = self.line;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = dx.hypot(dy);
        if len > 0.0 { ((x - x0) * dx + (y - y0) * dy) / len } else { 0.0 }
    }

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
/// to each of that plate's edges, the position itself, and the position
/// the outline was read at, the tile's carried by the warp.
pub struct Standing<'a> {
    pub wx: f64,
    pub wy: f64,
    pub plate: &'a PlateOutline,
    /// The position's distance to each edge of the plate, in the outline's
    /// edge order, as [`PlateOutline::distances`] reads them: exact for an
    /// edge that carries a wedge; for a quiet edge exact where it lies
    /// within [`OUTLINE_FAR`], else a lower bound on it, never nearer than
    /// the truth.
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
                    faces: carries.then(|| {
                        let sheets = sheets_of(converge);
                        let plunges = (0..sheets.ceil() as usize).map(|j| plunge_seed(plate.id, other.id, j, seed)).collect();
                        Faces::new(sheets, plunges)
                    }),
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
    pub fn at(&self, wx: f64, wy: f64) -> Option<Standing<'_>> {
        let key = (wx.to_bits(), wy.to_bits());
        if let Some(hit) = self.recent.get(&key) {
            let (id, distances, x, y) = &*hit;
            return self.plates.get(id).map(|plate| Standing { wx, wy, plate, distances: distances.clone(), x: *x, y: *y });
        }
        let found = self.find(wx, wy);
        if let Some(at) = &found {
            if self.recent.len() >= RECENT_READS {
                self.recent.clear();
            }
            self.recent.insert(key, (at.plate.id, at.distances.clone(), at.x, at.y));
        }
        found
    }

    /// [`Outlines::at`] without the memo.
    fn find(&self, wx: f64, wy: f64) -> Option<Standing<'_>> {
        let (x, y) = warp(wx, wy, self.seed);
        let home = plate_cell_for(x, y);
        let first = self.plates.get(&home)?;
        let (distances, inside) = first.distances(x, y);
        if inside { return Some(Standing { wx, wy, plate: first, distances, x, y }) }
        let mut tried = vec![home];
        for e in &first.edges {
            let Some(p) = self.plates.get(&e.neighbour) else { continue };
            tried.push(p.id);
            let (distances, inside) = p.distances(x, y);
            if inside { return Some(Standing { wx, wy, plate: p, distances, x, y }) }
        }
        for e in &first.edges {
            let Some(p) = self.plates.get(&e.neighbour) else { continue };
            for e in &p.edges {
                if tried.contains(&e.neighbour) { continue }
                let Some(p) = self.plates.get(&e.neighbour) else { continue };
                tried.push(p.id);
                let (distances, inside) = p.distances(x, y);
                if inside { return Some(Standing { wx, wy, plate: p, distances, x, y }) }
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
        let wedges = Self::wedges_of(at.plate, &at.distances, |e, faces, d| faces.at(d, e.along(at.x, at.y)));
        let toes = self.toes_on(at.plate, at.x, at.y, SCREE_REACH, |front, faces, d| faces.at(-d, front.along(at.x, at.y)));
        RANGE_RISE * wedges.max(toes)
    }

    /// The height of the crest of the range a position standing `at` lies
    /// on, in z-levels, as relief and at its plunge: the hillslope's whole
    /// height, which it erodes at, from its crest to its toe alike, and
    /// ended with its wedge. Nothing off the ranges and their scree.
    pub fn crest_of(&self, at: &Standing) -> f64 {
        let wedges = Self::wedges_of(at.plate, &at.distances, |e, faces, d| faces.crest_at(d, e.along(at.x, at.y)));
        let toes = self.toes_on(at.plate, at.x, at.y, SCREE_REACH, |front, faces, d| faces.crest_at(-d, front.along(at.x, at.y)));
        RANGE_RISE * wedges.max(toes)
    }

    /// How near a position standing `at` lies to the front of a wedge, as a
    /// share: one at a front, falling smoothly to nothing `own` in from it
    /// on the plate whose wedge it is and `under` out from it on any other,
    /// and ended against the wedge's plate's other edges as the wedge is;
    /// the nearest front's. Past a front it is read as the toes are, from
    /// every front within `under`, so it is one at the front from both
    /// sides and continuous around its corners.
    pub fn front_share(&self, at: &Standing, own: f64, under: f64) -> f64 {
        let near = |d: f64, reach: f64| 1.0 - smoothstep(d.max(0.0) / reach);
        let wedges = Self::wedges_of(at.plate, &at.distances, |_, _, d| near(d, own));
        wedges.max(self.toes_on(at.plate, at.x, at.y, under, |_, _, d| near(d, under)))
    }

    /// The height the ranges' structure stands at a position where
    /// [`Outlines::at`] found it standing, in z-levels: the strongest wedge
    /// as it was built at its plunge, with no scree swept over it. What
    /// lithology strips the column by.
    pub fn structure_of(&self, at: &Standing) -> f64 {
        RANGE_RISE * Self::wedges_of(at.plate, &at.distances, |e, faces, d| faces.structure_at(d, e.along(at.x, at.y)))
    }

    /// The strongest wedge of the convergent edges of a plate at a position
    /// standing in it, as a share of a range's rise, each read off its
    /// faces by `share` at the distance to its front: each ending against
    /// every other edge of the plate. A wedge ends against a convergent edge
    /// as much as a quiet one: past the corner that edge's own wedge climbs
    /// from nothing, and the plate across it stands at nothing but scree.
    fn wedges_of(plate: &PlateOutline, distances: &[f64], share: impl Fn(&EdgeOutline, &Faces, f64) -> f64) -> f64 {
        let mut best = 0.0f64;
        for (i, (e, d)) in plate.edges.iter().zip(distances).enumerate() {
            let Some(faces) = &e.faces else { continue };
            let family = share(e, faces, *d);
            if family <= 0.0 { continue }
            let others = distances.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, d)| *d).fold(f64::MAX, f64::min);
            best = best.max(family * smoothstep(others / RANGE_END));
        }
        best
    }

    /// The highest toe at a position standing outside the plate whose
    /// wedge it is, as `share` reads each front at the position's distance
    /// out past it: every wedge of every other plate whose front lies
    /// within `reach`, ended against that plate's other edges as the wedge
    /// is. Read from every front in reach, not only across the edge the
    /// position is nearest, so the plates meeting at a corner all read the
    /// same toes around it and nothing steps.
    fn toes_on(&self, plate: &PlateOutline, x: f64, y: f64, reach: f64, share: impl Fn(&EdgeOutline, &Faces, f64) -> f64) -> f64 {
        let mut best = 0.0f64;
        for &(id, k, bounds) in &self.fronts {
            if id == plate.id || box_distance(bounds, x, y) >= reach { continue }
            let p = &self.plates[&id];
            let front = &p.edges[k];
            let Some(faces) = &front.faces else { continue };
            let family = share(front, faces, front.distance(x, y).0);
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
/// toward the front: each flank falls straight at its dip from the crest
/// to nothing at the trough it shares with the next sheet, the steep flank
/// facing the front. The crest is a corner; its hillslope rounds it.
fn wedge(d: f64) -> f64 {
    let half = if d >= 0.0 { RANGE_STEEP_HALF_WIDTH } else { RANGE_GRADED_HALF_WIDTH };
    (1.0 - d.abs() / half).max(0.0)
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

/// A profile on a grid held under the falls its steps allow, in place:
/// every value of `h`, the structure, raised to the highest of any point's
/// less the falls between, where `falls[i]` is the most the surface falls
/// over the step between points `i - 1` and `i`. Two sweeps do it exactly:
/// forward, a value is at least the one before it less its step's fall,
/// and back the same. Read straight between points, the surface is
/// continuous, a chord of a slope within its falls is within them, and a
/// crest stands under its structure by a chord's error.
pub(crate) fn sweep(h: &mut [f64], falls: &[f64]) {
    for i in 1..h.len() {
        h[i] = h[i].max(h[i - 1] - falls[i]);
    }
    for i in (1..h.len()).rev() {
        h[i - 1] = h[i - 1].max(h[i] - falls[i]);
    }
}

/// A profile on a grid with its crest rounded to a curvature, in place:
/// the highest profile beneath it that a parabola of curvature `c`, per
/// step squared, fits under at every point. The profile rises to one crest
/// and falls from it, straight or curving the other way, as a sheet's
/// does. The crest, sharper than `c`, lowers to the parabola tangent to its
/// flanks; the rest is untouched; and no slope steepens, so a profile under
/// repose stays under it.
pub(crate) fn round(h: &mut [f64], c: f64) {
    let steepest = h.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
    if c <= 0.0 || steepest <= 0.0 {
        return;
    }
    // Past this many steps the parabola turns faster than the steepest
    // slope, so no point further off can decide either pass; a pass's
    // result is no steeper than what it read.
    let reach = (steepest / c).ceil() as usize + 1;
    // The cap lies within a reach of its apex and the apex within a reach
    // of the highest point, so outside twice that nothing moves; the passes
    // read twice that again, so the window's own ends decide nothing kept.
    let top = (0..h.len()).fold(0, |best, i| if h[i] > h[best] { i } else { best });
    let (lo, hi) = (top.saturating_sub(4 * reach), (top + 4 * reach + 1).min(h.len()));
    let window = &mut h[lo..hi];
    let keep = (top - lo).saturating_sub(2 * reach)..(top - lo + 2 * reach + 1).min(window.len());
    let n = window.len();
    if n < 2 {
        return;
    }
    // Each pass takes the parabola's extreme over every segment read
    // straight, not at the points alone, or a straight flank whose slope
    // is no whole number of the parabola's steps comes out lowered.
    let pass = |v: &[f64], sign: f64| -> Vec<f64> {
        (0..n)
            .map(|i| {
                let t = i as f64;
                let at = (i.saturating_sub(reach)..(i + reach).min(n - 1)).map(|k| {
                    let s = v[k + 1] - v[k];
                    let x = (t - sign * s / c).clamp(k as f64, (k + 1) as f64);
                    v[k] + s * (x - k as f64) + sign * 0.5 * c * (x - t) * (x - t)
                });
                if sign > 0.0 { at.fold(f64::MAX, f64::min) } else { at.fold(f64::MIN, f64::max) }
            })
            .collect()
    };
    let rounded = pass(&pass(window, 1.0), -1.0);
    for i in keep {
        window[i] = window[i].min(rounded[i]);
    }
}

/// The curvature a sheet's crest is rounded to, per profile step squared
/// as a share of a range's rise, where the crest stands `relief` z-levels
/// high: the hillslope its height erodes at.
fn crest_rounding(relief: f64) -> f64 {
    crest_curvature(erosion_rate(relief)) * REPOSE_STEP * REPOSE_STEP / RANGE_RISE
}

/// A profile's share at a distance `d` from the front, read straight
/// between its points, where `point` is its share at a point of the grid
/// and nothing past its end; nothing before the grid begins.
fn between(d: f64, point: impl Fn(usize) -> f64) -> f64 {
    let x = (d - SCREE_START) / REPOSE_STEP;
    if x < 0.0 {
        return 0.0;
    }
    let i = x.floor() as usize;
    let (h0, h1) = (point(i), point(i + 1));
    h0 + (h1 - h0) * (x - i as f64)
}

/// A wedge's cross-section held under repose, sheet by sheet, one profile
/// for each share of its height a sheet's plunge leaves it, each crest
/// rounded to the curvature that height erodes at.
///
/// The sheets' wedges are disjoint across strike, each meeting the next at
/// a trough, so the structure is the highest of them, and the highest of
/// their sweeps is the sweep of the whole: each sheet is swept alone at its
/// own plunge, and a position reads every sheet whose scree reaches it.
/// The plunge scales a sheet's structure before its sweep, since repose is
/// a rule for debris: under a lower crest the scree is shorter, never
/// gentler. A sheet's profiles are built at every [`PLUNGE_STEP`] of share
/// from [`PLUNGE_FLOOR`] to one, and a position blends the two bracketing
/// its sheet's plunge, so the surface is continuous along strike.
pub struct Faces {
    sheets: Vec<Sheet>,
    /// The plunge field's seed for each sheet, as [`Faces::plunge`] reads it.
    plunges: Vec<u64>,
}

/// One sheet of a wedge on the profiles' grid, over the points its
/// structure and its scree can reach.
struct Sheet {
    /// The grid point the sheet's span begins at.
    start: usize,
    /// The sheet's structure at full height over its span, nothing off its
    /// own wedge.
    structure: Vec<f64>,
    /// The height its crest stands at, at full height: one, or a partial
    /// sheet's share.
    crest: f64,
    /// The sheet's profile at each plunge, one after another, each as long
    /// as `structure`.
    profiles: Vec<f64>,
}

impl Faces {
    /// The faces of a wedge of `sheets`, the sheets plunging by the fields
    /// `plunges` seeds. A sheet's scree is spent within a scree reach of
    /// its wedge, so its span ends there.
    fn new(sheets: f64, plunges: Vec<u64>) -> Self {
        let full = ((WEDGE_REACH + 2.0 * SCREE_REACH) / REPOSE_STEP).ceil() as usize + 2;
        let reach = (SCREE_REACH / REPOSE_STEP).ceil() as usize + 1;
        let family: Vec<f64> = (0..full).map(|i| family_at(SCREE_START + i as f64 * REPOSE_STEP, sheets)).collect();
        let sheet_of = |i: usize| sheets_in(SCREE_START + i as f64 * REPOSE_STEP).floor() as usize;
        let sheets = (0..sheets.ceil() as usize)
            .filter_map(|j| {
                let own = |i: usize| family[i] > 0.0 && sheet_of(i) == j;
                let (a, b) = ((0..full).find(|&i| own(i))?, (0..full).rfind(|&i| own(i))?);
                let (start, end) = (a.saturating_sub(reach), (b + reach + 1).min(full));
                let structure: Vec<f64> = (start..end).map(|i| if own(i) { family[i] } else { 0.0 }).collect();
                let falls = vec![REPOSE_GRADE * REPOSE_STEP / RANGE_RISE; structure.len()];
                let crest = structure.iter().copied().fold(0.0, f64::max);
                let mut profiles = Vec::with_capacity(PLUNGE_STEPS * structure.len());
                for p in 0..PLUNGE_STEPS {
                    let mut h: Vec<f64> = structure.iter().map(|s| plunge_step(p) * s).collect();
                    sweep(&mut h, &falls);
                    round(&mut h, crest_rounding(plunge_step(p) * crest * RANGE_RISE));
                    profiles.extend(h);
                }
                Some(Sheet { start, structure, crest, profiles })
            })
            .collect();
        Faces { sheets, plunges }
    }

    /// The share of its height sheet `sheet` stands at, `along` its edge's
    /// strike, as [`plunge_at`] reads the sheet's field.
    fn plunge(&self, sheet: usize, along: f64) -> f64 {
        plunge_at(self.plunges[sheet], along)
    }

    /// Sheet `j`'s profile at plunge step `p` at point `i` of the grid;
    /// nothing off the sheet's span.
    fn point(&self, j: usize, p: usize, i: usize) -> f64 {
        let sheet = &self.sheets[j];
        let n = sheet.structure.len();
        match i.checked_sub(sheet.start) {
            Some(i) if i < n => sheet.profiles[p * n + i],
            _ => 0.0,
        }
    }

    /// The sheets whose spans hold a distance `d` from the front.
    fn spanning(&self, d: f64) -> impl Iterator<Item = usize> + '_ {
        let x = (d - SCREE_START) / REPOSE_STEP;
        let i = if x < 0.0 { None } else { Some(x as usize) };
        self.sheets.iter().enumerate().filter_map(move |(j, sheet)| {
            let i = i?;
            (i + 1 >= sheet.start && i < sheet.start + sheet.structure.len()).then_some(j)
        })
    }

    /// Sheet `j`'s face at a distance `d` from the front at the share
    /// `plunge` of its height: the two profiles bracketing it, blended.
    fn face(&self, j: usize, d: f64, plunge: f64) -> f64 {
        let m = ((plunge - PLUNGE_FLOOR) / PLUNGE_STEP).clamp(0.0, (PLUNGE_STEPS - 1) as f64);
        let p = (m as usize).min(PLUNGE_STEPS - 2);
        let read = |p: usize| between(d, |i| self.point(j, p, i));
        read(p) + (read(p + 1) - read(p)) * (m - p as f64)
    }

    /// The wedge's share at a distance `d` from the front, `along` its
    /// strike: the highest of its sheets, each at its own plunge.
    fn at(&self, d: f64, along: f64) -> f64 {
        self.at_plunged(d, |j| self.plunge(j, along))
    }

    /// [`Faces::at`] with sheet `j` at the share `plunge(j)` of its height.
    fn at_plunged(&self, d: f64, plunge: impl Fn(usize) -> f64) -> f64 {
        self.spanning(d).map(|j| self.face(j, d, plunge(j))).fold(0.0, f64::max)
    }

    /// The height of the crest of the sheet standing highest at a distance
    /// `d` from the front, `along` its strike, as a share of a range's rise
    /// at its plunge: the relief of the hillslope the position lies on.
    /// Nothing where no sheet stands.
    fn crest_at(&self, d: f64, along: f64) -> f64 {
        let mut best = (0.0, 0.0);
        for j in self.spanning(d) {
            let plunge = self.plunge(j, along);
            let face = self.face(j, d, plunge);
            if face > best.0 { best = (face, plunge * self.sheets[j].crest) }
        }
        best.1
    }

    /// The wedge's share at a distance `d` from the front, `along` its
    /// strike, as the structure stands: the sheet there at its plunge.
    fn structure_at(&self, d: f64, along: f64) -> f64 {
        self.structure_plunged(d, |j| self.plunge(j, along))
    }

    /// [`Faces::structure_at`] with sheet `j` at the share `plunge(j)`.
    fn structure_plunged(&self, d: f64, plunge: impl Fn(usize) -> f64) -> f64 {
        let mut best = 0.0f64;
        for j in self.spanning(d) {
            let sheet = &self.sheets[j];
            let at = |i: usize| i.checked_sub(sheet.start).and_then(|i| sheet.structure.get(i)).copied().unwrap_or(0.0);
            let s = between(d, at);
            if s > 0.0 { best = best.max(plunge(j) * s) }
        }
        best
    }
}

// ── The plunge ──────────────────────────────────────────────────────────────

/// Wavelength of the field a sheet plunges by along strike, in world
/// units: five sheet spacings, a hundred and seventy kilometres, within the
/// hundred to two hundred kilometres an orogen's axial culminations lie
/// apart. Two culminations of the field with a saddle between them lie
/// about half a wavelength apart at the nearest, so a sheet never rises
/// and falls within two sheet spacings, where a field finer than a sheet
/// broke the ranges into lenses; a test holds it.
pub const PLUNGE_WAVELENGTH: f64 = 5.0 * RANGE_SPACING;

/// The least share of its height a sheet stands at along strike, which
/// the field's extremes, never reached, would set: its deepest saddles
/// stand at about seven tenths of a culmination, above the three fifths
/// the Alps' passes stand at against their summits, since a saddle is the
/// whole sheet's crest and a pass is cut lower by the valleys meeting at
/// it. Still a range between its culminations, never the range broken.
pub const PLUNGE_FLOOR: f64 = 0.5;

/// Spacing of the shares a sheet's profiles are built at, from the floor
/// to one: a tenth of a sheet's height.
const PLUNGE_STEP: f64 = 0.1;

/// How many shares a sheet's profiles are built at.
const PLUNGE_STEPS: usize = ((1.0 - PLUNGE_FLOOR) / PLUNGE_STEP + 0.5) as usize + 1;

const PLUNGE_SEED: u64 = 0x506c_756e_6765_5f5f; // "Plunge__"

/// The share of its height profile step `p` builds a sheet at.
fn plunge_step(p: usize) -> f64 {
    (PLUNGE_FLOOR + p as f64 * PLUNGE_STEP).min(1.0)
}

/// The seed of sheet `sheet`'s plunge field on the edge `plate` overrides
/// on against `neighbour`: every sheet its own field, so neighbouring
/// sheets culminate out of step and a belt is a chain of culminations and
/// saddles, not ridges rising and falling together.
fn plunge_seed(plate: PlateId, neighbour: PlateId, sheet: usize, seed: u64) -> u64 {
    let key = |p: PlateId| ((p.0 as i64) << 32) ^ (p.1 as i64 & 0xffff_ffff);
    hash_channel(key(plate), key(neighbour), seed ^ PLUNGE_SEED, sheet as u64)
}

/// The share of its height a sheet stands at `along` its strike, from the
/// field `seed` seeds: one at a culmination, [`PLUNGE_FLOOR`] at the
/// deepest saddle, by the cube of the field's distance below its top, so
/// the field's middle stands a sheet at nineteen twentieths and most of a
/// sheet's length at nine tenths or more: a culmination is broad and a
/// saddle narrow. Each field is read on its own stretch of the noise, a
/// million cells wide, and off its lattice points, where every field is
/// nought: two seeds of the noise's own hash read alike over whole cells.
fn plunge_at(seed: u64, along: f64) -> f64 {
    let n = simplex_2d(along / PLUNGE_WAVELENGTH + (seed >> 44) as f64, 0.5, seed).clamp(-1.0, 1.0);
    1.0 - (1.0 - PLUNGE_FLOOR) * (0.5 * (1.0 - n)).powi(3)
}

// ── The event ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct ThrustingEvent;

impl ThrustingEvent {
    pub fn new() -> Self { ThrustingEvent }
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

/// The outlines a cell of the graph lattice reads: the entry the thrusting
/// layer published for it. What thrusting itself and every layer above at
/// the same scale ask in `prepare`.
pub fn outlines_for(scope: &CellScope) -> Arc<Outlines> {
    // The entry read is the caller's own cell on the graph lattice. A layer
    // at any other scale would need the outlines of every graph cell under
    // its footprint, which this does not gather.
    debug_assert_eq!(scope.lattice().radius, GRAPH_CELL_SCALE, "outlines_for reads one graph cell: the caller's own");
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

    /// A tile reads the whole outline of the plate it stands in, as far as
    /// an outline lies from a position in its plate.
    fn max_influence(&self) -> u32 { OUTLINE_REACH as u32 }

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

    /// The structure of a wedge of `sheets` on the profiles' grid, whole.
    fn structure_of(sheets: f64) -> Vec<f64> {
        let n = ((WEDGE_REACH + 2.0 * SCREE_REACH) / REPOSE_STEP).ceil() as usize + 2;
        (0..n).map(|i| family_at(SCREE_START + i as f64 * REPOSE_STEP, sheets)).collect()
    }

    /// A profile held under repose everywhere.
    fn under_repose(structure: &[f64]) -> Vec<f64> {
        let mut h = structure.to_vec();
        sweep(&mut h, &vec![REPOSE_GRADE * REPOSE_STEP / RANGE_RISE; structure.len()]);
        h
    }

    /// A range's surface stands no steeper than repose anywhere across the
    /// belt, though its structure does, and every crest keeps its height:
    /// the forelimb becomes scree from the crest down.
    #[test]
    fn a_range_surface_stands_under_repose() {
        assert!((REPOSE_TAN - REPOSE_DEGREES.to_radians().tan()).abs() < 1e-12);
        let weathered = under_repose(&structure_of(WEDGE_SHEETS));
        let structure = |d: f64| RANGE_RISE * family_at(d, WEDGE_SHEETS);
        let surface = |d: f64| RANGE_RISE * between(d, |i| weathered.get(i).copied().unwrap_or(0.0));
        let (mut steepest_structure, mut steepest_surface) = (0.0f64, 0.0f64);
        // A chord across a crest's corner falls under it by at most a step
        // of the gentler flank, steepened as far as the sheets tighten.
        let chord = REPOSE_STEP * RANGE_RISE / (RANGE_GRADED_HALF_WIDTH * (1.0 - BELT_SHORTENING));
        let mut d = -200.0;
        while d < WEDGE_REACH + 600.0 {
            steepest_structure = steepest_structure.max((structure(d + 1.0) - structure(d)).abs());
            steepest_surface = steepest_surface.max((surface(d + 1.0) - surface(d)).abs());
            assert!(surface(d) >= structure(d) - chord, "scree under the structure by more than a chord's error at {d}");
            d += 1.0;
        }
        assert!(steepest_structure > REPOSE_GRADE, "the structure never exceeded repose: {steepest_structure}");
        assert!(steepest_surface <= REPOSE_GRADE + 1e-6, "the surface exceeds repose: {steepest_surface} over {REPOSE_GRADE}");
        // The crests: the sweep only raises, so the highest point of the
        // grid stands full within a step of the corner, and keeps it.
        let grid = structure_of(WEDGE_SHEETS);
        let (top, at) = (0..grid.len())
            .map(|i| (RANGE_RISE * grid[i], i))
            .fold((0.0f64, 0), |best, p| if p.0 > best.0 { p } else { best });
        let dip = RANGE_RISE / RANGE_STEEP_HALF_WIDTH;
        assert!(top <= RANGE_RISE + 1e-9 && top >= RANGE_RISE - dip * REPOSE_STEP, "no crest stands full: {top}");
        assert!(RANGE_RISE * weathered[at] >= top - 1e-9, "a crest lowered by repose");
    }

    /// Rounding lowers a crest's corner to a parabola tangent to its flanks
    /// and leaves the flanks alone, as the whole opening would; it never
    /// raises, and no slope comes out steeper.
    #[test]
    fn rounding_takes_the_corner_off_a_crest_alone() {
        let ridge: Vec<f64> = (0..90).map(|i| 30.0 - (i as f64 - 30.0).abs() * if i < 30 { 0.5 } else { 1.0 }).collect();
        let steepest = |h: &[f64]| h.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        let same = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        let mut r = ridge.clone();
        round(&mut r, 0.2);
        assert!(r.iter().zip(&ridge).all(|(a, b)| a <= b), "rounding raised the ridge");
        assert!(r[30] < ridge[30], "the crest kept its corner");
        assert!(same(&r[..25], &ridge[..25]) && same(&r[36..], &ridge[36..]), "the flanks moved");
        assert!(steepest(&r) <= steepest(&ridge) + 1e-9, "a slope steepened");
        let mut sharper = ridge.clone();
        round(&mut sharper, 0.8);
        assert!(sharper[30] > r[30], "a sharper curvature took more off");
    }

    /// A crest erodes faster the higher it stands, so it is rounded
    /// sharper at a culmination than in a saddle; a range's full height is
    /// within the relief erosion is bounded over.
    #[test]
    fn crests_round_sharper_at_culminations() {
        let at = |share: f64| crest_rounding(share * RANGE_RISE);
        assert!(at(1.0) > at(PLUNGE_FLOOR) && at(PLUNGE_FLOOR) > 0.0);
        assert!(erosion_rate(RANGE_RISE) > erosion_rate(0.99 * RANGE_RISE) && erosion_rate(RANGE_RISE).is_finite());
        let faces = faces_of(1.0);
        let sheet = &faces.sheets[0];
        let n = sheet.structure.len();
        let crest = |p: usize| (0..n).map(|i| sheet.profiles[p * n + i]).fold(0.0, f64::max) / plunge_step(p);
        assert!(crest(PLUNGE_STEPS - 1) > crest(0), "a saddle's crest is no broader than a culmination's");
    }

    /// The faces of a wedge of `sheets`.
    fn faces_of(sheets: f64) -> Faces {
        let plunges = (0..sheets.ceil() as usize).map(|j| plunge_seed((3, 4), (4, 4), j, S)).collect();
        Faces::new(sheets, plunges)
    }

    /// A position reads each sheet's profile swept at repose at each
    /// plunge step, the highest of the sheets there, and between two steps
    /// it lies between them.
    #[test]
    fn faces_are_keyed_on_the_plunge() {
        let faces = faces_of(2.5);
        let whole = |p: usize, i: usize| {
            faces.sheets.iter().map(|sheet| {
                let mut h = under_repose(&sheet.structure.iter().map(|s| plunge_step(p) * s).collect::<Vec<_>>());
                round(&mut h, crest_rounding(plunge_step(p) * sheet.crest * RANGE_RISE));
                i.checked_sub(sheet.start).and_then(|r| h.get(r).copied()).unwrap_or(0.0)
            }).fold(0.0, f64::max)
        };
        let n = faces.sheets.iter().map(|s| s.start + s.structure.len()).max().unwrap();
        for p in 0..PLUNGE_STEPS - 1 {
            for i in (0..n).step_by(7) {
                let d = SCREE_START + i as f64 * REPOSE_STEP;
                let own = faces.at_plunged(d, |_| plunge_step(p));
                assert!((own - whole(p, i)).abs() < 1e-12, "the profile at plunge {p} is not its own");
                let mid = faces.at_plunged(d, |_| plunge_step(p) + 0.5 * PLUNGE_STEP);
                let (a, b) = (whole(p, i), whole(p + 1, i));
                assert!(mid >= a.min(b) - 1e-12 && mid <= a.max(b) + 1e-12, "a blend past its profiles at plunge {p}");
            }
        }
    }

    /// At full height the plunged structure is the wedge's, and at the
    /// floor every sheet stands at the floor of its share.
    #[test]
    fn the_plunge_scales_each_sheet() {
        for sheets in [0.5, 1.7, WEDGE_SHEETS] {
            let faces = faces_of(sheets);
            let structure = structure_of(sheets);
            for (i, &s) in structure.iter().enumerate() {
                let d = SCREE_START + i as f64 * REPOSE_STEP;
                assert!((faces.structure_plunged(d, |_| 1.0) - s).abs() < 1e-12, "full height is not the wedge at {d}");
                assert!((faces.structure_plunged(d, |_| PLUNGE_FLOOR) - PLUNGE_FLOOR * s).abs() < 1e-12, "the floor is not the floor of the share at {d}");
            }
        }
    }

    /// A sheet stands between the floor and its full height everywhere, at
    /// nine tenths or more along most of its length, and two culminations
    /// with a saddle between them, a twentieth of the height deep or more,
    /// lie at least two sheet spacings apart.
    #[test]
    fn a_sheet_plunges_between_the_floor_and_full_height() {
        let mut all = Vec::new();
        for j in 0..32 {
            let seed = plunge_seed((j, 1), (j, 2), j as usize % 4, S);
            let step = 8.0;
            let m: Vec<f64> = (0..25_000).map(|i| plunge_at(seed, i as f64 * step)).collect();
            assert!(m.iter().all(|&m| (PLUNGE_FLOOR..=1.0).contains(&m)), "a plunge past its bounds");
            let (mut rising, mut extreme, mut at, mut last) = (true, m[0], 0, None);
            for (i, &v) in m.iter().enumerate() {
                if rising && v > extreme || !rising && v < extreme {
                    (extreme, at) = (v, i);
                } else if (v - extreme).abs() >= 0.05 {
                    if rising {
                        if let Some(last) = last {
                            let apart = (at - last) as f64 * step;
                            assert!(apart >= 2.0 * RANGE_SPACING, "culminations {apart} apart");
                        }
                        last = Some(at);
                    }
                    (rising, extreme, at) = (!rising, v, i);
                }
            }
            all.extend(m);
        }
        all.sort_by(f64::total_cmp);
        assert!(all[all.len() / 2] >= 0.9, "the median sheet stands at {}", all[all.len() / 2]);
    }

    /// Neighbouring sheets culminate out of step: their fields are not one
    /// field, never stand alike for a wavelength, and over many edges rise
    /// and fall together no more than chance.
    #[test]
    fn neighbouring_sheets_culminate_out_of_step() {
        let (mut together, mut edges) = (0.0, 0);
        for e in 0..32 {
            let (a, b) = (plunge_seed((e, 0), (e, 1), 0, S), plunge_seed((e, 0), (e, 1), 1, S));
            let pairs: Vec<(f64, f64)> = (0..2_000).map(|i| (plunge_at(a, i as f64 * 50.0), plunge_at(b, i as f64 * 50.0))).collect();
            let n = pairs.len() as f64;
            let (ma, mb) = (pairs.iter().map(|p| p.0).sum::<f64>() / n, pairs.iter().map(|p| p.1).sum::<f64>() / n);
            let cov = pairs.iter().map(|p| (p.0 - ma) * (p.1 - mb)).sum::<f64>();
            let (va, vb) = (pairs.iter().map(|p| (p.0 - ma).powi(2)).sum::<f64>(), pairs.iter().map(|p| (p.1 - mb).powi(2)).sum::<f64>());
            assert!(pairs.iter().map(|p| (p.0 - p.1).abs()).sum::<f64>() / n > 0.02, "edge {e}'s first two sheets plunge as one");
            let alike = pairs.split(|p| (p.0 - p.1).abs() >= 0.005).map(|run| run.len()).max().unwrap_or(0);
            assert!((alike as f64) * 50.0 < PLUNGE_WAVELENGTH, "edge {e}'s first two sheets stand alike for {} units", alike * 50);
            together += cov / (va * vb).sqrt();
            edges += 1;
        }
        let together = together / edges as f64;
        assert!(together.abs() < 0.2, "neighbouring sheets rise and fall together, {together}");
    }

    /// Along strike the relief is continuous at a distance from a bent
    /// chain: the position's place along the line moves no faster than the
    /// position, so the plunge moves by no more than its gradient's chord
    /// over a tile.
    #[test]
    fn the_plunge_is_continuous_along_a_bent_front() {
        let outlines = Outlines::in_box(BELT.0, BELT.1, 8_000.0, S);
        let mut walked = 0;
        for plate in outlines.plates() {
            for e in plate.edges.iter().filter(|e| e.faces.is_some() && e.segments.len() > 8) {
                let [x0, y0, x1, y1] = e.line;
                let len = (x1 - x0).hypot(y1 - y0);
                let (ux, uy) = ((x1 - x0) / len, (y1 - y0) / len);
                let inward = if e.distance(x0 - uy * 10.0 + ux * len * 0.5, y0 + ux * 10.0 + uy * len * 0.5).1 > 0.0 { 1.0 } else { -1.0 };
                for offset in [300.0, 1_000.0] {
                    let at = |t: f64| (x0 + ux * t - inward * uy * offset, y0 + uy * t + inward * ux * offset);
                    let faces = e.faces.as_ref().unwrap();
                    let mut t = 0.0;
                    while t < len {
                        let ((ax, ay), (bx, by)) = (at(t), at(t + 1.0));
                        let (a, b) = (e.along(ax, ay), e.along(bx, by));
                        assert!((b - a).abs() <= 1.0 + 1e-9, "along leapt {} over a tile", b - a);
                        for j in 0..faces.sheets.len() {
                            assert!((faces.plunge(j, b) - faces.plunge(j, a)).abs() < 2e-3, "sheet {j}'s plunge stepped over a tile");
                        }
                        t += 1.0;
                    }
                    walked += 1;
                }
            }
        }
        assert!(walked > 0, "no bent front walked");
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

    /// The ranges' relief is continuous across every front and around every
    /// corner of one: the frontal scree comes to rest across the front, and no
    /// step stands anywhere steeper than repose, a range's end and the
    /// plunge's blend together can make, stretched by the warp the position
    /// reads it through. Every position is where a tile stands to read the
    /// point on the chain: the chain's point unwarped.
    #[test]
    fn scree_comes_to_rest_across_the_front() {
        let outlines = Outlines::in_box(BELT.0, BELT.1, 30_000.0, S);
        let steepest = REPOSE_GRADE + RANGE_RISE * 1.5 / RANGE_END;
        // The most two neighbouring profiles of any sheet part by, per share
        // of plunge between them.
        let mut plunged = 0.0f64;
        for f in outlines.plates().flat_map(|p| p.edges.iter().filter_map(|e| e.faces.as_ref())) {
            for (j, sheet) in f.sheets.iter().enumerate() {
                for p in 0..PLUNGE_STEPS - 1 {
                    for i in sheet.start..sheet.start + sheet.structure.len() {
                        plunged = plunged.max((f.point(j, p + 1, i) - f.point(j, p, i)).abs());
                    }
                }
            }
        }
        let plunged = plunged * RANGE_RISE / PLUNGE_STEP;
        // The most any sheet's plunge moves between two positions.
        let moved = |x0: f64, y0: f64, x1: f64, y1: f64| {
            let ((ax, ay), (bx, by)) = (warp(x0, y0, S), warp(x1, y1, S));
            outlines
                .plates()
                .flat_map(|p| p.edges.iter().filter(|e| e.faces.is_some()))
                .flat_map(|e| {
                    let f = e.faces.as_ref().unwrap();
                    (0..f.sheets.len()).map(move |j| (f.plunge(j, e.along(bx, by)) - f.plunge(j, e.along(ax, ay))).abs())
                })
                .fold(0.0, f64::max)
        };
        let bound = |x0: f64, y0: f64, x1: f64, y1: f64| {
            steepest * stretch_at(x0, y0, S).0 * (x1 - x0).hypot(y1 - y0) + plunged * moved(x0, y0, x1, y1)
        };
        let (mut fronts, mut toes) = (0, 0);
        for plate in outlines.plates() {
            for e in plate.edges.iter().filter(|e| e.converge > 0.0) {
                let s = e.segments[e.segments.len() / 2];
                let (mx, my) = s.mid();
                // Only fronts the box holds both sides of.
                if (mx - BELT.0).abs() > 25_000.0 || (my - BELT.1).abs() > 25_000.0 { continue }
                fronts += 1;
                // Across the front along the plate-facing normal, finely:
                // talus that reaches past the front rests there.
                let (mx, my) = unwarp(mx, my, S);
                let at = |t: f64| outlines.relief(mx + s.nx * t, my + s.ny * t);
                let (fx, fy) = warp(mx, my, S);
                if e.faces.as_ref().unwrap().at(-10.0, e.along(fx, fy)) > 0.0 {
                    toes += 1;
                    assert!(at(-4.0) > 0.0, "no talus rests across the front of {:?}", plate.id);
                }
                let step = 2.0;
                let mut t = -400.0;
                while t < 400.0 {
                    let jump = (at(t + step) - at(t)).abs();
                    let bound = bound(mx + s.nx * t, my + s.ny * t, mx + s.nx * (t + step), my + s.ny * (t + step));
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
                        assert!((east - here).abs() <= bound(x, y, x + step, y) + 1e-6, "a step of {} at a corner of {:?}, {i} {j} from it", east - here, plate.id);
                        assert!((north - here).abs() <= bound(x, y, x, y + step) + 1e-6, "a step of {} at a corner of {:?}, {i} {j} from it", north - here, plate.id);
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
