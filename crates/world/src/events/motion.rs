//! MotionEvent — plate motion and the boundary field it resolves.

//! Index-only. Contributes no elevation and no tags: its whole product is
//! [`PlateBoundaryIndex`], which states, for every macro plate pair sharing a
//! Voronoi boundary, which way that boundary is being worked — convergent,
//! divergent or transform — and for the convergent ones, which flank a wedge
//! standing on it would lean onto.

//! A boundary is already a linear object in the plate graph, so a layer that
//! wants a range along one reads it here rather than recovering linearity from
//! elevations around a tile.

use std::collections::HashMap;
use std::sync::Arc;

use crate::noise::simplex_2d;
use crate::plates::PlateCache;
use crate::{MACRO_CELL_SIZE, PlateCenter, hex_to_world, world_to_hex};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::{CellScope, TileOutput, TileView, WorldEvent};

/// Matched to `PLATE_CELL_SCALE` so the two layers share cell boundaries and
/// warm together. Motion has no structure of its own at any other scale — the
/// features it describes are plate-sized because plates are what move.
pub const MOTION_CELL_SCALE: u32 = 1800;

/// Wavelength of the differential octave of the motion field, in world units.
///
/// Plate centroids sit `MACRO_CELL_SIZE` apart, so this is roughly nine plate
/// widths: neighbours sample a tenth of a wavelength apart and therefore share
/// a broad sense of direction, while the gradient across that span is still
/// large enough that convergent and divergent boundaries both occur. This
/// octave is what a boundary's convergence is a difference of.
const STRAIN_WAVELENGTH: f64 = 8000.0;

/// Wavelength of the drift octave of the motion field, in world units.
///
/// Long compared with a plate, and deliberately so. Convergence is a
/// *difference* between two plates, so it wants a wavelength close to the plate
/// spacing or there is nothing to differ about. Vergence is not a difference —
/// it asks which of the two is advancing in absolute terms, and that answer has
/// to hold along a whole belt. Reading it from a field that turns over every
/// few plates gives a belt whose steep side alternates, which is the one
/// failure this layer exists to prevent.
const DRIFT_WAVELENGTH: f64 = 40_000.0;

/// Wavelength of the field deciding whether a continent–ocean margin is active
/// or passive, in world units.
///
/// Deliberately far longer than [`STRAIN_WAVELENGTH`]: a margin must resolve
/// the same way along its whole run, and the only way to get that without
/// chaining segments into runs is for every segment in a run to sample
/// essentially the same value. At this wavelength a margin has to run most of
/// the way across a continent before the classification can flip.
const MARGIN_WAVELENGTH: f64 = 48000.0;

/// How much of a margin's convergence magnitude comes from the margin field
/// rather than from the two plates' own motion. The field sets the sign
/// outright, so this only decides how much a strongly-driven margin outweighs a
/// weakly-driven one.
const MARGIN_CONVERGENCE_DRIVE: f64 = 0.30;

/// How much of a margin's convergence magnitude the plate motion still
/// contributes. Below 1 so a margin is not simply the interior rule with a flag
/// attached, above 0 so two stretches of one margin do not close at identical
/// rates.
const MARGIN_MOTION_SHARE: f64 = 0.5;

/// Shifts the active/passive split off the midpoint of the margin field.
/// Positive values make passive margins more common.
const MARGIN_PASSIVE_BIAS: f64 = 0.10;

/// Additive nudge toward convergence on any boundary touching a plate sited in
/// the coastal transition band.
///
/// A deliberate cheat. Real ranges run parallel to coasts because subduction
/// does, but the regime field decides where the water goes independently of
/// plate motion, so nothing in the generator correlates the two. This supplies
/// the correlation directly. Kept small: it biases the coin, it does not
/// replace it.
const COASTAL_CONVERGENCE_BIAS: f64 = 0.02;

/// Below this the along-strike component reads as absent, so a boundary with no
/// motion at all does not classify as transform-dominant on rounding noise.
const TRANSFORM_EPSILON: f64 = 1e-9;

