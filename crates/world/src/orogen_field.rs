//! Orogen as a field — PROTOTYPE, not wired into the event stack.
//!
//! A belt is not a feature placed on a carrier. Four carriers were tried and
//! all failed the same way: every one inherited its spacing from a field whose
//! spacing was smaller than a mountain range. Voronoi faces 1,080 WU, strain
//! orientation coherence 1,877 WU, compression cells ~2,000 WU, ridge spacing
//! 2,144 WU — against a belt wanting a 2,625 WU half-width and tens of
//! thousands of world units of length.
//!
//! What fixed it was replacing the source rather than patching the outputs:
//! the tectonic velocity field runs at [`OROGEN_LAMBDA`] with amplitude scaled
//! to hold strain rate, which puts ridge spacing at 12,985 WU — five belt
//! half-widths — and 80% of ridge length into lines longer than a continent.
//!
//! # What a belt is here
//!
//! A belt is the neighbourhood of a *ridge line of the shortening field*. Every
//! quantity is answered per position in constant time, with no index, no
//! ownership, no containment proof and no belt assembly:
//!
//! - [`shortening`] — the compressive principal strain, straight from the
//!   velocity gradient.
//! - [`project_to_crest`] — Newton iteration along the least-curvature axis
//!   walks a position onto the crest it belongs to. 98% convergence from
//!   anywhere inside a belt, 3 iterations at the median.
//! - The belt axis is the crest's tangent. Not derived from an orientation
//!   octave (44.5° off, measured) nor from the strain tensor (24.2° off): the
//!   belt is the expression of the shortening, so its axis is the shortening's
//!   own geometry. Median 0.70°, 99.9% within 32°.
//!
//! # Three claims, and nothing else
//!
//! Convergent shortening thickens crust, localised on ridge lines of the
//! shortening field. Thickened crust floats higher — the coupling between
//! [`OROGEN_MAX_RISE`], the half-width and the flank angle, plus the taper and
//! [`OCEANIC_LIFT`]. And the wedge is asymmetric because one plate underthrusts
//! the other, so the short steep flank faces vergence.
//!
//! What that produces is a smooth asymmetric swath: a mountain range's
//! *envelope*, which is what the process makes before erosion touches it. Crest
//! height varies along a belt because [`shortening`] varies along its own ridge
//! line — a tectonic parameter varying, not a texture applied.
//!
//! # What this layer does not do
//!
//! No dissection, no valleys, no carving, no peaks, and **no fold texture**.
//!
//! A fold generator lived here and was removed. It failed on principle rather
//! than on a measurement. Surface fold topography is a *shallow* structure —
//! sedimentary cover crumpling, visible as ridges only where erosion has
//! stripped weak beds differentially — so it is lithology plus dissection, two
//! layers that do not exist yet. Drawing the cover's response inside the layer
//! that describes the crustal root is a layering error, and it is why those
//! constants stopped corresponding to anything physical.
//!
//! It could not have worked in this form either. Real fold ridges are discrete
//! objects: thrust sheets with individual traces and displacements, each with
//! its own length and termination. A continuous phase field has no analogue —
//! every ridge is the same ridge shifted — which is why each successive fix
//! produced "still lineated, differently". **Ridges belong to dissection and
//! lithology, not to crustal thickening.** A smooth swath is the correct
//! interim state; roughening it to look finished would be building the wrong
//! thing.
//!
//! # A trap for any later along-strike coordinate
//!
//! Nothing here needs one now, and the finding cost enough to be worth
//! recording. Projecting a position onto the belt axis — `position · axis` —
//! amplifies without bound: the coordinate is of order 10^5 and the axis is a
//! measured direction that wobbles, so a thousandth of a radian moves the
//! result by hundreds of world units. Measured `|Δalong| / |Δposition|`: p50
//! 1.7×, p90 9.2×, **max 4,632×**. Any texture keyed on it varies by that
//! factor faster than the ground does, and an octave built to be isotropic
//! comes out squashed by the same factor along strike.
//!
//! A layer needing an along-strike coordinate should take it from the crest
//! *point*, which slides along the belt as the query moves along strike and
//! stays put as it moves across — an along-strike coordinate that is bounded
//! because it is a position rather than a projection.
//!
//! # Known artifact, not fixed
//!
//! [`crust_share`] has a slope break at sea level. Its value is continuous
//! through the datum — both branches meet at 1.0 — but its derivative jumps
//! from 0.00477 per z to zero, so relief carries a kink along every coastline a
//! belt crosses. Measured, unfixed, and recorded here so it is not rediscovered
//! as something new.

