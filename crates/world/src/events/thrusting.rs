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
//! stands at a thrust wedge's dips, the forelimb at 50°, and its surface
//! stands under the limit its rock sets at each point. Rock sheds as it
//! weathers and its talus rests at repose, so the steepest a face stands is
//! set by how weathered its rock is: every rock on an old plate lies at
//! repose, and basement on a young one stands steepest. The rock at a point
//! is the bed the structure exposes there, so a hard bed stands as a cliff,
//! a weak bed lies at its own limit under the talus the cliff above it
//! sheds, and a flank is a staircase. The crest keeps its height, the
//! frontal sheet's talus runs out past the front onto the plate going under
//! as far as its rock lets it, and the backlimb, gentler than any limit, is
//! barely touched. Repose is over the one level a step climbs, so a
//! weathered range is climbed by traverse and crossed at its passes.
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
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use crate::chains::{join_at_nodes, Segment};
use crate::noise::{hash_channel, simplex_2d};
use crate::tectonic::aged;
use crate::lattice::{nearest_node, node_world, NodeKey, PATH_SWING};
use crate::tectonic::{edges_of, plate_cell_for, plates_near, PlateId, PLATE_REACH};
use super::plates::{warp, WARP_SWING};
use crate::{hex_to_world, smoothstep, RISE, SEA_MAX_DEPTH};
use super::index::{CellId, CellIndex, IndexRegistry};
use super::lithology::{cover, cover_on, exposed_in, Column, Rock, BELT_COVER, CONTACT_RAMP};
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
/// The dip is the structure's. The surface stands under its rock's limit,
/// [`face_grade`]; what a player can climb is movement's to decide.
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

/// The angle loose, angular rock debris comes to rest at, in degrees: the
/// slope a range's surface lies at where its rock is fully weathered.
#[cfg(test)]
const REPOSE_DEGREES: f64 = 34.0;

/// tan of the repose angle, 34°. A const cannot call `tan`; a test holds
/// the two equal.
const REPOSE_TAN: f64 = 0.674_508_516_842_426_5;

/// Scree's fall per world unit of run, in z-levels: the slope talus rests
/// at, and the gentlest any face weathers to. Over the one level a step
/// climbs, so a weathered range is climbed by traverse.
pub const REPOSE_GRADE: f64 = REPOSE_TAN / RISE;

/// How far talus from a full range's crest reaches, in world units: its
/// height over the repose grade, the gentlest slope any rock lies at.
pub const SCREE_REACH: f64 = RANGE_RISE / REPOSE_GRADE;

/// Spacing of the points a cross-section is read at to hold it under its
/// rock's limit, in world units, on a grid fixed in the across-strike
/// coordinate so the result is continuous. The structure is taken straight
/// between points, so a crest stands under its structure by a chord's
/// error, under a level.
const REPOSE_STEP: f64 = 8.0;

/// Where a face profile begins, in world units from the front: talus from
/// the frontal crest reaches no farther out past the front.
const SCREE_START: f64 = -SCREE_REACH;

/// Spacing of the depths into a column a plate's rock is read at for its
/// faces, in z-levels: an eighth of the contact ramp, so a contact on a
/// face lies within an eighth of a ramp of where lithology reads it.
const DEPTH_STEP: f64 = CONTACT_RAMP / 8.0;

/// The steepest a face of `rock` stands on a plate of `age`, in z-levels
/// per world unit: the repose grade over the rock's weathering share,
/// `aged(age) × erodibility` over basement's erodibility, at most one.
///
/// A face is buried in its own talus at repose once its plate has aged as
/// far as its rock needs, and basement, the hardest bed in any column,
/// needs the whole age: an old range is rounded in every rock. A softer
/// rock is buried sooner by its erodibility, and a face not yet buried
/// holds a grade the share's inverse steeper, as a slope's threshold
/// scales with its rock's strength. Basement on a new plate holds 53°.
pub fn face_grade(rock: Rock, age: f64) -> f64 {
    REPOSE_GRADE / (aged(age) * rock.erodibility() / Rock::Basement.erodibility()).min(1.0)
}

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
    /// The wedge's cross-section under its rock's limit, none where the
    /// edge carries no wedge.
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
                        Faces::new(sheets, &Column::of(plate.id, seed), plate.age, cover_band(bounds, seed), plunges)
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
        let remaining = cover_on(self, at, self.seed);
        let wedges = Self::wedges_of(at.plate, &at.distances, |e, faces, d| faces.at(d, e.along(at.x, at.y), remaining));
        let toes = self.toes_on(at.plate, at.x, at.y, SCREE_REACH, |front, faces, d| faces.at(-d, front.along(at.x, at.y), remaining));
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
    /// as it was built, before its rock weathers, and no talus. What
    /// lithology strips the column by, since the surface itself stands by
    /// the rock.
    pub fn structure_of(&self, at: &Standing) -> f64 {
        RANGE_RISE * Self::wedges_of(at.plate, &at.distances, |e, faces, d| faces.structure_at(d, e.along(at.x, at.y)))
    }

    /// The strongest wedge of the convergent edges of a plate at a position
    /// standing in it, as a share of a range's rise, each read off its
    /// faces by `share` at the distance to its front: each ending against
    /// every other edge of the plate. A wedge ends against a convergent edge
    /// as much as a quiet one: past the corner that edge's own wedge climbs
    /// from nothing, and the plate across it stands at nothing but talus.
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