/// Furthest a plate returned by `PlateCache::plate_neighbors` can sit from its
/// owner. Half of it bounds how far a segment's endpoints lie from the midpoint
/// that decides which cell owns the segment.
const NEIGHBOR_SEARCH_RADIUS: f64 = MACRO_CELL_SIZE * 4.0;

const STRAIN_SEED_X: u64 = 0x4D6F_7469_6F6E_5F58; // "Motion_X"
const STRAIN_SEED_Y: u64 = 0x4D6F_7469_6F6E_5F59; // "Motion_Y"
const DRIFT_SEED_X: u64 = 0x4472_6966_745F_5F58; // "Drift__X"
const DRIFT_SEED_Y: u64 = 0x4472_6966_745F_5F59; // "Drift__Y"
const MARGIN_SEED: u64 = 0x4D61_7267_696E_5F5F; // "Margin__"

// ── Published types ─────────────────────────────────────────────────────────

/// What a continent–ocean boundary is doing.
///
/// The dichotomy is real on both sides: an active margin subducts and carries an
/// arc range, a passive one carries a coastal plain and no range at all. A
/// consumer reads this rather than inferring it from a convergence threshold of
/// its own choosing, which is what keeps every segment along one margin in
/// agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginClass {
    /// Not a continent–ocean boundary — both sides ocean, or neither.
    Interior,
    /// Convergent. The ocean plate subducts, the arc stands on the continental
    /// side, and the wedge verges seaward.
    Active,
    /// Not convergent — `convergence` is negative. Coastal plain, no range.
    Passive,
}

/// How a boundary resolves, once convergence and transform are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryRegime {
    Convergent,
    Divergent,
    /// Along-strike motion exceeds normal motion either way.
    Transform,
}

/// One plate pair's shared boundary, resolved against the motion of both.
#[derive(Debug, Clone)]
pub struct BoundarySegment {
    /// Plate ids, ordered `a < b`, so a boundary reads identically from either
    /// side and appears once however the pair was discovered.
    pub plate_a: u64,
    pub plate_b: u64,
    /// Substrate elevation at each plate's centroid, in the same order.
    /// Positive is continental crust; a consumer asking which side of a margin
    /// is the continent compares these against 0.
    pub elev_a: f64,
    pub elev_b: f64,
    /// Centroid of plate `a`, world units.
    pub ax: f64,
    pub ay: f64,
    /// Centroid of plate `b`, world units.
    pub bx: f64,
    pub by: f64,
    /// Midpoint of the two centroids. The boundary passes through here, and the
    /// cell containing it is the cell that owns this segment.
    pub mx: f64,
    pub my: f64,
    /// Unit direction along the boundary — perpendicular to `a → b`. A line
    /// direction: `strike` and `-strike` describe the same boundary.
    pub strike_x: f64,
    pub strike_y: f64,
    /// Signed normal component of `v_a - v_b`, positive when the plates close.
    pub convergence: f64,
    /// Magnitude of the along-strike component of `v_a - v_b`.
    pub transform: f64,
    /// Unit vector toward the flank a wedge on this boundary leans onto — the
    /// steep, short side. Zero when the boundary is not convergent.
    pub vergence_x: f64,
    pub vergence_y: f64,
    pub margin: MarginClass,
}

impl BoundarySegment {
    /// Distance between the two centroids, world units.
    pub fn separation(&self) -> f64 {
        (self.bx - self.ax).hypot(self.by - self.ay)
    }

    pub fn regime(&self) -> BoundaryRegime {
        if self.transform > self.convergence.abs() {
            BoundaryRegime::Transform
        } else if self.convergence > 0.0 {
            BoundaryRegime::Convergent
        } else {
            BoundaryRegime::Divergent
        }
    }
}

/// Boundary segments, by the cell containing the segment's midpoint.
///
/// Ownership is by midpoint and nothing else, so a segment lands in exactly one
/// cell whichever of its two plates was walked first, and a cell regenerated
/// after eviction rebuilds the same set.
#[derive(Default)]
pub struct PlateBoundaryIndex {
    pub cells: HashMap<CellId, Vec<BoundarySegment>>,
}