use crate::events::motion::{STRAIN_SEED_X, STRAIN_SEED_Y, STRAIN_WAVELENGTH, plate_drift};
use crate::noise::simplex_2d;
use crate::{SEA_MAX_DEPTH, substrate_elevation_at};


// ── 1. The tectonic field ───────────────────────────────────────────────────

/// Wavelength of the tectonic strain octave this layer reads, in world units.
///
/// Not [`STRAIN_WAVELENGTH`], which is what the boundary layer reads. Measured
/// across a sweep of 8,000 / 50,000 / 150,000: spacing scales linearly with
/// wavelength until roughly 50,000 and then stops, because past that the drift
/// octave at 40,000 WU becomes the shortest scale in the velocity field and
/// sets spacing itself. 50,000 sits just under that ceiling — it buys the whole
/// linear range and nothing beyond it is available without touching drift.
///
/// What it buys, against the 8,000 control: ridge spacing 2,144 → 12,985 WU
/// (0.82 → 4.95 belt half-widths, so belts stop merging), share of ridge length
/// in lines longer than a continent 37% → 81%, and curvature 2.277 → 0.879
/// deg/100 WU against a Voronoi boundary's 8.2.
pub const OROGEN_LAMBDA: f64 = 50_000.0;

/// Amplitude of the strain octave at [`OROGEN_LAMBDA`], relative to the
/// boundary layer's.
///
/// Shortening is a strain *rate* — a gradient — so it falls as 1/λ. Lengthening
/// the wavelength alone collapses it, which is what capped drive at 0.307 in an
/// earlier sweep. Scaling amplitude by λ/λ₀ holds the rate: measured magnitude
/// is p50 0.313 / p99 0.992 at this wavelength against p50 0.314 / p99 0.996 at
/// the control, invariant to three decimals over an 18.75× wavelength change.
pub const OROGEN_AMPLITUDE: f64 = OROGEN_LAMBDA / STRAIN_WAVELENGTH;

/// Velocity of the crust at a position, for this layer's wavelength.
/// Drift is untouched — it is an absolute-motion property at 40,000 WU that
/// vergence reads, and it is independent of shortening.
fn velocity(wx: f64, wy: f64, seed: u64) -> (f64, f64) {
    let (dx, dy) = plate_drift(wx, wy, seed);
    let (x, y) = (wx / OROGEN_LAMBDA, wy / OROGEN_LAMBDA);
    (
        dx + OROGEN_AMPLITUDE * simplex_2d(x, y, seed ^ STRAIN_SEED_X),
        dy + OROGEN_AMPLITUDE * simplex_2d(x, y, seed ^ STRAIN_SEED_Y),
    )
}

/// Stencil for the velocity gradient, scaled to the layer's wavelength so the
/// difference is a derivative rather than a sample of the field's own variation.
const STRAIN_STEP: f64 = 50.0 * OROGEN_AMPLITUDE;

/// Shortening rate that saturates crustal thickening.
///
/// EMPIRICAL: the p99 of measured compressive principal strain. The velocity
/// field is a sum of simplex octaves and its gradient has no sound analytic
/// bound — the product-rule form proved untrue for the regime field by 1.30× —
/// so this is measured and labelled rather than derived and wrong.
pub const SHORTENING_FULL: f64 = 6.65e-4;

/// The most-compressive principal strain at a position, zero where the crust is
/// extending. Read pointwise from the velocity gradient: no centroids, no
/// boundaries, nothing whose shape the answer inherits.
pub fn shortening(wx: f64, wy: f64, seed: u64) -> f64 {
    let h = STRAIN_STEP;
    let (vxp, vyp) = velocity(wx + h, wy, seed);
    let (vxm, vym) = velocity(wx - h, wy, seed);
    let (vxq, vyq) = velocity(wx, wy + h, seed);
    let (vxr, vyr) = velocity(wx, wy - h, seed);
    let exx = (vxp - vxm) / (2.0 * h);
    let eyy = (vyq - vyr) / (2.0 * h);
    let exy = 0.5 * ((vxq - vxr) / (2.0 * h) + (vyp - vym) / (2.0 * h));
    let mean = 0.5 * (exx + eyy);
    let radius = (0.25 * (exx - eyy) * (exx - eyy) + exy * exy).sqrt();
    (-(mean - radius)).max(0.0)
}

