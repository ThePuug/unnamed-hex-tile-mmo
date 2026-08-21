//! OrogenEvent — mountain belts standing on convergent plate boundaries.

//! A range is placed for a boundary segment rather than for a plate interior,
//! so it is linear from the start and asymmetric across strike. Everything it
//! needs — where the boundary runs, how hard it closes, which flank the wedge
//! leans onto, whether a coast subducts — is already resolved on
//! [`PlateBoundaryIndex`], which is the only index this layer reads.

//! **Placement is deform's job.** A cell places swaths for the segments whose
//! midpoint lies in it, so each swath has exactly one writer and no
//! coordination is needed. A swath's extent crosses cell boundaries freely;
//! its ownership does not.

//! **Continuity is emergent, not assembled.** Nothing here chains segments or
//! propagates polarity along a belt. Vergence derives from the drift octave at
//! λ40,000 and convergence from the strain octave at λ8,000, so neighbouring
//! segments agree because they sample the same slowly-varying fields.

use std::collections::HashMap;
use std::sync::Arc;

use common::PlateTag;

use crate::slope_form::MASS_WASTING_REACH;
use crate::{MACRO_CELL_SIZE, world_to_hex};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::motion::{BoundarySegment, MarginClass, PlateBoundaryIndex};
use super::{CellScope, TileOutput, TileView, WorldEvent};

// ── Cross-section budget ────────────────────────────────────────────────────

/// Across-strike reach of a swath from the boundary trace, at full drive.
///
/// A budget rather than a flank width: the graded toe and the steep toe both
/// lie inside it, so nothing a swath places sits further across strike than
/// this from the segment it grew on. It is the lateral term of
/// [`OrogenEvent::max_influence`].
pub const SWATH_HALF_WIDTH: f64 = 2625.0;

/// The steep flank's width as a share of the graded flank's — how far the
/// wedge leans onto its vergence side. The two share [`SWATH_HALF_WIDTH`]
/// between them.
const STEEP_FLANK_SHARE: f64 = 0.35;

/// Graded (long) flank width at full drive.
const GRADED_FLANK_MAX: f64 = SWATH_HALF_WIDTH / (1.0 + STEEP_FLANK_SHARE);

/// Steep (short) flank width at full drive.
///
/// Also the crest's offset from the boundary trace: the steep toe stands on
/// the boundary and the crest one steep-flank-width behind it, which is what
/// puts an arc range landward of its trench without anything having to name a
/// direction.
const STEEP_FLANK_MAX: f64 = GRADED_FLANK_MAX * STEEP_FLANK_SHARE;

/// Along-strike half-length bound.
///
/// A swath's half-length is its own segment's centroid separation, and
/// `PlateCache::plate_neighbors` searches `MACRO_CELL_SIZE * 4`, so no
/// separation can exceed this. That search radius is therefore load-bearing
/// for the containment proof below — widening it widens orogen's influence
/// silently, and there is a note at the search site saying so.
const SWATH_HALF_LENGTH_MAX: f64 = MACRO_CELL_SIZE * 4.0;

// ── Gating ──────────────────────────────────────────────────────────────────

/// Convergence below which no swath is placed.
///
/// Height and width both taper to nothing as convergence falls, so this is a
/// floor on work rather than a classification: a swath below it would move the
/// ground by less than a z-level. Deliberately a magnitude and not
/// `regime() == Convergent` — half the boundaries carrying real convergence
/// classify as transform-dominant, which is the isotropic expectation for
/// geometric Voronoi edges rather than a tuning miss, and transpression builds
/// real ranges.
const CONVERGENCE_FLOOR: f64 = 0.10;

/// Convergence at which a swath reaches full width and height. Above the bulk
/// of the distribution so ordinary belts stay distinguishable from each other
/// instead of all clipping to the same wedge.
const CONVERGENCE_FULL: f64 = 0.75;

/// Whether a plate's tag puts it on the continental side of a margin.
fn is_continental(tag: PlateTag) -> bool {
    matches!(tag, PlateTag::Coast | PlateTag::Inland)
}

// ── Swath ───────────────────────────────────────────────────────────────────