impl PlateBoundaryIndex {
    /// Segments owned by any of `cell_ids`.
    pub fn segments_in(&self, cell_ids: &[CellId]) -> Vec<&BoundarySegment> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter())
            .collect()
    }
}

impl CellIndex for PlateBoundaryIndex {
    type Cell = Vec<BoundarySegment>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl EventIndex for PlateBoundaryIndex {
    fn source_scale(&self) -> u32 { MOTION_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|segs| segs.iter().map(|s| world_to_hex(s.mx, s.my)))
            .collect()
    }

    /// Not published. Which segments continue which is a belt question, and a
    /// belt needs thresholds — how far strike may turn, how weak a link may get
    /// — belonging to the layer that assembles belts, not to the layer stating
    /// what each boundary does.
    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

// ── Motion field ────────────────────────────────────────────────────────────

/// The drift the crust at `(wx, wy)` is carried on — the long octave of the
/// motion field, read on its own.
///
/// Published because polarity reads it and nothing else can reconstruct it from
/// a velocity: the two octaves are summed by the time [`plate_velocity`]
/// returns.
pub fn plate_drift(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let x = wx / DRIFT_WAVELENGTH;
    let y = wy / DRIFT_WAVELENGTH;
    (
        simplex_2d(x, y, seed ^ DRIFT_SEED_X),
        simplex_2d(x, y, seed ^ DRIFT_SEED_Y),
    )
}

/// Velocity of the plate whose centroid is at `(wx, wy)`: the drift it rides on
/// plus the differential motion it has against its neighbours.
///
/// Two channels of one low-frequency field rather than a hash per plate id:
/// neighbouring plates must broadly agree about which way the crust is going,
/// and independent hashes give a boundary field that is pure noise — every
/// segment disagreeing with the next about sign and vergence alike.
pub fn plate_velocity(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let x = wx / STRAIN_WAVELENGTH;
    let y = wy / STRAIN_WAVELENGTH;
    let (dx, dy) = plate_drift(wx, wy, seed);
    (
        dx + simplex_2d(x, y, seed ^ STRAIN_SEED_X),
        dy + simplex_2d(x, y, seed ^ STRAIN_SEED_Y),
    )
}

/// One end of a boundary: which plate, where its centroid is, and what the
/// substrate does there.
#[derive(Clone, Copy)]
pub struct PlateSite {
    pub id: u64,
    pub wx: f64,
    pub wy: f64,
    /// Substrate elevation at the centroid. Above 0 is continental crust —
    /// this is the whole of the land test, and the only one.
    pub elevation: f64,
    /// Whether the centroid sits in a steep stretch of the regime field.
    /// A gradient property, not a land one: either side of the datum can be
    /// steep, and both are the transition zone.
    pub in_transition: bool,
}

impl PlateSite {
    /// Whether this end stands on continental crust.
    pub fn is_continental(&self) -> bool { self.elevation >= 0.0 }
}

/// Resolve one plate pair, already ordered by id.
fn resolve_segment(
    a: PlateSite,
    b: PlateSite,
    seed: u64,
) -> Option<BoundarySegment> {
    let PlateSite { id: a_id, wx: ax, wy: ay, .. } = a;
    let PlateSite { id: b_id, wx: bx, wy: by, .. } = b;

    let dx = bx - ax;
    let dy = by - ay;
    let sep = dx.hypot(dy);
    if sep <= f64::EPSILON { return None; }

    // Normal points a → b; strike is its left perpendicular.
    let nx = dx / sep;
    let ny = dy / sep;
    let sx = -ny;
    let sy = nx;

    let (vax, vay) = plate_velocity(ax, ay, seed);
    let (vbx, vby) = plate_velocity(bx, by, seed);

    let rel_x = vax - vbx;
    let rel_y = vay - vby;

    let mut convergence = rel_x * nx + rel_y * ny;
    let along = (rel_x * sx + rel_y * sy).abs();
    let transform = if along < TRANSFORM_EPSILON { 0.0 } else { along };

    if a.in_transition || b.in_transition {
        convergence += COASTAL_CONVERGENCE_BIAS;
    }

    let mx = (ax + bx) * 0.5;
    let my = (ay + by) * 0.5;

    // A continent–ocean margin takes its convergence from the margin field, so
    // the classification below and the number here cannot disagree.
    let ocean_side = match (a.is_continental(), b.is_continental()) {
        (true, false) => Some(1.0),  // ocean lies toward +n
        (false, true) => Some(-1.0), // ocean lies toward -n
        _ => None,
    };
    let margin = match ocean_side {
        None => MarginClass::Interior,
        Some(_) => {
            let drive = simplex_2d(
                mx / MARGIN_WAVELENGTH,
                my / MARGIN_WAVELENGTH,
                seed ^ MARGIN_SEED,
            ) - MARGIN_PASSIVE_BIAS;
            // Whether a margin subducts is a property of the margin, not of one
            // plate pair on it, so the field's sign decides it outright and the
            // plate motion is left to say only how hard. Letting the motion
            // reach the sign is what makes a margin flicker active/passive down
            // its length, and a range that appears and vanishes along a coast is
            // the same failure as a belt with an alternating steep side.
            let strength = convergence.abs() * MARGIN_MOTION_SHARE
                + drive.abs() * MARGIN_CONVERGENCE_DRIVE;
            if drive > 0.0 {
                convergence = strength;
                MarginClass::Active
            } else {
                convergence = -strength;
                MarginClass::Passive
            }
        }
    };

    // Vergence: which flank the wedge leans onto.
    //
    // At a continent–ocean margin it is not a choice — the ocean plate is the
    // dense one, so it subducts and the wedge verges seaward whatever the
    // relative motion says. Everywhere else the plate advancing faster in
    // absolute terms goes under, and the wedge therefore verges against the
    // drift: if the pair is being carried toward `b`, it is `a` that is driving
    // into the collision, so `a` is the flank that ends up steep.
    //
    // Both rules read a side off something attached to the ground rather than
    // off the relative motion, which is what makes them hold along a belt. The
    // relative motion is a difference between two plates and turns over between
    // one boundary and the next; the drift and the position of the ocean do
    // not.
    let (vergence_x, vergence_y) = if convergence <= 0.0 {
        (0.0, 0.0)
    } else if let Some(side) = ocean_side {
        (nx * side, ny * side)
    } else {
        let (dx, dy) = plate_drift(mx, my, seed);
        let drift_normal = dx * nx + dy * ny;
        let sign = if drift_normal >= 0.0 { -1.0 } else { 1.0 };
        (nx * sign, ny * sign)
    };

    Some(BoundarySegment {
        plate_a: a_id, plate_b: b_id,
        elev_a: a.elevation, elev_b: b.elevation,
        ax, ay, bx, by, mx, my,
        strike_x: sx, strike_y: sy,
        convergence, transform,
        vergence_x, vergence_y,
        margin,
    })
}

// ── MotionEvent ─────────────────────────────────────────────────────────────

/// Plate motion, resolved onto the Voronoi boundary graph.
pub struct MotionEvent {
    plate_cache: Arc<PlateCache>,
    seed: u64,
}

impl MotionEvent {
    pub fn new(seed: u64) -> Self {
        Self::with_cache(Arc::new(PlateCache::new(seed)), seed)
    }