/// Fraction of full shortening below which ground carries no belt.
///
/// Measured free at this wavelength: gating here leaves the longest ridge run
/// at 93% of its ungated length and *raises* median run length, because what it
/// removes is scrap rather than belt. Gating harder starts chopping — at 55%
/// the longest run halves.
pub const SHORTENING_GATE: f64 = 0.40;

/// Crest strength at which a belt reaches full amplitude, ending the ramp out
/// of the gate. The gate above was measured free and this one was measured to
/// begin chopping runs, so the band between them is where a belt is real but
/// marginal — which is what a taper is for.
const GATE_RAMP_TOP: f64 = 0.55;

// ── 2. Finding the crest ────────────────────────────────────────────────────

/// Half-width of a belt, in world units. Coverage at this width under the gate
/// is 26.6% of land.
pub const BELT_HALF_WIDTH: f64 = 2_625.0;

/// Baseline over which the belt axis is estimated.
///
/// One belt half-width, and measured as the right one: it keeps 98% of the
/// belt's real curvature (median turn 1.98° per 900 WU of run against 2.02° raw,
/// where the field's curvature is 7.91°) while cutting the p99 noise tail by
/// 38%. A finite-difference second derivative with step `h` averages curvature
/// over `2h`, so the stencil below is half this and the smoothing is structural
/// rather than a separate pass.
const AXIS_BASELINE: f64 = BELT_HALF_WIDTH;
const AXIS_STEP: f64 = AXIS_BASELINE * 0.5;

/// Hessian of the shortening field, its least-curvature eigenvector (the
/// across-ridge direction), the curvature along that eigenvector, and the
/// gradient. Closed form at any position, on a crest or off it.
fn across_axis(wx: f64, wy: f64, seed: u64) -> ((f64, f64), f64, (f64, f64)) {
    let h = AXIS_STEP;
    let c = shortening(wx, wy, seed);
    let fxx = (shortening(wx + h, wy, seed) - 2.0 * c + shortening(wx - h, wy, seed)) / (h * h);
    let fyy = (shortening(wx, wy + h, seed) - 2.0 * c + shortening(wx, wy - h, seed)) / (h * h);
    let fxy = (shortening(wx + h, wy + h, seed) - shortening(wx + h, wy - h, seed)
             - shortening(wx - h, wy + h, seed) + shortening(wx - h, wy - h, seed)) / (4.0 * h * h);
    let gx = (shortening(wx + h, wy, seed) - shortening(wx - h, wy, seed)) / (2.0 * h);
    let gy = (shortening(wx, wy + h, seed) - shortening(wx, wy - h, seed)) / (2.0 * h);

    let mean = 0.5 * (fxx + fyy);
    let r = (0.25 * (fxx - fyy) * (fxx - fyy) + fxy * fxy).sqrt();
    let l = mean - r;
    let (mut ex, mut ey) = if fxy.abs() > 1e-30 {
        (l - fyy, fxy)
    } else if fxx <= fyy { (1.0, 0.0) } else { (0.0, 1.0) };
    let m = ex.hypot(ey);
    if m > 1e-30 { ex /= m; ey /= m } else { ex = 1.0; ey = 0.0 }
    let curv = fxx * ex * ex + 2.0 * fxy * ex * ey + fyy * ey * ey;
    ((ex, ey), curv, (gx, gy))
}