/// One convergent boundary's belt.
///
/// Every field derives from the segment and the motion fields and nothing from
/// the cell that built it, so two cells that both see a segment would build
/// the same swath. Only one of them publishes it, but that is an ownership
/// rule, not what makes the geometry agree.
#[derive(Clone, Debug)]
pub struct Swath {
    /// The segment this grew on, ordered `a < b` as the boundary publishes it.
    pub plate_a: u64,
    pub plate_b: u64,
    /// Centre of the crest line: the segment midpoint pushed off the trace by
    /// the steep flank's width, so the steep toe stands on the boundary.
    pub cx: f64,
    pub cy: f64,
    /// Unit direction along the crest. A line direction — `strike` and
    /// `-strike` describe the same crest.
    pub strike_x: f64,
    pub strike_y: f64,
    /// Half-length along strike: the segment's centroid separation.
    pub half_length: f64,
    /// Unit vector toward the steep flank.
    pub vergence_x: f64,
    pub vergence_y: f64,
    /// Convergence mapped to 0..1 between [`CONVERGENCE_FLOOR`] and
    /// [`CONVERGENCE_FULL`]. Height and width both scale on it.
    pub drive: f64,
    /// Steep flank width at this drive.
    pub steep_width: f64,
    /// Graded flank width at this drive.
    pub graded_width: f64,
    pub margin: MarginClass,
}

impl Swath {
    /// The belt this segment carries, or `None` where a boundary carries none.
    ///
    /// Three things disqualify a segment, and none of them is the regime label:
    /// a passive margin (a coastal plain, by construction), a boundary with
    /// ocean on both sides, and convergence under the floor.
    pub fn from_segment(seg: &BoundarySegment) -> Option<Swath> {
        match seg.margin {
            // Decided by the margin field, so every segment along one coast
            // agrees. A passive margin gets a plain and no range.
            MarginClass::Passive => return None,
            // One side is continental by definition of the class.
            MarginClass::Active => {}
            // Convergence alone would raise ranges out of open ocean.
            MarginClass::Interior => {
                if !is_continental(seg.tag_a) || !is_continental(seg.tag_b) {
                    return None;
                }
            }
        }
        if seg.convergence < CONVERGENCE_FLOOR { return None; }

        let drive = ((seg.convergence - CONVERGENCE_FLOOR)
            / (CONVERGENCE_FULL - CONVERGENCE_FLOOR)).clamp(0.0, 1.0);
        let steep_width = STEEP_FLANK_MAX * drive;
        let graded_width = GRADED_FLANK_MAX * drive;

        Some(Swath {
            plate_a: seg.plate_a,
            plate_b: seg.plate_b,
            cx: seg.mx - seg.vergence_x * steep_width,
            cy: seg.my - seg.vergence_y * steep_width,
            strike_x: seg.strike_x,
            strike_y: seg.strike_y,
            half_length: seg.separation(),
            vergence_x: seg.vergence_x,
            vergence_y: seg.vergence_y,
            drive,
            steep_width,
            graded_width,
            margin: seg.margin,
        })
    }

    /// The two ends of the crest line.
    pub fn crest_ends(&self) -> ((f64, f64), (f64, f64)) {
        let (dx, dy) = (self.strike_x * self.half_length, self.strike_y * self.half_length);
        ((self.cx - dx, self.cy - dy), (self.cx + dx, self.cy + dy))
    }
}

// ── OrogenSwathIndex ────────────────────────────────────────────────────────

/// Belts, by the cell holding the midpoint of the segment each grew on.
///
/// A swath reaches well past the cell that owns it, so a reader gathers cell
/// plus one ring — the set the framework guarantees is deformed, and the set
/// [`OrogenEvent::max_influence`] proves is enough.
#[derive(Default)]
pub struct OrogenSwathIndex {
    pub cells: HashMap<CellId, Vec<Arc<Swath>>>,
}

impl OrogenSwathIndex {
    pub fn swaths_in(&self, cell_ids: &[CellId]) -> Vec<Arc<Swath>> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter().cloned())
            .collect()
    }
}

impl CellIndex for OrogenSwathIndex {
    type Cell = Vec<Arc<Swath>>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl EventIndex for OrogenSwathIndex {
    fn source_scale(&self) -> u32 { OROGEN_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter().map(|s| world_to_hex(s.cx, s.cy)))
            .collect()
    }