    pub fn with_cache(plate_cache: Arc<PlateCache>, seed: u64) -> Self {
        Self { plate_cache, seed }
    }
}

impl WorldEvent for MotionEvent {
    fn name(&self) -> &str { "motion" }
    fn scale(&self) -> u32 { MOTION_CELL_SCALE }

    /// Boundaries come from the plate graph, which is discovered from centroids
    /// rather than by enumerating tiles — the same reason plates take none.

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<PlateBoundaryIndex>();
    }

    fn deform(&self, scope: &CellScope) {
        let cell_id = scope.cell();
        let lattice = scope.lattice();
        let (center_q, center_r) = lattice.cell_center(cell_id);
        let (center_wx, center_wy) = hex_to_world(center_q, center_r);
        let cell_world_radius = self.scale() as f64 * 1.5 + MACRO_CELL_SIZE;

        // A segment whose midpoint lies in this cell has both endpoints within
        // half a neighbour hop of that midpoint, so gathering that far past the
        // cell guarantees both plates of every owned pair are in `plates`. That
        // is what makes the `nbr.id > plate.id` dedup below complete: each pair
        // is reached from both ends, and exactly one ordering is kept.
        let gather_radius = cell_world_radius + NEIGHBOR_SEARCH_RADIUS * 0.5;
        let plates = self.plate_cache.plates_in_radius(center_wx, center_wy, gather_radius);

        let mut segments: Vec<BoundarySegment> = Vec::new();
        for plate in &plates {
            let a = self.site(plate);
            for nbr in self.plate_cache.plate_neighbors(plate.wx, plate.wy) {
                if nbr.id <= plate.id { continue; }

                let mx = (plate.wx + nbr.wx) * 0.5;
                let my = (plate.wy + nbr.wy) * 0.5;
                let (mq, mr) = world_to_hex(mx, my);
                if lattice.cell_id(mq, mr) != cell_id { continue; }

                let b = self.site(&nbr);

                if let Some(seg) = resolve_segment(a, b, self.seed) {
                    segments.push(seg);
                }
            }
        }

        // Ordered by plate pair so the published Vec does not depend on the
        // order `plates_in_radius` happened to return.
        segments.sort_by_key(|s| (s.plate_a, s.plate_b));
        scope.publish::<PlateBoundaryIndex>(segments);
    }