/// Where a position's crest is, and which way it runs.
#[derive(Clone, Copy, Debug)]
pub struct Crest {
    pub x: f64,
    pub y: f64,
    /// Belt axis at the crest: the tangent to the shortening ridge line.
    pub axis: (f64, f64),
    /// Signed distance from the crest along the axis normal.
    pub across: f64,
    /// Shortening at the crest, normalised and ramped out of the gate. This is
    /// the belt's strength; the value at the caller's own position is not,
    /// because a belt's amplitude is a property of its crest and not of the
    /// flank being asked about.
    pub strength: f64,
    /// How decisively the drift picks a side here, -1 to 1. The magnitude is
    /// how far from symmetric the wedge leans; the sign is which
    /// way. Carrying the direction here rather than in the sign of `across`
    /// is what lets it pass through zero continuously — at zero there is no
    /// lean to have a direction.
    pub asymmetry: f64,
}

/// Walk a position onto the crest of the ridge it sits on.
///
/// Newton along the least-curvature axis: `p ← p − (∇f·v)/(vᵀHv)·v`. Measured
/// from across a belt's full width, 98.1–99.0% converge in 3 iterations at the
/// median, moving a distance that matches the start offset to within 0.5% — it
/// walks back to the crest it came from rather than wandering — and landing
/// within 102 WU of the independently extracted ridge line.
///
/// `None` where there is no crest within reach: extensional ground, or a
/// position further than a belt's width from any ridge.
/// Where a position's crest is, read from the field in one step.
///
/// The crest offset along the least-curvature axis is `-(∇s·v) / (vᵀHv)` — the
/// distance at which the quadratic the Hessian already fits reaches its own
/// maximum. Both terms come from [`across_axis`], so this is one evaluation of
/// the field and nothing else.
///
/// It replaced an iterated walk. The walk was correct on average and wrong in
/// detail: iterating a smooth map has basin structure far finer than the map,
/// and that structure was being rendered. The field cannot resolve anything
/// below its own [`AXIS_STEP`] stencil of 1,312 WU, yet the walk was drawing
/// 16 WU filaments that carried a full 5 km wedge while the ground beside them
/// carried a flank value — two positions on the same ground, 8 WU apart,
/// disagreeing by 316 z. One step cannot do that: it is a ratio of two smooth
/// fields, so neighbouring positions get neighbouring answers by construction.
pub fn project_to_crest(wx: f64, wy: f64, seed: u64) -> Option<Crest> {
    let (v, curv, g) = across_axis(wx, wy, seed);
    // Non-negative curvature along the axis means no crest lies along it.
    if curv >= 0.0 { return None }

    // Clamped where the widest taper has already reached zero. That costs
    // nothing real — a crest further away carries no relief here — and it makes
    // the two branches agree: as curvature approaches zero from below the offset
    // runs out to the clamp, where the wedge is zero, which is what the
    // non-negative branch above returns.
    let t = ((g.0 * v.0 + g.1 * v.1) / curv).clamp(-GRADED_HALF_WIDTH, GRADED_HALF_WIDTH);
    let (qx, qy) = (wx - t * v.0, wy - t * v.1);

    let axis = (-v.1, v.0);
    let raw = (shortening(qx, qy, seed) / SHORTENING_FULL).clamp(0.0, 1.0);
    if raw < SHORTENING_GATE { return None }

    // Amplitude ramps out of the gate rather than stepping out of it. A hard
    // gate leaves a cliff across strike wherever crest strength crosses the
    // threshold, which is a belt ending in a wall. The ramp spans the gate that
    // was measured free (0.40) to the one that was measured to start chopping
    // runs (0.55) — the interval between them is exactly the band where a belt
    // is real but marginal.
    let u = ((raw - SHORTENING_GATE) / (GATE_RAMP_TOP - SHORTENING_GATE)).clamp(0.0, 1.0);
    let strength = raw * u * u * (3.0 - 2.0 * u);

    // Across-strike coordinate on the axis normal, and how decisively the drift
    // picks a side. The two stay separate: `across` is pure geometry and `bias`
    // carries the vergence, so neither needs a sign switch on the other.
    let (ax, ay) = (-axis.1, axis.0);
    let bias = vergence_bias(qx, qy, axis, seed);
    let across = (wx - qx) * ax + (wy - qy) * ay;

    Some(Crest { x: qx, y: qy, axis, across, strength, asymmetry: bias })
}