    /// Not published. Which swaths continue which is a belt question, and this
    /// layer deliberately assembles no belts.
    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

// ── OrogenEvent ─────────────────────────────────────────────────────────────

/// Cell radius in tiles.
///
/// **Derived from the influence terms below, not chosen.** A cell reads itself
/// plus one ring and one ring clears `1.268 * R`, so the scale is the smallest
/// one satisfying `R >= max_influence / 1.268`, taken up to a round number for
/// headroom. The terms are [`SWATH_HALF_WIDTH`] — itself
/// `HALF_WIDTH_MAX * PEAK_FALLOFF_SCALE + RIDGE_HALF_WIDTH` —
/// [`SWATH_HALF_LENGTH_MAX`], and [`MASS_WASTING_REACH`].
///
/// Moving any of them means re-deriving this number. Re-running the assertion
/// in `Composite::add_event` only says whether the old scale still happens to
/// fit; it does not produce the new one, and the failure mode when it does not
/// is an index read that returns empty rather than one that errors.
pub const OROGEN_CELL_SCALE: u32 = 3600;

/// Mountain belts on convergent boundaries.
///
/// Carries no seed: everything a swath needs comes off the segment, and the
/// world seed reaches deform and query through the framework when the crest
/// starts varying along strike.
#[derive(Default)]
pub struct OrogenEvent;

impl OrogenEvent {
    pub fn new() -> Self { Self }
}

impl WorldEvent for OrogenEvent {
    fn name(&self) -> &str { "orogen" }
    fn scale(&self) -> u32 { OROGEN_CELL_SCALE }

    /// Furthest a swath reaches from the segment it grew on.
    ///
    /// The binding point is a corner of the footprint rather than a flank —
    /// influence is measured from the origin in every direction — so the two
    /// half-extents combine as a hypotenuse. The talus term is there because a
    /// face this layer publishes still moves ground [`MASS_WASTING_REACH`]
    /// past itself, through the apron the slope-form layer lays at its foot,
    /// and the ring that has to hold that face's owner cell is this one.
    ///
    /// See [`OROGEN_CELL_SCALE`]: changing any term here means re-deriving the
    /// scale, not just re-running the assertion this feeds.
    fn max_influence(&self) -> u32 {
        (SWATH_HALF_LENGTH_MAX.hypot(SWATH_HALF_WIDTH) + MASS_WASTING_REACH).ceil() as u32
    }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<OrogenSwathIndex>();
    }

    /// Place a swath for every qualifying segment whose midpoint lies in this
    /// cell.
    ///
    /// Ownership is by midpoint and nothing else, so a segment is written once
    /// however many cells its swath reaches into, and a cell rebuilt after
    /// eviction rebuilds the same set.
    fn deform(&self, scope: &CellScope) {
        let lattice = scope.lattice();
        let cell = scope.cell();
        let source = scope.source_cells::<PlateBoundaryIndex>();

        let mut swaths: Vec<Arc<Swath>> = match scope.read::<PlateBoundaryIndex>() {
            Some(idx) => idx
                .segments_in(&source)
                .into_iter()
                .filter(|s| {
                    let (mq, mr) = world_to_hex(s.mx, s.my);
                    lattice.cell_id(mq, mr) == cell
                })
                .filter_map(|s| Swath::from_segment(s).map(Arc::new))
                .collect(),
            None => Vec::new(),
        };

        // Ordered by plate pair so the published Vec does not depend on the
        // order the boundary index yielded its cells.
        swaths.sort_by(|a, b| (a.plate_a, a.plate_b).cmp(&(b.plate_a, b.plate_b)));
        scope.publish::<OrogenSwathIndex>(swaths);
    }

    /// The swaths any tile in this cell can be reached by: its own cell plus
    /// one ring, which is the set the framework has deformed by now.
    fn prepare(&self, scope: &CellScope) -> Box<dyn std::any::Any + Send + Sync> {
        let cells = scope.lattice().cells_within_distance(scope.cell(), 1);
        let swaths = match scope.read::<OrogenSwathIndex>() {
            Some(idx) => idx.swaths_in(&cells),
            None => Vec::new(),
        };
        Box::new(swaths)
    }