/// A cross-section on the grid every [`REPOSE_STEP`] from [`SCREE_START`]
/// held under the limits its rock sets, in place: every value of `h`, the
/// structure, raised to the highest of any point's less the falls between,
/// where `falls[i]` is the most the surface falls over the step between
/// points `i - 1` and `i`. So the ground below a face lies under what the
/// face sheds, as steep as the rock beneath lets it stand, and stands as
/// built where its rock holds a slope steeper than the structure takes. Two
/// sweeps do it exactly: forward, a value is at least the one before it
/// less its step's fall, and back the same. Read straight between points,
/// the surface is continuous, a chord of a slope within its limits is
/// within them, and a crest stands under its structure by a chord's error.
fn sweep(h: &mut [f64], falls: &[f64]) {
    for i in 1..h.len() {
        h[i] = h[i].max(h[i - 1] - falls[i]);
    }
    for i in (1..h.len()).rev() {
        h[i - 1] = h[i - 1].max(h[i] - falls[i]);
    }
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

/// A wedge's cross-section with its surface held under its rock's limit,
/// sheet by sheet, one profile for each cover its ground can carry and
/// each share of its height a sheet's plunge leaves it.
///
/// The sheets' wedges are disjoint across strike, each meeting the next at
/// a trough, so the structure is the highest of them, and the highest of
/// their sweeps is the sweep of the whole: each sheet is swept alone at its
/// own plunge, and a position reads every sheet whose talus reaches it.
/// The plunge scales a sheet's structure before its sweep, since repose is
/// a rule for debris: under a lower crest the talus is shorter, never
/// gentler. A sheet's rock is read from its own structure alone, so where
/// its talus runs past the trough onto its neighbour's flank, the rock
/// beneath is read without the neighbour's stripping: off by at most the
/// plate's age times the neighbour's height there, within a scree reach of
/// a trough.
///
/// The rock at a point is the bed the structure exposes there, and of what
/// sets it, the column, the plate's age and the structure are the edge's,
/// and along the edge only the cover and the plunge vary. So the profiles
/// of one sheet are a family of two scalars: one is built every
/// [`CONTACT_RAMP`] of cover, over the cover the edge's ground carries, at
/// every [`PLUNGE_STEP`] of share from [`PLUNGE_FLOOR`] to one, and a
/// position blends the four bracketing its own, so the surface is
/// continuous along strike where the nearest profile would step.
///
/// The blend in cover is the contact ramp. Between neighbouring profiles
/// every contact moves one ramp of column, and a position between them
/// reads the change spread straight across that ramp, as lithology spreads
/// a scarp. On an aged plate a ramp of column moves a contact a sliver of a
/// step along the cross-section, and the blend is all but exact. A young
/// plate exposes little more than a ramp of column from trough to crest, so
/// one contact re-weathers a whole flank between two profiles, as much as a
/// quarter of a range's rise in a trough, and the blend spreads that over
/// the ground the cover takes to move a ramp. The blend in plunge is
/// coarser: between neighbouring plunges the stripping at a crest changes
/// by the plate's age times a tenth of a range's rise, fifty levels on an
/// aged plate, so a contact high on a flank moves between the two as far
/// as fifty levels of cover would move it.
pub struct Faces {
    sheets: Vec<Sheet>,
    /// The plunge field's seed for each sheet, as [`Faces::plunge`] reads it.
    plunges: Vec<u64>,
    /// The cover the first profile is built for, in z-levels.
    first: f64,
    /// How many profiles per plunge, one every [`CONTACT_RAMP`] of cover
    /// from `first`.
    count: usize,
    /// The plate's column and age, which the profiles weather.
    column: Column,
    age: f64,
}

/// One sheet of a wedge on the profiles' grid, over the points its
/// structure and its talus can reach.
struct Sheet {
    /// The grid point the sheet's span begins at.
    start: usize,
    /// The sheet's structure at full height over its span, nothing off its
    /// own wedge.
    structure: Vec<f64>,
    /// Where each point of the span sits in a profile, or [`BARE`] where
    /// talus never reaches it.
    slots: Vec<u32>,
    /// How many points of the span talus reaches: each profile's length.
    reached: usize,
    /// The runs of points talus reaches, `[start, end)` within the span.
    runs: Vec<(usize, usize)>,
    /// The profiles at each plunge, one after another by cover, each over
    /// the points talus reaches, built when a position first reads them: a
    /// cell's outlines hold the edges of every plate its ring reaches, and
    /// its own ground reads a few sheets of a few at a few plunges.
    profiles: Vec<OnceLock<Vec<f32>>>,
}

/// The slot of a point of a face profile no talus reaches: the most
/// weathered rock, lying at repose under the sheet at full height, leaves
/// the structure bare there, and every rock holds a slope at least as
/// steep and every plunge lowers the crest the talus falls from, so every
/// profile is the structure there.
const BARE: u32 = u32::MAX;

impl Faces {
    /// The faces of a wedge of `sheets` on a plate of `age` whose column is
    /// `column`, for ground carrying the cover in `band`, the sheets
    /// plunging by the fields `plunges` seeds. A sheet's talus is spent
    /// within a scree reach of its wedge, so its span ends there. With no
    /// cover the surface is basement everywhere, and under more than the
    /// column and all the age strips from the crest it is the top bed
    /// everywhere, so the profiles span no cover past either.
    fn new(sheets: f64, column: &Column, age: f64, band: (f64, f64), plunges: Vec<u64>) -> Self {
        let full = ((WEDGE_REACH + 2.0 * SCREE_REACH) / REPOSE_STEP).ceil() as usize + 2;
        let reach = (SCREE_REACH / REPOSE_STEP).ceil() as usize + 1;
        let family: Vec<f64> = (0..full).map(|i| family_at(SCREE_START + i as f64 * REPOSE_STEP, sheets)).collect();
        let sheet_of = |i: usize| sheets_in(SCREE_START + i as f64 * REPOSE_STEP).floor() as usize;
        let mut crest = 0.0f64;
        let sheets: Vec<Sheet> = (0..sheets.ceil() as usize)
            .filter_map(|j| {
                let own = |i: usize| family[i] > 0.0 && sheet_of(i) == j;
                let (a, b) = ((0..full).find(|&i| own(i))?, (0..full).rfind(|&i| own(i))?);
                let (start, end) = (a.saturating_sub(reach), (b + reach + 1).min(full));
                let structure: Vec<f64> = (start..end).map(|i| if own(i) { family[i] } else { 0.0 }).collect();
                crest = structure.iter().copied().fold(crest, f64::max);
                let mut weathered = structure.clone();
                sweep(&mut weathered, &vec![REPOSE_GRADE * REPOSE_STEP / RANGE_RISE; structure.len()]);
                let mut slots = vec![BARE; structure.len()];
                let mut runs: Vec<(usize, usize)> = Vec::new();
                let mut reached = 0;
                for i in 0..structure.len() {
                    if weathered[i] <= structure[i] { continue }
                    slots[i] = reached as u32;
                    reached += 1;
                    match runs.last_mut() {
                        Some(run) if run.1 == i => run.1 = i + 1,
                        _ => runs.push((i, i + 1)),
                    }
                }
                let profiles = (0..PLUNGE_STEPS).map(|_| OnceLock::new()).collect();
                Some(Sheet { start, structure, slots, reached, runs, profiles })
            })
            .collect();
        let first = band.0.max(0.0);
        let top = band.1.min(column.total + age * RANGE_RISE * crest).max(first);
        let count = ((top - first) / CONTACT_RAMP).ceil() as usize + 1;
        Faces { sheets, plunges, first, count, column: column.clone(), age }
    }

    /// The share of its height sheet `sheet` stands at, `along` its edge's
    /// strike, as [`plunge_at`] reads the sheet's field.
    fn plunge(&self, sheet: usize, along: f64) -> f64 {
        plunge_at(self.plunges[sheet], along)
    }

    /// Sheet `j`'s profiles at plunge step `p`, built on the first read. A
    /// run of points talus reaches is held at both ends by points it never
    /// does, so each run is swept alone.
    fn profiles(&self, j: usize, p: usize) -> &[f32] {
        let sheet = &self.sheets[j];
        sheet.profiles[p].get_or_init(|| {
            let (column, age, n) = (&self.column, self.age, sheet.structure.len());
            let share = plunge_step(p);
            let limits = Limits::new(column, age);
            let mut profiles = Vec::with_capacity(self.count * sheet.reached);
            let (mut depths, mut sums, mut falls, mut h) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
            for k in 0..self.count {
                let remaining = self.first + k as f64 * CONTACT_RAMP;
                for &(a, b) in &sheet.runs {
                    let (lo, hi) = (a.saturating_sub(1), (b + 1).min(n));
                    for i in lo..hi {
                        depths[i] = exposed_in(column, remaining, age, RANGE_RISE * share * sheet.structure[i]);
                        sums[i] = limits.integral(depths[i]);
                    }
                    for i in lo + 1..hi {
                        falls[i - lo] = limits.fall(depths[i - 1], depths[i], sums[i - 1], sums[i]);
                    }
                    let h = &mut h[..hi - lo];
                    for (h, &s) in h.iter_mut().zip(&sheet.structure[lo..hi]) {
                        *h = share * s;
                    }
                    sweep(h, &falls[..hi - lo]);
                    profiles.extend(h[a - lo..b - lo].iter().map(|&h| h as f32));
                }
            }
            profiles
        })
    }

    /// Sheet `j`'s profile at cover profile `k` and plunge step `p`, at
    /// point `i` of the grid; nothing off the sheet's span.
    fn point(&self, j: usize, k: usize, p: usize, i: usize) -> f64 {
        let sheet = &self.sheets[j];
        let Some(i) = i.checked_sub(sheet.start) else { return 0.0 };
        match sheet.slots.get(i) {
            None => 0.0,
            Some(&BARE) => plunge_step(p) * sheet.structure[i],
            Some(&slot) => self.profiles(j, p)[k * sheet.reached + slot as usize] as f64,
        }
    }

    /// The wedge's share at a distance `d` from the front, `along` its
    /// strike, on ground where `remaining` cover is left: the highest of
    /// its sheets, each at its own plunge.
    fn at(&self, d: f64, along: f64, remaining: f64) -> f64 {
        self.at_plunged(d, remaining, |j| self.plunge(j, along))
    }

    /// [`Faces::at`] with sheet `j` at the share `plunge(j)` of its height:
    /// each sheet whose span holds `d` read from the four profiles
    /// bracketing its cover and plunge, blended; past either end of the
    /// covers, the end one.
    fn at_plunged(&self, d: f64, remaining: f64, plunge: impl Fn(usize) -> f64) -> f64 {
        let x = (d - SCREE_START) / REPOSE_STEP;
        if x < 0.0 {
            return 0.0;
        }
        let i = x as usize;
        let c = ((remaining - self.first) / CONTACT_RAMP).clamp(0.0, (self.count - 1) as f64);
        let k = (c as usize).min(self.count.saturating_sub(2));
        let k1 = (k + 1).min(self.count - 1);
        let mut best = 0.0f64;
        for (j, sheet) in self.sheets.iter().enumerate() {
            if i + 1 < sheet.start || i >= sheet.start + sheet.structure.len() { continue }
            let m = ((plunge(j) - PLUNGE_FLOOR) / PLUNGE_STEP).clamp(0.0, (PLUNGE_STEPS - 1) as f64);
            let p = (m as usize).min(PLUNGE_STEPS - 2);
            let read = |k: usize, p: usize| between(d, |i| self.point(j, k, p, i));
            let low = read(k, p) + (read(k, p + 1) - read(k, p)) * (m - p as f64);
            let high = read(k1, p) + (read(k1, p + 1) - read(k1, p)) * (m - p as f64);
            best = best.max(low + (high - low) * (c - k as f64));
        }
        best
    }

    /// The wedge's share at a distance `d` from the front, `along` its
    /// strike, as the structure stands: the sheet there at its plunge.
    fn structure_at(&self, d: f64, along: f64) -> f64 {
        self.structure_plunged(d, |j| self.plunge(j, along))
    }

    /// [`Faces::structure_at`] with sheet `j` at the share `plunge(j)`.
    fn structure_plunged(&self, d: f64, plunge: impl Fn(usize) -> f64) -> f64 {
        let x = (d - SCREE_START) / REPOSE_STEP;
        if x < 0.0 {
            return 0.0;
        }
        let i = x as usize;
        let mut best = 0.0f64;
        for (j, sheet) in self.sheets.iter().enumerate() {
            if i + 1 < sheet.start || i >= sheet.start + sheet.structure.len() { continue }
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

// ── The cover ───────────────────────────────────────────────────────────────

/// Spacing of the samples an edge reads the cover over its ground at, in
/// world units: a sheet spacing, a sixteenth of the basin potential's
/// wavelength.
const COVER_SAMPLE: f64 = RANGE_SPACING;

/// The cover the ground in reach of an edge's faces carries, in z-levels:
/// the basin potential read every [`COVER_SAMPLE`] over the box `bounds`
/// of the edge's chain, widened by the faces' reach and the warp a tile
/// reads the chain through, then widened by the most two neighbouring
/// samples part by, more than the cover moves between a sample and a
/// position nearest it, and up to the [`BELT_COVER`] the belt fills to at
/// its front. A position outside reads the end profile, so its faces stay
/// continuous.
fn cover_band(bounds: [f64; 4], seed: u64) -> (f64, f64) {
    let reach = WEDGE_REACH + SCREE_REACH + WARP_SWING;
    let [x0, y0, x1, y1] = [bounds[0] - reach, bounds[1] - reach, bounds[2] + reach, bounds[3] + reach];
    let (nx, ny) = (((x1 - x0) / COVER_SAMPLE).ceil() as usize + 1, ((y1 - y0) / COVER_SAMPLE).ceil() as usize + 1);
    let mut row = vec![0.0; nx];
    let (mut lo, mut hi, mut part) = (f64::MAX, f64::MIN, 0.0f64);
    for j in 0..ny {
        for i in 0..nx {
            let c = cover(x0 + i as f64 * COVER_SAMPLE, y0 + j as f64 * COVER_SAMPLE, seed);
            if i > 0 { part = part.max((c - row[i - 1]).abs()) }
            if j > 0 { part = part.max((c - row[i]).abs()) }
            row[i] = c;
            (lo, hi) = (lo.min(c), hi.max(c));
        }
    }
    (lo - part, (hi + part).max(BELT_COVER))
}

/// The grades a plate's rock holds down its column, as what a step of a
/// face profile may fall through it.
///
/// A step of the grid runs through every bed its structure exposes between
/// its two ends, and falls no further than each bed holds over the share of
/// the run that bed takes: the mean grade over the depths it spans. Read at
/// a point instead, a bed thinner than a step's drop was seen or missed by
/// where the grid fell, and a face moved by a step's whole fall as the
/// cover carried a contact across a point.
struct Limits {
    /// The grade the rock at the floor lithology reads holds, summed down
    /// the column every [`DEPTH_STEP`] from the top: `sums[j]` is the
    /// integral over the first `j` steps, in z-levels per world unit times
    /// z-levels of column.
    sums: Vec<f64>,
    /// The grade at the top of the column, and in basement past its foot.
    top: f64,
    basement: f64,
}

impl Limits {
    fn new(column: &Column, age: f64) -> Self {
        let steps = (column.total / DEPTH_STEP).ceil() as usize + 1;
        // The faces stand by the rock the platform's cuesta floor shows:
        // the flank's cut is lithology's, laid over them.
        let grade = |depth: f64| face_grade(column.floor_at(depth, age, 0.0).0, age);
        let mut sums = Vec::with_capacity(steps + 1);
        sums.push(0.0);
        for j in 0..steps {
            sums.push(sums[j] + DEPTH_STEP * grade((j as f64 + 0.5) * DEPTH_STEP));
        }
        Limits { sums, top: grade(0.0), basement: face_grade(Rock::Basement, age) }
    }

    /// The integral of the grade from the top of the column to `depth`,
    /// straight between the steps and carried on past either end at the
    /// grade there.
    fn integral(&self, depth: f64) -> f64 {
        let end = (self.sums.len() - 1) as f64 * DEPTH_STEP;
        if depth <= 0.0 { return depth * self.top }
        if depth >= end { return self.sums[self.sums.len() - 1] + (depth - end) * self.basement }
        let x = depth / DEPTH_STEP;
        let j = x as usize;
        self.sums[j] + (self.sums[j + 1] - self.sums[j]) * (x - j as f64)
    }

    /// The most the surface falls over a step whose ends expose depths `a`
    /// and `b`, where [`Limits::integral`] reads `sum_a` and `sum_b`, as a
    /// share of a range's rise: the mean grade over the depths between,
    /// over at least a [`DEPTH_STEP`] about their middle, so a level step
    /// reads the rock it stands on and the fall is continuous in both.
    fn fall(&self, a: f64, b: f64, sum_a: f64, sum_b: f64) -> f64 {
        let mean = if (b - a).abs() >= DEPTH_STEP {
            (sum_b - sum_a) / (b - a)
        } else {
            let mid = 0.5 * (a + b);
            (self.integral(mid + 0.5 * DEPTH_STEP) - self.integral(mid - 0.5 * DEPTH_STEP)) / DEPTH_STEP
        };
        mean * REPOSE_STEP / RANGE_RISE
    }
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
    use super::super::lithology::{COVER_MEAN, COVER_SWING};
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

    /// A profile held under one rock's limit everywhere.
    fn under(structure: &[f64], grade: f64) -> Vec<f64> {
        let mut h = structure.to_vec();
        sweep(&mut h, &vec![grade * REPOSE_STEP / RANGE_RISE; structure.len()]);
        h
    }

    /// The steepest step of a profile, in z-levels per world unit.
    fn steepest(profile: &[f64]) -> f64 {
        profile.windows(2).map(|w| (w[1] - w[0]).abs() * RANGE_RISE / REPOSE_STEP).fold(0.0, f64::max)
    }

    /// Where its rock is fully weathered a range's surface stands no
    /// steeper than repose anywhere across the belt, though its structure
    /// does, and every crest keeps its height: the forelimb is talus from
    /// the crest down.
    #[test]
    fn a_weathered_range_stands_under_repose() {
        assert!((REPOSE_TAN - REPOSE_DEGREES.to_radians().tan()).abs() < 1e-12);
        assert!((face_grade(Rock::Shale, 1.0) - REPOSE_GRADE).abs() < 1e-12, "aged shale is not fully weathered");
        let weathered = under(&structure_of(WEDGE_SHEETS), face_grade(Rock::Shale, 1.0));
        let structure = |d: f64| RANGE_RISE * family_at(d, WEDGE_SHEETS);
        let surface = |d: f64| RANGE_RISE * between(d, |i| weathered.get(i).copied().unwrap_or(0.0));
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

    /// The ends of weathering: on a fully aged plate every rock lies at
    /// repose, and basement on a new plate holds the steepest grade any
    /// rock does, steeper than repose. A face that holds a grade past the
    /// steepest its structure takes stands as the structure does.
    #[test]
    fn weathering_runs_from_young_basement_to_repose() {
        let rocks = [Rock::Basement, Rock::Limestone, Rock::Sandstone, Rock::Shale];
        let hardest = face_grade(Rock::Basement, 0.0);
        for rock in rocks {
            assert!((face_grade(rock, 1.0) - REPOSE_GRADE).abs() < 1e-12, "{rock:?} on an aged plate stands past repose");
            for age in [0.0, 0.5, 1.0] {
                assert!(face_grade(rock, age) <= hardest, "{rock:?} at age {age} holds more than new basement");
            }
        }
        assert!(hardest > REPOSE_GRADE, "new basement holds no face");
        for sheets in [0.5, 1.0, 1.7, 2.5, WEDGE_SHEETS] {
            for p in 0..PLUNGE_STEPS {
                let structure: Vec<f64> = structure_of(sheets).iter().map(|s| s * plunge_step(p)).collect();
                assert_eq!(under(&structure, steepest(&structure) * 1.001), structure, "a limit past the structure moved {sheets} sheets");
            }
        }
    }

    /// The more weathered a range's rock, the more of its structure lies
    /// under talus: from the structure to repose by degrees, every crest at
    /// its height, no face steeper than its rock holds; and the harder the
    /// rock and the younger the plate, the steeper it holds.
    #[test]
    fn weathering_buries_a_range_by_degrees() {
        for p in 0..PLUNGE_STEPS {
            let structure: Vec<f64> = structure_of(WEDGE_SHEETS).iter().map(|s| s * plunge_step(p)).collect();
            let crest = structure.iter().copied().fold(0.0, f64::max);
            let buries = |more: &[f64], less: &[f64]| more.iter().zip(less).all(|(a, b)| *a >= *b - 1e-12);
            let rocks = [Rock::Basement, Rock::Limestone, Rock::Sandstone, Rock::Shale];
            let ages = [0.0, 0.25, 0.5, 0.75, 1.0];
            let h: Vec<Vec<Vec<f64>>> = ages.iter().map(|&age| rocks.iter().map(|&rock| under(&structure, face_grade(rock, age))).collect()).collect();
            for (a, &age) in ages.iter().enumerate() {
                for (r, &rock) in rocks.iter().enumerate() {
                    assert!(steepest(&h[a][r]) <= face_grade(rock, age) + 1e-9, "{rock:?} at age {age} stands past its limit");
                    assert!((h[a][r].iter().copied().fold(0.0, f64::max) - crest).abs() < 1e-12, "a crest moved");
                    if r > 0 { assert!(buries(&h[a][r], &h[a][r - 1]), "{rock:?} at age {age} buries less than a harder rock") }
                    if a > 0 { assert!(buries(&h[a][r], &h[a - 1][r]), "{rock:?} at age {age} buries less than on a younger plate") }
                }
            }
        }
    }

    /// On a new plate's column a range is a staircase: no step falls
    /// further than the hardest rock it runs through holds, a step through
    /// basement alone stands steeper than repose, and a step through shale
    /// alone lies at repose.
    #[test]
    fn hard_beds_stand_and_weak_beds_lie_at_their_limit() {
        let (plate, age) = ((3, 4), 0.0);
        let column = Column::of(plate, S);
        let faces = faces_of(WEDGE_SHEETS, &column, age);
        let (mut cliffs, mut slopes) = (0, 0);
        for (j, sheet) in faces.sheets.iter().enumerate() {
            for (k, p) in (0..faces.count).step_by(5).flat_map(|k| [(k, 0), (k, PLUNGE_STEPS - 1)]) {
            let (n, share) = (sheet.structure.len(), plunge_step(p));
            let remaining = faces.first + k as f64 * CONTACT_RAMP;
            let h: Vec<f64> = (0..n).map(|i| faces.point(j, k, p, sheet.start + i)).collect();
            let depth = |i: usize| exposed_in(&column, remaining, age, RANGE_RISE * share * sheet.structure[i]);
            for i in 1..n {
                let (a, b) = (depth(i - 1).min(depth(i)), depth(i - 1).max(depth(i)));
                let rocks: Vec<Rock> = (0..=16).map(|j| column.floor_at(a + (b - a) * j as f64 / 16.0, age, 0.0).0).collect();
                let hardest = rocks.iter().map(|&r| face_grade(r, age)).fold(0.0, f64::max);
                let grade = (h[i] - h[i - 1]).abs() * RANGE_RISE / REPOSE_STEP;
                assert!(grade <= hardest * (1.0 + 1e-4) + 1e-3, "a step at {grade}, past the {hardest} its hardest rock holds");
                if rocks.iter().all(|&r| r == Rock::Shale) {
                    assert!(grade <= REPOSE_GRADE * (1.0 + 1e-4) + 1e-3, "shale stands at {grade}");
                    slopes += 1;
                } else if rocks.iter().all(|&r| r == Rock::Basement) && grade > REPOSE_GRADE * 1.5 {
                    cliffs += 1;
                }
            }
            }
        }
        assert!(cliffs > 0 && slopes > 0, "{cliffs} steps of basement stand as cliffs, {slopes} of shale lie as slopes");
    }

    /// The faces of a wedge of `sheets` on `column` at `age`, over every
    /// cover the basin potential leaves.
    fn faces_of(sheets: f64, column: &Column, age: f64) -> Faces {
        let plunges = (0..sheets.ceil() as usize).map(|j| plunge_seed((3, 4), (4, 4), j, S)).collect();
        Faces::new(sheets, column, age, (COVER_MEAN - COVER_SWING, COVER_MEAN + COVER_SWING), plunges)
    }

    /// A position reads each sheet's profile built for its own cover and
    /// plunge at each profile's, the highest of the sheets there, and
    /// between them it lies between the four bracketing it.
    #[test]
    fn faces_are_keyed_on_the_cover_and_the_plunge() {
        let (plate, age) = ((3, 4), 0.6);
        let column = Column::of(plate, S);
        let faces = faces_of(2.5, &column, age);
        let limits = Limits::new(&column, age);
        let built = |sheet: &Sheet, remaining: f64, share: f64| {
            let depths: Vec<f64> = sheet.structure.iter().map(|&s| exposed_in(&column, remaining, age, RANGE_RISE * share * s)).collect();
            let mut falls = vec![0.0; depths.len()];
            for i in 1..depths.len() {
                falls[i] = limits.fall(depths[i - 1], depths[i], limits.integral(depths[i - 1]), limits.integral(depths[i]));
            }
            let mut h: Vec<f64> = sheet.structure.iter().map(|s| share * s).collect();
            sweep(&mut h, &falls);
            h
        };
        // The highest sheet's whole sweep at grid point `i`.
        let whole = |remaining: f64, p: usize, i: usize| {
            faces.sheets.iter().map(|sheet| {
                i.checked_sub(sheet.start).and_then(|r| built(sheet, remaining, plunge_step(p)).get(r).copied()).unwrap_or(0.0)
            }).fold(0.0, f64::max)
        };
        let n = faces.sheets.iter().map(|s| s.start + s.structure.len()).max().unwrap();
        for k in (0..faces.count - 1).step_by(23) {
            for p in 0..PLUNGE_STEPS - 1 {
                let (c0, c1) = (faces.first + k as f64 * CONTACT_RAMP, faces.first + (k + 1) as f64 * CONTACT_RAMP);
                for i in (0..n).step_by(7) {
                    let d = SCREE_START + i as f64 * REPOSE_STEP;
                    let corners = [whole(c0, p, i), whole(c1, p, i), whole(c0, p + 1, i), whole(c1, p + 1, i)];
                    let own = faces.at_plunged(d, c0, |_| plunge_step(p));
                    assert!((own - corners[0]).abs() < 1e-5, "the profile for cover {c0} at plunge {p} is not its own: {own} for {}", corners[0]);
                    let mid = faces.at_plunged(d, 0.5 * (c0 + c1), |_| plunge_step(p) + 0.5 * PLUNGE_STEP);
                    let (lo, hi) = corners.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &c| (lo.min(c), hi.max(c)));
                    assert!(mid >= lo - 1e-5 && mid <= hi + 1e-5, "a blend past its profiles at cover {c0} and plunge {p}");
                }
            }
        }
    }

    /// At full height the plunged structure is the wedge's, and at the
    /// floor every sheet stands at the floor of its share.
    #[test]
    fn the_plunge_scales_each_sheet() {
        let column = Column::of((3, 4), S);
        for sheets in [0.5, 1.7, WEDGE_SHEETS] {
            let faces = faces_of(sheets, &column, 0.5);
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
    /// corner of one: the frontal talus comes to rest across the front, and
    /// no step stands anywhere steeper than the steepest face, a range's end
    /// and the blend of two faces as the cover moves together can make,
    /// stretched by the warp the position reads it through. Every position
    /// is where a tile stands to read the point on the chain: the chain's
    /// point unwarped.
    #[test]
    fn scree_comes_to_rest_across_the_front() {
        let outlines = Outlines::in_box(BELT.0, BELT.1, 30_000.0, S);
        let steepest = face_grade(Rock::Basement, 0.0) + RANGE_RISE * 1.5 / RANGE_END;
        // The most two neighbouring profiles of any sheet part by, per level
        // of cover and per share of plunge between them.
        let (mut blend, mut plunged) = (0.0f64, 0.0f64);
        for f in outlines.plates().flat_map(|p| p.edges.iter().filter_map(|e| e.faces.as_ref())) {
            for (j, sheet) in f.sheets.iter().enumerate() {
                for k in 0..f.count {
                    for p in 0..PLUNGE_STEPS {
                        for i in sheet.start..sheet.start + sheet.structure.len() {
                            let h = f.point(j, k, p, i);
                            if k + 1 < f.count { blend = blend.max((f.point(j, k + 1, p, i) - h).abs()) }
                            if p + 1 < PLUNGE_STEPS { plunged = plunged.max((f.point(j, k, p + 1, i) - h).abs()) }
                        }
                    }
                }
            }
        }
        let (blend, plunged) = (blend * RANGE_RISE / CONTACT_RAMP, plunged * RANGE_RISE / PLUNGE_STEP);
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
        // The cover a position reads its faces at, the belt's fill with it.
        let filled = |x: f64, y: f64| outlines.at(x, y).map_or(cover(x, y, S), |at| cover_on(&outlines, &at, S));
        let bound = |x0: f64, y0: f64, x1: f64, y1: f64| {
            steepest * stretch_at(x0, y0, S).0 * (x1 - x0).hypot(y1 - y0)
                + blend * (filled(x1, y1) - filled(x0, y0)).abs()
                + plunged * moved(x0, y0, x1, y1)
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
                if e.faces.as_ref().unwrap().at(-10.0, e.along(fx, fy), filled(mx, my)) > 0.0 {
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