/// How decisively the drift picks a side of this belt, in [-1, 1].
///
/// The crust advancing in absolute terms is the one that goes under, so a wedge
/// leans against the drift — but only to the extent the drift actually crosses
/// the belt. Drift running *along* a belt names no side, and the answer is the
/// cosine of the angle between the two, which needs no constant: +/-1 where the
/// drift crosses square, 0 where it runs parallel and the wedge is symmetric.
///
/// A hard sign here put a discontinuity through every belt, because the wedge
/// flipped its steep flank from one side to the other along the line where the
/// drift turned. Blending on the same quantity removes it without softening the
/// asymmetry where the drift is decisive.
fn vergence_bias(wx: f64, wy: f64, axis: (f64, f64), seed: u64) -> f64 {
    let (ax, ay) = (-axis.1, axis.0);
    let (dx, dy) = plate_drift(wx, wy, seed);
    let m = dx.hypot(dy);
    if m < 1e-12 { return 0.0 }
    -(dx * ax + dy * ay) / m
}

// ── 3. The wedge in cross-section ───────────────────────────────────────────

/// Width of a wedge's steep flank as a share of its graded flank.
///
/// A fold-and-thrust wedge is asymmetric because it is built by thrusts that
/// all verge one way: the forelimb dips 40–60° and the backlimb 10–25°, so the
/// steep flank is roughly a third the width of the graded one at the same
/// height. Both flanks below are scaled from this so their mean is
/// [`BELT_HALF_WIDTH`] and coverage is unchanged.
const STEEP_FLANK_SHARE: f64 = 0.35;

const STEEP_HALF_WIDTH: f64 = 2.0 * BELT_HALF_WIDTH * STEEP_FLANK_SHARE / (1.0 + STEEP_FLANK_SHARE);
const GRADED_HALF_WIDTH: f64 = 2.0 * BELT_HALF_WIDTH / (1.0 + STEEP_FLANK_SHARE);

/// Exponent of the cross-strike amplitude taper.
///
/// A squared taper puts the belt's shoulders where a wedge's are: relief falls
/// away slowly near the crest and steeply at the toe, rather than linearly.
///
/// **This value is currently unjustified.** It was derived from where the fold
/// axis stopped being trustworthy — the share of ground whose fold axis sat
/// more than 32° off its own crest, weighted by the taper and integrated, which
/// put p = 2 as the smallest exponent holding misaligned relief under 1%. The
/// fold texture is gone, so that argument no longer applies to anything. The
/// value is kept because it is measured into the belt's shape and the four
/// fixed measurements depend on it; it wants a derivation of its own.
const TAPER_EXPONENT: f64 = 2.0;

/// Cross-strike amplitude at a signed distance from the crest, positive toward
/// vergence. Zero past the flank's own half-width.
///
/// The asymmetry is in the shape, not in a correction applied to a symmetric
/// one: the steep flank simply reaches zero sooner. An earlier attempt warped
/// the sampling position along vergence in proportion to height, which
/// *translates* a symmetric ridge rather than shearing it and measured 52.0%
/// steeper on the vergence side against a 50% coin flip.
fn wedge(across: f64, asymmetry: f64) -> f64 {
    // Blend between a symmetric wedge and a fully leaning one, on how squarely
    // the drift crosses the belt. A hard switch put a seam down every belt.
    let lean = asymmetry.abs();
    let steep = BELT_HALF_WIDTH + (STEEP_HALF_WIDTH - BELT_HALF_WIDTH) * lean;
    let graded = BELT_HALF_WIDTH + (GRADED_HALF_WIDTH - BELT_HALF_WIDTH) * lean;
    // Which flank is the steep one is the sign of the asymmetry, not a separate
    // switch on which side of the crest this is. At zero asymmetry the two
    // half-widths are equal, so the choice cannot show — which is what lets the
    // sign pass through zero without a seam.
    let half = if across * asymmetry >= 0.0 { steep } else { graded };
    let u = (across.abs() / half).min(1.0);
    (1.0 - u * u).powf(TAPER_EXPONENT)
}

// ── 5. Crust and the ceiling ────────────────────────────────────────────────

/// Vertical spacing of one z-level in world units. Matches `common::camera::RISE`
/// — the flank angles below are angles on the ground the player walks, so the
/// two axes have to be in the same units.
const RISE: f64 = 0.8;