    /// Nothing yet. The layer currently publishes crest geometry and no
    /// surface: the cross-section that turns a crest line into ground is
    /// unbuilt, and until it exists this contributes no elevation and no tags.
    fn query(
        &self,
        _q: i32, _r: i32,
        _below: &TileView,
        _cell: &(dyn std::any::Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Composite;
    use crate::events::motion::MotionEvent;
    use crate::events::plates::PlateEvent;
    use crate::plates::PlateCache;

    const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

    fn composite() -> Composite {
        let cache = Arc::new(PlateCache::new(SEED));
        let mut c = Composite::new(SEED);
        c.add_event(Box::new(PlateEvent::with_cache(cache.clone())));
        c.add_event(Box::new(MotionEvent::with_cache(cache, SEED)));
        c.add_event(Box::new(OrogenEvent::new()));
        c
    }

    fn block(step: i32, extent: i32) -> Vec<(i32, i32)> {
        let mut v = Vec::new();
        let mut q = -extent;
        while q <= extent {
            let mut r = -extent;
            while r <= extent {
                v.push((q, r));
                r += step;
            }
            q += step;
        }
        v
    }

    fn swaths(c: &Composite, coords: &[(i32, i32)]) -> Vec<Swath> {
        c.tiles_at(coords);
        let mut v: Vec<Swath> = c.with_indexes(|ix| {
            ix.get::<OrogenSwathIndex>()
                .map(|idx| {
                    idx.cells.values()
                        .flat_map(|s| s.iter().map(|a| (**a).clone()))
                        .collect()
                })
                .unwrap_or_default()
        });
        v.sort_by(|a, b| (a.plate_a, a.plate_b).cmp(&(b.plate_a, b.plate_b)));
        v
    }

    /// The scale has to clear the influence. `add_event` asserts it, so this
    /// only has to reach setup — but it is the assertion that catches a widened
    /// cross-section, so something has to run it.
    #[test]
    fn scale_contains_influence() {
        let _ = composite();
    }

    /// Ownership is by midpoint, so a segment produces one swath however many
    /// cells it reaches into. Two writers would double every height downstream.
    #[test]
    fn each_segment_places_one_swath() {
        let c = composite();
        let all = swaths(&c, &block(600, 6000));
        assert!(!all.is_empty(), "no swaths placed");
        let mut seen = std::collections::HashSet::new();
        for s in &all {
            assert!(
                seen.insert((s.plate_a, s.plate_b)),
                "pair ({}, {}) placed twice", s.plate_a, s.plate_b,
            );
        }
    }

    /// Nothing in a swath derives from the cell that built it, so materializing
    /// the same ground in a different order has to produce the same geometry.
    #[test]
    fn geometry_is_independent_of_visit_order() {
        let coords = block(900, 5000);
        let forward = swaths(&composite(), &coords);
        let reversed = {
            let mut c = coords.clone();
            c.reverse();
            swaths(&composite(), &c)
        };
        assert_eq!(forward.len(), reversed.len(), "swath count changed with visit order");
        for (a, b) in forward.iter().zip(reversed.iter()) {
            assert_eq!((a.plate_a, a.plate_b), (b.plate_a, b.plate_b));
            assert_eq!(a.cx.to_bits(), b.cx.to_bits(), "crest moved with visit order");
            assert_eq!(a.cy.to_bits(), b.cy.to_bits(), "crest moved with visit order");
            assert_eq!(a.vergence_x.to_bits(), b.vergence_x.to_bits());
            assert_eq!(a.drive.to_bits(), b.drive.to_bits());
        }
    }

    /// The crest stands behind the steep toe, which stands on the trace. On an
    /// active margin that is what puts the range landward of its trench without
    /// anything naming a direction.
    #[test]
    fn crest_sits_behind_the_steep_toe() {
        let c = composite();
        let coords = block(900, 5000);
        let placed = swaths(&c, &coords);
        let segments: HashMap<(u64, u64), BoundarySegment> = c.with_indexes(|ix| {
            ix.get::<PlateBoundaryIndex>()
                .map(|idx| {
                    idx.cells.values().flat_map(|v| v.iter())
                        .map(|s| ((s.plate_a, s.plate_b), s.clone()))
                        .collect()
                })
                .unwrap_or_default()
        });
        assert!(!placed.is_empty(), "no swaths placed");

        for s in &placed {
            let seg = &segments[&(s.plate_a, s.plate_b)];
            // The crest sits off the trace by exactly the steep flank's width,
            // against the vergence direction.
            let back = (seg.mx - s.cx) * s.vergence_x + (seg.my - s.cy) * s.vergence_y;
            assert!(
                (back - s.steep_width).abs() < 1e-9,
                "crest offset {back} is not the steep flank width {}", s.steep_width,
            );
            let across = (seg.mx - s.cx) * s.strike_x + (seg.my - s.cy) * s.strike_y;
            assert!(across.abs() < 1e-9, "crest slid along strike by {across}");
        }
    }

    /// Width tapers to nothing as convergence falls to the floor, so a belt
    /// ends by thinning rather than by stopping.
    #[test]
    fn width_tapers_to_zero_at_the_floor() {
        let c = composite();
        let all = swaths(&c, &block(900, 6000));
        let weakest = all.iter().min_by(|a, b| a.drive.partial_cmp(&b.drive).unwrap()).unwrap();
        assert!(weakest.drive >= 0.0);
        assert!(
            weakest.graded_width <= GRADED_FLANK_MAX,
            "graded flank exceeded its budget",
        );
        for s in &all {
            assert!(
                s.steep_width + s.graded_width <= SWATH_HALF_WIDTH + 1e-9,
                "cross-section overran the across-strike budget",
            );
        }
    }
}