    /// Nothing. The layer states what boundaries do; it does not put anything
    /// on the ground. `PlateBoundaryIndex` is its whole product.
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

impl MotionEvent {
    fn site(&self, plate: &PlateCenter) -> PlateSite {
        PlateSite {
            id: plate.id,
            wx: plate.wx,
            wy: plate.wy,
            elevation: self.plate_cache.plate_elevation(plate),
            in_transition: self.plate_cache.plate_in_transition(plate),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Composite;
    use crate::events::plates::PlateEvent;

    const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

    fn composite() -> Composite {
        let cache = Arc::new(PlateCache::new(SEED));
        let mut c = Composite::new(SEED);
        c.add_event(Box::new(PlateEvent::with_cache(cache.clone())));
        c.add_event(Box::new(MotionEvent::with_cache(cache, SEED)));
        c
    }

    fn segments_around(c: &Composite, coords: &[(i32, i32)]) -> Vec<BoundarySegment> {
        // Materializing the tiles is what deforms the layers under them, which
        // is the only way an index gets filled.
        c.tiles_at(coords);
        let mut segs: Vec<BoundarySegment> = c.with_indexes(|ix| {
            ix.get::<PlateBoundaryIndex>()
                .map(|idx| idx.cells.values().flat_map(|v| v.iter().cloned()).collect())
                .unwrap_or_default()
        });
        // Cells arrive in HashMap order; sort so two runs compare elementwise.
        segs.sort_by_key(|s| (s.plate_a, s.plate_b));
        segs
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

    /// Every boundary appears once. A pair reached from both of its plates and
    /// published twice would double every downstream weight.
    #[test]
    fn each_plate_pair_appears_once() {
        let c = composite();
        let segs = segments_around(&c, &block(600, 3000));
        assert!(!segs.is_empty(), "no boundaries resolved");

        let mut seen = std::collections::HashSet::new();
        for s in &segs {
            assert!(s.plate_a < s.plate_b, "pair not ordered: {} {}", s.plate_a, s.plate_b);
            assert!(
                seen.insert((s.plate_a, s.plate_b)),
                "pair ({}, {}) published twice", s.plate_a, s.plate_b
            );
        }
    }

    /// Both signs must occur, or the layer states nothing.
    #[test]
    fn both_convergent_and_divergent_boundaries_occur() {
        let c = composite();
        let segs = segments_around(&c, &block(600, 3000));
        let conv = segs.iter().filter(|s| s.convergence > 0.0).count();
        let div = segs.iter().filter(|s| s.convergence < 0.0).count();
        assert!(conv > 0 && div > 0, "convergent {conv}, divergent {div} — field is one-sided");
    }

    /// The published flag and the published number describe the same boundary.
    /// A consumer reading one and a consumer reading the other must agree.
    #[test]
    fn margin_class_agrees_with_convergence() {
        let c = composite();
        for s in segments_around(&c, &block(600, 3000)) {
            match s.margin {
                MarginClass::Active => assert!(s.convergence > 0.0,
                    "active margin with convergence {}", s.convergence),
                MarginClass::Passive => assert!(s.convergence <= 0.0,
                    "passive margin with convergence {}", s.convergence),
                MarginClass::Interior => {}
            }
        }
    }

    /// Vergence is a unit vector along the boundary normal, present exactly on
    /// the convergent boundaries. A consumer reads the steep flank straight off
    /// it rather than recovering a side from a sign convention of its own.
    #[test]
    fn vergence_is_set_exactly_where_convergent() {
        let c = composite();
        for s in segments_around(&c, &block(600, 3000)) {
            let mag = s.vergence_x.hypot(s.vergence_y);
            if s.convergence > 0.0 {
                assert!((mag - 1.0).abs() < 1e-9, "vergence not unit: {mag}");
                let nx = (s.bx - s.ax) / s.separation();
                let ny = (s.by - s.ay) / s.separation();
                let along_n = (s.vergence_x * nx + s.vergence_y * ny).abs();
                assert!((along_n - 1.0).abs() < 1e-9, "vergence off the boundary normal");
            } else {
                assert_eq!(mag, 0.0, "vergence set on a non-convergent boundary");
            }
        }
    }

    /// An active margin verges seaward: the ocean plate subducts because it is
    /// the dense one, and a belt whose steep flank alternated between the sea
    /// and the interior is the failure this layer exists to prevent.
    #[test]
    fn active_margins_verge_seaward() {
        let c = composite();
        let mut checked = 0;
        for s in segments_around(&c, &block(600, 3000)) {
            if s.margin != MarginClass::Active { continue; }
            let (ox, oy) = if s.elev_a >= 0.0 {
                (s.bx - s.mx, s.by - s.my)
            } else {
                (s.ax - s.mx, s.ay - s.my)
            };
            let dot = s.vergence_x * ox + s.vergence_y * oy;
            assert!(dot > 0.0, "active margin verges inland (dot {dot})");
            checked += 1;
        }
        assert!(checked > 0, "no active margins in the sampled block");
    }

    /// A cell rebuilt after eviction must publish exactly what it published
    /// before. Nothing here accumulates — velocity and the margin field are
    /// pure functions of position and seed, and ownership is by midpoint — so
    /// this holds without framework support.
    #[test]
    fn boundaries_survive_cell_eviction() {
        let c = composite();
        let coords = block(900, 2000);
        let before = segments_around(&c, &coords);

        let cells: Vec<CellId> = c.with_indexes(|ix| {
            ix.get::<PlateBoundaryIndex>()
                .map(|idx| idx.cells.keys().copied().collect())
                .unwrap_or_default()
        });
        c.with_indexes(|ix| { for cell in &cells { ix.remove_cell(*cell); } });

        let fresh = composite();
        let after = segments_around(&fresh, &coords);

        assert_eq!(before.len(), after.len(), "segment count changed");
        for (a, b) in before.iter().zip(after.iter()) {
            assert_eq!(a.plate_a, b.plate_a);
            assert_eq!(a.convergence, b.convergence);
            assert_eq!(a.margin, b.margin);
            assert_eq!(a.vergence_x, b.vergence_x);
        }
    }
}