/// Mean flank angle a belt is built to stand at, in degrees.
///
/// The ceiling follows from this and nothing else. Threshold-slope work put
/// repose at 30–35°, and a belt whose *mean* flank already sits at repose
/// leaves dissection nothing to cut steeper into — at 2,000 z the mean flank is
/// 31.4° and the belt is finished before erosion touches it. 20° leaves the
/// whole repose band above it for the dissection layer to work in.
const TARGET_FLANK_DEGREES: f64 = 20.0;

/// Elevation in z-levels a belt reaches at full shortening on continental crust.
///
/// **Height and half-width are coupled, and the coupling is the durable thing
/// here.** What a player experiences is the slope, not the number:
/// `flank = atan(height × RISE / half_width)`. So the ceiling is
/// `half_width × tan(TARGET_FLANK_DEGREES) / RISE` — at 2,625 WU that is 1,194 z,
/// and 1,200 is the same angle to a tenth of a degree. **Widening belts requires
/// raising this proportionally**, or the belt flattens into a ramp.
///
/// What the angle buys, at the current half-width:
/// ```text
///   275 z  =  220 WU  ->   4.8 deg   a ramp
/// 1,200 z  =  960 WU  ->  20.1 deg   a mountain
/// 2,000 z  = 1600 WU  ->  31.4 deg   the whole flank already at repose
/// 4,000 z  = 3200 WU  ->  50.6 deg   a wall
/// ```
///
/// The previous 275 z came from `SEA_MAX_DEPTH × (5.5 / 4.0)`, preserving
/// Earth's orogen-to-abyss ratio. The ratio was right and the anchor was not:
/// `SEA_MAX_DEPTH` is 160 WU against Earth's 4 km abyssal plain, so the world's
/// vertical scale is ~25× compressed and the derivation faithfully reproduced a
/// compressed number. Both of its cross-checks agreed because both stood on the
/// same compressed baseline. Earth-matching cannot fix it either — horizontal is
/// compressed ~400×, and matching vertical to *that* gives 22 m mountains.
///
/// 3,000–4,000 z is wanted eventually. At 20° it needs an 8,800 WU half-width,
/// which needs wider ridge spacing, which needs λ 150,000 and a raised
/// `DRIFT_WAVELENGTH`. That is a separate measurement and is deliberately not
/// taken here.
pub const OROGEN_MAX_RISE: f64 = 1_200.0;

/// How far a full-strength oceanic belt lifts the seafloor, in z-levels.
///
/// This is the quantity with meaning, not the share below it. Against
/// [`SEA_MAX_DEPTH`] of 200 z, a 55 z lift breaches shelf water and not an
/// abyssal plain, which is what makes island arcs appear where convergence is
/// strong and the water shallow rather than everywhere — measured at a 14.9%
/// breach share, neither 0% nor 100%.
///
/// It was 275 z × 7/35 when the ceiling was 275, so the crustal-thickness ratio
/// set it by coincidence. The ceiling has since moved for a reason that has
/// nothing to do with crust thickness, so the lift is now held directly and the
/// share is what falls out.
const OCEANIC_LIFT: f64 = 55.0;

/// Share of full thickening oceanic crust takes, so that a full-strength belt
/// lifts the seafloor by [`OCEANIC_LIFT`]. About 1.6/35 of a continental
/// column, against the 7/35 the crustal-thickness ratio would give.
const OCEANIC_CRUST_SHARE: f64 = OCEANIC_LIFT / OROGEN_MAX_RISE;

/// Share of full thickening the crust at a position can take.
fn crust_share(base: f64) -> f64 {
    let e = base;
    if e >= 0.0 { return 1.0 }
    let submerged = (1.0 + e / SEA_MAX_DEPTH).clamp(0.0, 1.0);
    OCEANIC_CRUST_SHARE + (1.0 - OCEANIC_CRUST_SHARE) * submerged
}

// ── The field ───────────────────────────────────────────────────────────────

/// Elevation the orogen adds at a position, in z-levels. Zero outside a belt.
/// Elevation the orogen adds at a position, in z-levels, given the elevation of
/// the ground it stands on.
///
/// The base decides how much of the thickening reaches the surface: oceanic
/// crust is thin and dense, so a belt built on it floats lower. Taking it as an
/// argument is what lets the event read the layer below rather than recomputing
/// the substrate a second time.
pub fn relief_on(wx: f64, wy: f64, base: f64, seed: u64) -> f64 {
    let Some(c) = project_to_crest(wx, wy, seed) else { return 0.0 };
    let taper = wedge(c.across, c.asymmetry);
    if taper <= 0.0 { return 0.0 }
    OROGEN_MAX_RISE * c.strength * crust_share(base) * taper
}

/// Elevation the orogen adds at a position, reading the substrate itself.
/// Rendering and measurement only — the event passes the layer below instead.
pub fn relief(wx: f64, wy: f64, seed: u64) -> f64 {
    relief_on(wx, wy, substrate_elevation_at(wx, wy, seed), seed)
}

/// Substrate plus orogen. Rendering and measurement only — nothing composes
/// this into the real stack.
pub fn surface(wx: f64, wy: f64, seed: u64) -> f64 {
    substrate_elevation_at(wx, wy, seed) + relief(wx, wy, seed)
}

/// Whether a position lies inside a belt at all.
pub fn is_belt(wx: f64, wy: f64, seed: u64) -> bool {
    project_to_crest(wx, wy, seed).map_or(false, |c| wedge(c.across, c.asymmetry) > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ceiling is the flank angle, restated. If half-width moves and the
    /// ceiling does not, the belt is no longer a mountain — this is the
    /// coupling, asserted rather than described.
    #[test]
    fn ceiling_matches_the_target_flank() {
        let derived = BELT_HALF_WIDTH * TARGET_FLANK_DEGREES.to_radians().tan() / RISE;
        let angle = (OROGEN_MAX_RISE * RISE / BELT_HALF_WIDTH).atan().to_degrees();
        assert!((OROGEN_MAX_RISE - derived).abs() < 10.0,
            "ceiling {OROGEN_MAX_RISE} is {derived} by derivation");
        assert!((angle - TARGET_FLANK_DEGREES).abs() < 0.5,
            "mean flank is {angle} deg against a {TARGET_FLANK_DEGREES} deg target");
    }

    /// The oceanic share exists to hold the lift, so that is what is checked.
    #[test]
    fn oceanic_lift_survives_the_ceiling() {
        assert!((OROGEN_MAX_RISE * OCEANIC_CRUST_SHARE - OCEANIC_LIFT).abs() < 1e-9);
        assert!(OCEANIC_LIFT < SEA_MAX_DEPTH,
            "a full-strength oceanic belt must not clear the abyssal plain by itself");
    }

    /// Where the crest lookup resolves, how confidently, and where it does not.
    /// Reported against depth into the shortening gate, because a failure
    /// concentrated at a belt margin is a taper and one in a belt interior is a
    /// hole.
    #[test]
    #[ignore]
    fn crest_lookup_quality() {
        const S: u64 = 0x9E3779B97F4A7C15;
        let (mut n, mut some, mut full) = (0usize, 0usize, 0usize);
        let (mut fails, mut alls, mut soft) = ([0usize; 5], [0usize; 5], [0usize; 5]);
        for i in 0..800i64 {
            for j in 0..800i64 {
                let wx = -260_000.0 + i as f64 * 150.0;
                let wy = -180_000.0 + j as f64 * 150.0;
                let raw = (shortening(wx, wy, S) / SHORTENING_FULL).clamp(0.0, 1.0);
                if raw < SHORTENING_GATE { continue }
                n += 1;
                let b = (((raw - SHORTENING_GATE) / (1.0 - SHORTENING_GATE)) * 5.0).clamp(0.0, 4.0) as usize;
                alls[b] += 1;
                match project_to_crest(wx, wy, S) {
                    None => fails[b] += 1,
                    Some(_) => { some += 1; full += 1 }
                }
            }
        }
        println!("\n=== CREST LOOKUP QUALITY ===");
        println!("gated positions sampled: {n}");
        println!("  resolved            {some:>8} ({:.2}%)", 100.0 * some as f64 / n as f64);
        println!("  fully certain       {full:>8} ({:.2}%)", 100.0 * full as f64 / n as f64);
        println!("\n  by depth into the gate:");
        println!("    {:<12} {:>9} {:>10} {:>10}", "band", "sampled", "no crest", "tapered");
        for b in 0..5 {
            let lo = SHORTENING_GATE + (1.0 - SHORTENING_GATE) * b as f64 / 5.0;
            let hi = SHORTENING_GATE + (1.0 - SHORTENING_GATE) * (b + 1) as f64 / 5.0;
            println!("    {:<12} {:>9} {:>9.2}% {:>9.2}%", format!("{lo:.2}-{hi:.2}"), alls[b],
                     100.0 * fails[b] as f64 / alls[b].max(1) as f64,
                     100.0 * soft[b] as f64 / alls[b].max(1) as f64);
        }
    }

    /// A hard edge is a jump in relief between neighbouring positions. Find the
    /// jumps first, then ask which projection outcome produced them — assuming
    /// the cause gets the wrong fix.
    #[test]
    #[ignore]
    fn relief_discontinuities() {
        const S: u64 = 0x9E3779B97F4A7C15;
        let step = 12.0;
        let mut jumps: Vec<(f64, f64, f64)> = Vec::new();
        let mut sampled = 0usize;
        for i in 0..1400i64 {
            for j in 0..1400i64 {
                let wx = -215_000.0 + i as f64 * step;
                let wy = -135_000.0 + j as f64 * step;
                let a = relief(wx, wy, S);
                let b = relief(wx + step, wy, S);
                let c = relief(wx, wy + step, S);
                sampled += 1;
                let d = (a - b).abs().max((a - c).abs());
                if d > 1.0 { jumps.push((d, wx, wy)) }
            }
        }
        jumps.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
        // Slope as well as height. A profile truncated by the lookup is
        // continuous in value and broken in slope, so a height-only sweep
        // reports nothing while hillshading draws a hard line.
        let mut kinks: Vec<f64> = Vec::new();
        for i in 1..600i64 {
            for j in 1..600i64 {
                let wx = -215_000.0 + i as f64 * step * 2.0;
                let wy = -135_000.0 + j as f64 * step * 2.0;
                let g = |x: f64, y: f64| {
                    (relief(x + step, y, S) - relief(x - step, y, S)) / (2.0 * step)
                };
                let (a, b, c) = (g(wx, wy), g(wx + step, wy), g(wx - step, wy));
                let k = (b - a).abs().max((a - c).abs());
                if k > 1e-12 { kinks.push(k) }
            }
        }
        kinks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if !kinks.is_empty() {
            let kq = |f: f64| kinks[((kinks.len() - 1) as f64 * f) as usize];
            println!("\n  adjacent-sample slope change, z per WU:");
            println!("    p50 {:.5}  p90 {:.5}  p99 {:.5}  max {:.5}",
                     kq(0.5), kq(0.9), kq(0.99), kinks[kinks.len() - 1]);
            for t in [0.05, 0.2, 0.5] {
                println!("    over {t:.2} z/WU: {:.4}%",
                         100.0 * kinks.iter().filter(|v| **v > t).count() as f64
                             / kinks.len() as f64);
            }
        }
        println!("\n=== RELIEF DISCONTINUITIES ===");
        println!("samples {sampled}, adjacent pairs {step} WU apart");
        println!("jumps over 1 z: {} ({:.3}%)", jumps.len(),
                 100.0 * jumps.len() as f64 / sampled as f64);
        for t in [2.0, 5.0, 20.0, 50.0] {
            let n = jumps.iter().filter(|j| j.0 > t).count();
            println!("  over {t:>5.0} z: {n:>8} ({:.4}%)", 100.0 * n as f64 / sampled as f64);
        }
        println!("\n  largest jumps, and the projection outcome either side:");
        for (d, wx, wy) in jumps.iter().take(10) {
            let l = project_to_crest(*wx, *wy, S);
            let r = project_to_crest(*wx + step, *wy, S);
            let u = project_to_crest(*wx, *wy + step, S);
            let show = |c: &Option<Crest>| match c {
                None => "none".to_string(),
                Some(c) => format!("s{:.2} a{:>7.0} w{:.2}", c.strength, c.across, wedge(c.across, c.asymmetry)),
            };
            println!("    {d:>7.1} z at ({wx:>9.0},{wy:>9.0})  here[{}]  +x[{}]  +y[{}]",
                     show(&l), show(&r), show(&u));
        }
    }


}
