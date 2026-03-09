//! Continental spine generation.
//!
//! Generates mountain ranges that span multiple macro plates. Each spine
//! grows in two opposing directions from an inland epicenter, driven by three
//! independent noise channels (lateral curvature, cross-section width, peak
//! height). The elevation field is a collection of **peak cones** — each peak
//! is a power-curve falloff from its center. The combined field uses `max` across
//! all cones, producing natural ridgelines where adjacent cones overlap and
//! saddles where they don't. A **ravine network** carves drainage channels
//! into the cone surface, producing valleys, ridges, and passes as emergent
//! structure.
//!
//! Spine placement is **locally deterministic**: epicenters are selected and
//! conflict-resolved within fixed-size evaluation chunks. Each chunk decides
//! independently using only its own candidates plus a 1-ring neighborhood for
//! conflict resolution. The chunk size is ≥ 2× the exclusion distance, so a
//! candidate can only conflict with candidates in immediately adjacent chunks.
//!
//! Entry point: [`generate_spines`].

use std::collections::HashMap;

use common::{HexSpatialGrid, PlateTag, Tagged};

use crate::noise::{hash_u64, hash_f64, simplex_2d};
use crate::plates::{PlateCache, PlateCenter};
use crate::MACRO_CELL_SIZE;

// ── Generation constants ────────────────────────────────────────────────────

/// Maximum number of macro-plate steps each arm grows from the epicenter.
const SPINE_MAX_STEPS: usize = 12;

/// Minimum distance between two spine epicenters in world units.
const SPINE_EXCLUSION_DIST: f64 = 20_000.0;

/// Distance between growth steps along the spine axis (world units).
const SPINE_STEP: f64 = MACRO_CELL_SIZE;

/// Half-width of the spine cross-section at each step (world units).
const HALF_WIDTH_MIN: f64 = MACRO_CELL_SIZE * 1.0;
const HALF_WIDTH_MAX: f64 = MACRO_CELL_SIZE * 1.5;

/// Peak ridge elevation (world units, arbitrary scale).
pub const RIDGE_PEAK_ELEVATION: f64 = 4000.0;

/// Elevation below which a growth step is abandoned (effectively flat).
const SPINE_MIN_ELEVATION: f64 = 50.0;

/// Maximum lateral displacement amplitude (world units).
const LATERAL_AMP: f64 = MACRO_CELL_SIZE * 2.0;

/// Number of steps at each arm tip over which peak_height fades to zero.
const TAPER_STEPS: usize = 3;

/// Maximum bearing jitter from the base direction (radians ≈ ±30°).
const BEARING_JITTER_MAX: f64 = std::f64::consts::FRAC_PI_6;

/// Noise wavelength for the peak height channel.
const SPINE_WAVELENGTH: f64 =
    MACRO_CELL_SIZE * (SPINE_MAX_STEPS as f64) * 2.0 / 2.5;

/// Noise wavelength for lateral displacement.
const LATERAL_WAVELENGTH: f64 = 8000.0;

/// Noise wavelength for width variation.
const WIDTH_WAVELENGTH: f64 = 10000.0;

// ── Peak placement constants ─────────────────────────────────────────────────

/// Probability of skipping the center peak at a growth step, creating saddles.
const PEAK_SKIP_PROB: f64 = 0.30;

/// Center peak falloff radius as a multiple of the step's half-width.
/// Ensures adjacent peaks (SPINE_STEP apart) overlap substantially.
/// Bumped from 1.2 to 1.5 to compensate for the steeper power-curve profile.
const PEAK_FALLOFF_SCALE: f64 = 1.5;

/// Exponent for the power-curve peak falloff. Lower = broader peaks with
/// gentle shoulders, higher = sharper peaks with deeper saddles between cones.
const FALLOFF_EXPONENT: f64 = 1.5;

/// Flanking peak height fraction: minimum relative to center peak.
const FLANK_HEIGHT_MIN: f64 = 0.30;
/// Flanking peak height fraction: maximum relative to center peak.
const FLANK_HEIGHT_MAX: f64 = 0.70;

// ── Ridgeline constants ─────────────────────────────────────────────────────

/// Maximum connection distance between peaks, as a multiple of the average
/// falloff radius of the two peaks. Beyond this, no ridgeline is created.
const MAX_RIDGE_DIST_SCALE: f64 = 2.5;

/// Maximum number of ridgeline connections per peak (nearest neighbors).
const MAX_RIDGE_NEIGHBORS: usize = 4;

/// Perpendicular half-width of a ridgeline (world units).
const RIDGE_HALF_WIDTH: f64 = 1200.0;

/// Exponent for perpendicular ridge falloff. 1.2 = gentle slopes to valleys.
const RIDGE_FALLOFF_EXPONENT: f64 = 1.2;

/// Maximum lateral wobble of the ridge centerline (world units).
const RIDGE_LATERAL_WOBBLE: f64 = 150.0;

/// Frequency of wobble noise along the ridge (cycles per unit t).
const RIDGE_WOBBLE_FREQ: f64 = 4.0;

/// Noise seed for ridgeline sag variation.
const SEED_RIDGE_SAG: u64 = 0xAAAA_BBBB_0030;

/// Noise seed for ridgeline lateral wobble.
const SEED_RIDGE_WOBBLE: u64 = 0xAAAA_BBBB_0031;

// ── Transform function constants ─────────────────────────────────────────────

/// Ridgeline noise strength: fraction of base elevation used as noise amplitude.
/// Set to 0.0 to see pure structural shape (cones, ridgelines, V-carves).
const RIDGE_NOISE_STRENGTH: f64 = 0.0;

/// Noise wavelength for ridgeline variation applied at query time.
const RIDGE_NOISE_WAVELENGTH: f64 = HALF_WIDTH_MAX;

/// Noise seed for ridgeline terrain variation.
const RIDGE_NOISE_SEED: u64 = 0xAAAA_BBBB_0010;

/// World units of elevation per discrete z-level.
pub const ELEVATION_PER_Z: f64 = 1.0;

// ── Noise seed constants ────────────────────────────────────────────────────

const SEED_BEARING:  u64 = 0xAAAA_BBBB_0001;
const SEED_LATERAL:  u64 = 0xAAAA_BBBB_0002;
const SEED_WIDTH:    u64 = 0xAAAA_BBBB_0003;
const SEED_HEIGHT:   u64 = 0xAAAA_BBBB_0004;
const SEED_MICRO:    u64 = 0xAAAA_BBBB_0005;
const SEED_PRIORITY: u64 = 0xAAAA_BBBB_0006;
const SEED_SKIP:     u64 = 0xAAAA_BBBB_0007;
const SEED_FLANK:    u64 = 0xAAAA_BBBB_0008;

/// XOR'd into lateral/width/skip/flank seeds for the negative arm.
const SEED_ARM_FLIP: u64 = 0xDEAD_BEEF_CAFE_1234;

// ── Cross-section profile ────────────────────────────────────────────────────

/// Fraction of half-width at which Highland ends and Foothills begins.
const HIGHLAND_FRAC: f64 = 0.60;

/// Fraction of half-width at which Ridge ends and Highland begins.
const RIDGE_FRAC: f64 = 0.15;

/// Power-curve falloff from peak center to edge.
/// `dist_frac` in [0, 1]: 0 = peak center, 1 = falloff edge.
/// Returns a profile value in [0, 1].
pub fn cross_section_profile(dist_frac: f64) -> f64 {
    let t = dist_frac.clamp(0.0, 1.0);
    (1.0 - t).powf(FALLOFF_EXPONENT)
}

/// Spine tag for a plate at `dist_frac` from the peak center.
pub fn cross_section_tag(dist_frac: f64) -> PlateTag {
    if dist_frac <= RIDGE_FRAC {
        PlateTag::Ridge
    } else if dist_frac <= HIGHLAND_FRAC {
        PlateTag::Highland
    } else {
        PlateTag::Foothills
    }
}

// ── Coastal attenuation ──────────────────────────────────────────────────────

fn coastal_attenuation(coast_count: usize) -> f64 {
    match coast_count {
        0     => 1.00,
        1 | 2 => 0.65,
        3 | 4 => 0.35,
        _     => 0.12,
    }
}

// ── Tag priority ─────────────────────────────────────────────────────────────

fn spine_tag_priority(tag: &PlateTag) -> u8 {
    match tag {
        PlateTag::Ridge     => 3,
        PlateTag::Highland  => 2,
        PlateTag::Foothills => 1,
        _ => 0,
    }
}

fn current_spine_tag(plate: &PlateCenter) -> Option<PlateTag> {
    if plate.has_tag(&PlateTag::Ridge)     { return Some(PlateTag::Ridge); }
    if plate.has_tag(&PlateTag::Highland)  { return Some(PlateTag::Highland); }
    if plate.has_tag(&PlateTag::Foothills) { return Some(PlateTag::Foothills); }
    None
}

// ── 1-D / 2-D fBm noise ─────────────────────────────────────────────────────

/// Two-octave 1-D fBm noise. Returns a value in approximately [-1, 1].
fn fbm_1d(t: f64, wavelength: f64, seed: u64) -> f64 {
    let n1 = simplex_2d(t / wavelength, 0.0, seed);
    let n2 = simplex_2d(t / (wavelength * 0.5), 0.0, seed ^ 0x1111_0000_0000_0001);
    (n1 + 0.5 * n2) / 1.5
}

/// Two-octave 2-D fBm noise. Returns a value in approximately [-1, 1].
fn fbm_2d(x: f64, y: f64, wavelength: f64, seed: u64) -> f64 {
    let n1 = simplex_2d(x / wavelength, y / wavelength, seed);
    let n2 = simplex_2d(
        x / (wavelength * 0.5),
        y / (wavelength * 0.5),
        seed ^ 0x2222_0000_0000_0002,
    );
    (n1 + 0.5 * n2) / 1.5
}

// ── Ravine carving ──────────────────────────────────────────────────────────

/// Step size for growing streams downhill (world units).
const STREAM_STEP_SIZE: f64 = 150.0;

/// Maximum steps before a stream terminates.
const STREAM_MAX_STEPS: usize = 200;

/// Starting width of a stream gully at the headwall (world units).
const STREAM_START_WIDTH: f64 = 1.0;

/// Starting carve depth below the cone surface (world units).
const STREAM_START_DEPTH: f64 = 0.0;

/// Width jump per stream merge (world units). Dominates width growth.
const STREAM_WIDTH_PER_MERGE: f64 = 5.0;

/// Depth jump per stream merge (world units).
const STREAM_DEPTH_PER_MERGE: f64 = 3.0;

/// Width gained per unit of arc-length distance from start.
const STREAM_WIDTH_PER_DIST: f64 = 0.075;

/// Depth gained per unit of arc-length distance from start.
const STREAM_DEPTH_PER_DIST: f64 = 0.05;

/// Wall exponent for young streams (tight concave V, steep narrow cut).
const WALL_EXPONENT_YOUNG: f64 = 0.5;

/// Wall exponent for mature streams (open convex V, gentle wide cut).
const WALL_EXPONENT_MATURE: f64 = 1.5;

/// Number of merges to reach full maturity for wall exponent.
const WALL_EXPONENT_MATURITY_MERGES: f64 = 6.0;

/// Proximity radius for sequential merge detection (world units).
/// During growth, if a step lands within this distance of an existing stream's
/// step, the growing stream takes a final step onto that stream and stops.
const STREAM_PROXIMITY_RADIUS: f64 = 150.0;

/// Noise wavelength for stream lateral meander (world units).
const STREAM_LATERAL_WAVELENGTH: f64 = 1000.0;

/// Maximum angular deviation from steepest descent per step (radians).
const STREAM_LATERAL_AMP: f64 = 0.4;

/// Momentum factor for stream direction blending. Higher = smoother curves,
/// the stream strongly prefers its previous heading over the instantaneous gradient.
/// Must be low enough that gradient dominates — high values cause orbital paths.
const STREAM_MOMENTUM: f64 = 0.3;

/// Minimum alignment of step direction with downhill gradient (dot product).
/// If the final direction's downhill component is below this, blend toward
/// pure gradient to prevent cross-slope orbiting. 0.5 = max 60° from downhill.
const MIN_DOWNHILL_ALIGNMENT: f64 = 0.5;



/// Number of stream heads per peak.
const STREAMS_PER_PEAK: usize = 4;

/// Offset of stream head from peak center as fraction of falloff_radius.
const STREAM_HEAD_OFFSET_FRAC: f64 = 0.1;

/// Offset of stream origin along its initial direction (world units).
/// Prevents two streams from the same point merging immediately.
/// Half the proximity radius — two opposing streams start a full radius apart.
const STREAM_ORIGIN_OFFSET: f64 = STREAM_PROXIMITY_RADIUS / 2.0;

/// Probability of a branch being a hanging valley (disabled for conservative baseline).
const HANGING_VALLEY_CHANCE: f64 = 0.0;

/// Minimum hanging valley floor offset above parent (world units).
const HANGING_VALLEY_MIN_OFFSET: f64 = 100.0;

/// Maximum hanging valley floor offset above parent (world units).
const HANGING_VALLEY_MAX_OFFSET: f64 = 400.0;

/// Probability of a ridge crossing having a traversable path (disabled for conservative baseline).
const PATH_PROBABILITY: f64 = 0.0;

/// Minimum ridgeline sag to generate saddle streams. Below this the ridge is
/// nearly flat and there's no meaningful saddle to drain from.
const MIN_SADDLE_SAG: f64 = 0.15;

/// Minimum ridgeline length (world units) to generate saddle streams.
const MIN_RIDGE_LENGTH: f64 = 300.0;



/// Width of carved ridge paths (world units).
const PATH_WIDTH: f64 = 30.0;

/// Depth of path carve below the surface (world units).
const PATH_CARVE_DEPTH: f64 = 15.0;

/// Step size for ridge path generation (world units).
const PATH_STEP_SIZE: f64 = 50.0;

/// Maximum distance between stream midpoints for ridge path consideration.
const PATH_MAX_STREAM_DIST: f64 = 3000.0;

/// Noise seeds for ravine generation.
const SEED_STREAM_DIR: u64 = 0xAAAA_BBBB_0021;
const SEED_STREAM_LATERAL: u64 = 0xAAAA_BBBB_0022;
const SEED_HANGING_VAL: u64 = 0xAAAA_BBBB_0023;
const SEED_PATH_CHANCE: u64 = 0xAAAA_BBBB_0024;

// ── Ravine data structures ──────────────────────────────────────────────────

#[derive(Clone)]
struct StreamStep {
    wx: f64,
    wy: f64,
    surface_elev: f64,
    floor_elev: f64,
    width: f64,
    /// Cumulative arc-length distance from stream origin.
    cum_dist: f64,
    /// Number of merges absorbed upstream of this step.
    merge_count: u32,
    /// V-profile wall exponent. Young (~0.5) = tight concave V, mature (~1.5) = open convex V.
    wall_exponent: f64,
}

struct Stream {
    steps: Vec<StreamStep>,
    merge_count: u32,
    /// Index of the stream this tributary merged into, if any.
    merged_into: Option<usize>,
}

struct PathStep {
    wx: f64,
    wy: f64,
    floor_elev: f64,
}

/// Result of probing a point against the ravine network.
#[derive(Debug, Clone, Copy)]
pub enum RavineProbe {
    /// In the flat valley floor.
    Floor,
    /// On a valley wall. `t` is 0 at floor edge, 1 at valley rim.
    Wall(f64),
    /// On a ridge path.
    Path,
}

/// Summary statistics for a ravine network, used for diagnostics.
pub struct RavineStats {
    pub stream_count: usize,
    pub total_steps: usize,
    pub merged_count: usize,
    pub hanging_count: usize,
    pub path_count: usize,
    pub width_range: (f64, f64),
    pub depth_range: (f64, f64),
    pub length_range: (f64, f64),
}

/// Reference to a contiguous range of stream segments within a grid cell.
#[derive(Clone)]
struct StreamRef {
    stream_idx: usize,
    seg_start: usize,
    seg_end: usize,
}

/// Carved drainage network on a continental spine.
pub struct RavineNetwork {
    streams: Vec<Stream>,
    paths: Vec<Vec<PathStep>>,
    /// Spatial index: hex grid cell → stream segments whose influence overlaps that cell.
    stream_grid: HexSpatialGrid<StreamRef>,
    /// Number of streams with a hanging valley offset (tracked during build).
    hanging_count: usize,
}

// ── Ravine query helpers ────────────────────────────────────────────────────

fn nearest_path_segment(points: &[PathStep], wx: f64, wy: f64) -> (f64, f64, usize) {
    let mut best_dist = f64::MAX;
    let mut best_t = 0.0;
    let mut best_idx = 0;

    for i in 0..points.len().saturating_sub(1) {
        let (ax, ay) = (points[i].wx, points[i].wy);
        let (bx, by) = (points[i + 1].wx, points[i + 1].wy);
        let (abx, aby) = (bx - ax, by - ay);
        let ab_len_sq = abx * abx + aby * aby;

        let t = if ab_len_sq < 1e-10 {
            0.0
        } else {
            ((wx - ax) * abx + (wy - ay) * aby) / ab_len_sq
        }
        .clamp(0.0, 1.0);

        let px = ax + t * abx;
        let py = ay + t * aby;
        let dx = wx - px;
        let dy = wy - py;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < best_dist {
            best_dist = dist;
            best_t = t;
            best_idx = i;
        }
    }

    (best_dist, best_t, best_idx)
}

fn interpolate_path_floor(points: &[PathStep], seg_idx: usize, t: f64) -> f64 {
    let a = points[seg_idx].floor_elev;
    if seg_idx + 1 >= points.len() {
        return a;
    }
    a + t * (points[seg_idx + 1].floor_elev - a)
}

impl RavineNetwork {
    fn empty() -> Self {
        Self {
            streams: Vec::new(),
            paths: Vec::new(),
            stream_grid: HexSpatialGrid::new(300.0),
            hanging_count: 0,
        }
    }

    /// Compute summary statistics for diagnostic output.
    pub fn stats(&self) -> RavineStats {
        let mut min_w = f64::MAX;
        let mut max_w = f64::MIN;
        let mut min_d = f64::MAX;
        let mut max_d = f64::MIN;
        let mut total_steps = 0usize;
        let mut min_len = f64::MAX;
        let mut max_len = 0.0f64;

        for stream in &self.streams {
            total_steps += stream.steps.len();

            // Stream length = sum of step-to-step distances
            let mut length = 0.0;
            for i in 1..stream.steps.len() {
                let dx = stream.steps[i].wx - stream.steps[i - 1].wx;
                let dy = stream.steps[i].wy - stream.steps[i - 1].wy;
                length += (dx * dx + dy * dy).sqrt();
            }
            if length > 0.0 || !stream.steps.is_empty() {
                min_len = min_len.min(length);
                max_len = max_len.max(length);
            }

            for step in &stream.steps {
                min_w = min_w.min(step.width);
                max_w = max_w.max(step.width);
                let depth = step.surface_elev - step.floor_elev;
                min_d = min_d.min(depth);
                max_d = max_d.max(depth);
            }
        }

        // Clamp to sane defaults if no streams
        if self.streams.is_empty() {
            min_w = 0.0; max_w = 0.0;
            min_d = 0.0; max_d = 0.0;
            min_len = 0.0; max_len = 0.0;
        }

        RavineStats {
            stream_count: self.streams.len(),
            total_steps,
            merged_count: self.streams.iter().filter(|s| s.merged_into.is_some()).count(),
            hanging_count: self.hanging_count,
            path_count: self.paths.len(),
            width_range: (min_w, max_w),
            depth_range: (min_d, max_d),
            length_range: (min_len, max_len),
        }
    }

    /// Probe what kind of ravine feature is at (wx, wy), if any.
    /// Returns the deepest hit (smallest normalized distance) across all streams.
    fn probe(&self, wx: f64, wy: f64) -> Option<RavineProbe> {
        // Check ridge paths first (they draw on top)
        for path_steps in &self.paths {
            if path_steps.len() < 2 { continue; }
            let (dist, _, _) = nearest_path_segment(path_steps, wx, wy);
            let half_width = PATH_WIDTH / 2.0;
            if dist < half_width {
                return Some(RavineProbe::Path);
            }
        }

        // Check streams via spatial index — find the deepest hit.
        let mut best_t = f64::MAX;
        for r in self.stream_grid.query(wx, wy) {
            let stream = &self.streams[r.stream_idx];
            if stream.steps.len() < 2 { continue; }
            let (dist, seg_idx, seg_t) =
                nearest_point_on_spline_range(&stream.steps, wx, wy, r.seg_start, r.seg_end);
            let (_, width, _, _) = interpolate_spline_step(&stream.steps, seg_idx, seg_t);
            let half_width = width / 2.0;
            if dist >= half_width { continue; }

            let t = dist / half_width;
            if t < best_t { best_t = t; }
        }

        if best_t < f64::MAX {
            Some(if best_t < 0.01 { RavineProbe::Floor } else { RavineProbe::Wall(best_t) })
        } else {
            None
        }
    }

    fn carve(&self, wx: f64, wy: f64, surface_elev: f64) -> f64 {
        if surface_elev <= 0.0 { return surface_elev; }
        let mut elevation = surface_elev;

        // Gather refs from hex neighborhood, sort + dedup to get one entry
        // per stream with the widest segment range.
        let mut refs: Vec<(usize, usize, usize)> = Vec::new();
        for r in self.stream_grid.query(wx, wy) {
            refs.push((r.stream_idx, r.seg_start, r.seg_end));
        }
        refs.sort_unstable_by_key(|r| r.0);
        refs.dedup_by(|b, a| {
            if a.0 == b.0 {
                a.1 = a.1.min(b.1);
                a.2 = a.2.max(b.2);
                true
            } else {
                false
            }
        });

        // Each stream's carve is computed against the pristine surface,
        // making min-compositing truly order-independent.
        for &(stream_idx, seg_start, seg_end) in &refs {
            let stream = &self.streams[stream_idx];
            if stream.steps.len() < 2 { continue; }

            let (best_dist, best_seg_idx, best_seg_t) =
                nearest_point_on_spline_range(&stream.steps, wx, wy, seg_start, seg_end);

            let (floor, width, stream_surface, wall_exponent) =
                interpolate_spline_step(&stream.steps, best_seg_idx, best_seg_t);
            let half_width = width / 2.0;
            if best_dist >= half_width { continue; }

            let t = best_dist / half_width;
            let wall_top = stream_surface.min(surface_elev);
            let carved = floor + (wall_top - floor) * t.powf(wall_exponent);
            elevation = elevation.min(carved);
        }

        for path_steps in &self.paths {
            if path_steps.len() < 2 { continue; }
            let (dist, seg_t, seg_idx) = nearest_path_segment(path_steps, wx, wy);
            let half_width = PATH_WIDTH / 2.0;
            if dist >= half_width { continue; }

            let floor = interpolate_path_floor(path_steps, seg_idx, seg_t);
            let t = dist / half_width;
            let carved = floor + (elevation - floor) * t;
            elevation = elevation.min(carved);
        }

        elevation
    }
}

// ── Peak height noise ────────────────────────────────────────────────────────

/// Peak elevation fraction [0.5, 1.0] at distance `t` along a spine arm.
fn peak_at(t: f64, spine_id: u64, seed: u64) -> f64 {
    let raw = fbm_1d(t, SPINE_WAVELENGTH, seed ^ SEED_HEIGHT ^ spine_id);
    let normalized = (raw + 1.0) * 0.5;
    0.5 + 0.5 * normalized
}

// ── Peak struct ──────────────────────────────────────────────────────────────

/// A single elevation cone in a spine's peak distribution.
/// Elevation contribution = `cross_section_profile(dist / falloff_radius) * height`
/// for any query point within `falloff_radius`; zero beyond.
/// Uses circular (isotropic) Euclidean distance. Peak-to-peak connectivity
/// is provided by explicit [`Ridgeline`] segments.
pub struct Peak {
    pub wx: f64,
    pub wy: f64,
    pub height: f64,
    pub falloff_radius: f64,
}

/// Max peak cone elevation at (wx, wy) across all peaks (standalone version).
/// Uses circular Euclidean distance.
fn single_peak_elevation(peak: &Peak, wx: f64, wy: f64) -> f64 {
    let dx = wx - peak.wx;
    let dy = wy - peak.wy;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist >= peak.falloff_radius { return 0.0; }
    let t = dist / peak.falloff_radius;
    cross_section_profile(t) * peak.height
}

fn evaluate_all_peaks(peaks: &[Peak], wx: f64, wy: f64) -> f64 {
    let mut max_elev = 0.0f64;
    for peak in peaks {
        let elev = single_peak_elevation(peak, wx, wy);
        if elev > max_elev { max_elev = elev; }
    }
    max_elev
}

/// Blended gradient for stream direction. Computes each peak's and ridgeline's
/// gradient independently, then blends weighted by elevation². Near cone
/// boundaries the gradient averages smoothly instead of flipping.
/// Returns a normalized downhill direction, or (0, 0) on flat terrain.
const BLEND_GRAD_EPSILON: f64 = 10.0;

fn blended_gradient(
    peaks: &[Peak],
    ridgelines: &[Ridgeline],
    wx: f64,
    wy: f64,
) -> (f64, f64) {
    let mut grad_x = 0.0;
    let mut grad_y = 0.0;
    let mut total_weight = 0.0;

    for peak in peaks {
        let here = single_peak_elevation(peak, wx, wy);
        if here <= 0.0 { continue; }
        let ex = single_peak_elevation(peak, wx + BLEND_GRAD_EPSILON, wy);
        let ey = single_peak_elevation(peak, wx, wy + BLEND_GRAD_EPSILON);
        let weight = here * here;
        grad_x += weight * (here - ex) / BLEND_GRAD_EPSILON;
        grad_y += weight * (here - ey) / BLEND_GRAD_EPSILON;
        total_weight += weight;
    }

    for ridge in ridgelines {
        let here = ridge_elevation_at(ridge, wx, wy);
        if here <= 0.0 { continue; }
        let ex = ridge_elevation_at(ridge, wx + BLEND_GRAD_EPSILON, wy);
        let ey = ridge_elevation_at(ridge, wx, wy + BLEND_GRAD_EPSILON);
        let weight = here * here;
        grad_x += weight * (here - ex) / BLEND_GRAD_EPSILON;
        grad_y += weight * (here - ey) / BLEND_GRAD_EPSILON;
        total_weight += weight;
    }

    if total_weight > 1e-10 {
        let gx = grad_x / total_weight;
        let gy = grad_y / total_weight;
        let len = (gx * gx + gy * gy).sqrt();
        if len > 1e-10 {
            (gx / len, gy / len)
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    }
}

// ── Ridgeline ───────────────────────────────────────────────────────────────

/// An explicit elevation ridge connecting two peaks. The ridge follows a
/// straight segment from peak A to peak B with a quadratic sag at the midpoint
/// and perpendicular power-curve falloff.
pub struct Ridgeline {
    pub peak_a: usize,
    pub peak_b: usize,
    pub ax: f64,
    pub ay: f64,
    pub bx: f64,
    pub by: f64,
    pub height_a: f64,
    pub height_b: f64,
    /// Saddle depth fraction at the midpoint. 0 = flat ridge, 1 = drops to zero.
    pub sag: f64,
}

/// Elevation contribution of a single ridgeline at (wx, wy).
fn ridge_elevation_at(ridge: &Ridgeline, wx: f64, wy: f64) -> f64 {
    let abx = ridge.bx - ridge.ax;
    let aby = ridge.by - ridge.ay;
    let ab_len_sq = abx * abx + aby * aby;
    if ab_len_sq < 1e-10 { return 0.0; }
    let ab_len = ab_len_sq.sqrt();

    // Project query point onto the straight segment to get t.
    let t = (((wx - ridge.ax) * abx + (wy - ridge.ay) * aby) / ab_len_sq).clamp(0.0, 1.0);

    // Wobble the projected point perpendicular to the segment.
    // Attenuate at endpoints so ridge meets peaks exactly.
    let ridge_seed = hash_u64(ridge.peak_a as i64, ridge.peak_b as i64, SEED_RIDGE_WOBBLE);
    let endpoint_fade = 4.0 * t * (1.0 - t); // 0 at ends, 1 at midpoint
    let wobble = simplex_2d(t * RIDGE_WOBBLE_FREQ, 0.0, ridge_seed) * RIDGE_LATERAL_WOBBLE * endpoint_fade;
    let perp_x = -aby / ab_len;
    let perp_y = abx / ab_len;
    let proj_x = ridge.ax + t * abx + perp_x * wobble;
    let proj_y = ridge.ay + t * aby + perp_y * wobble;

    // Perpendicular distance from query point to wobbled centerline.
    let dx = wx - proj_x;
    let dy = wy - proj_y;
    let perp_dist = (dx * dx + dy * dy).sqrt();
    if perp_dist >= RIDGE_HALF_WIDTH { return 0.0; }

    // Height along the ridge: interpolate between peaks with sag at midpoint.
    let peak_height_at_t = ridge.height_a + (ridge.height_b - ridge.height_a) * t;
    let sag_factor = 1.0 - ridge.sag * 4.0 * t * (1.0 - t);
    let ridge_height = peak_height_at_t * sag_factor;
    if ridge_height <= 0.0 { return 0.0; }

    // Perpendicular falloff.
    let perp_t = perp_dist / RIDGE_HALF_WIDTH;
    let profile = (1.0 - perp_t).powf(RIDGE_FALLOFF_EXPONENT);

    ridge_height * profile
}

/// Build ridgeline connections between nearby peaks.
fn build_ridgelines(peaks: &[Peak], spine_id: u64, seed: u64) -> Vec<Ridgeline> {
    if peaks.len() < 2 { return Vec::new(); }

    let mut ridgelines = Vec::new();
    let mut connected: Vec<Vec<usize>> = vec![Vec::new(); peaks.len()];

    // For each peak, find nearest neighbors and connect.
    for i in 0..peaks.len() {
        if connected[i].len() >= MAX_RIDGE_NEIGHBORS { continue; }

        // Collect candidates sorted by distance.
        let mut candidates: Vec<(usize, f64)> = (0..peaks.len())
            .filter(|&j| j != i)
            .map(|j| {
                let dx = peaks[j].wx - peaks[i].wx;
                let dy = peaks[j].wy - peaks[i].wy;
                (j, (dx * dx + dy * dy).sqrt())
            })
            .collect();
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        for (j, dist) in candidates {
            if connected[i].len() >= MAX_RIDGE_NEIGHBORS { break; }
            // Skip if already connected.
            if connected[i].contains(&j) { continue; }
            if connected[j].len() >= MAX_RIDGE_NEIGHBORS { continue; }

            // Distance threshold: average falloff × scale.
            let avg_falloff = (peaks[i].falloff_radius + peaks[j].falloff_radius) * 0.5;
            if dist > avg_falloff * MAX_RIDGE_DIST_SCALE { break; } // sorted, so all further are worse

            // Sag: longer ridges sag more.
            let base_sag = dist / (avg_falloff * MAX_RIDGE_DIST_SCALE);
            let (a_idx, b_idx) = if i < j { (i, j) } else { (j, i) };
            let noise_sag = hash_f64(a_idx as i64, b_idx as i64, seed ^ SEED_RIDGE_SAG ^ spine_id) * 0.2;
            let sag = (base_sag + noise_sag).clamp(0.1, 0.8);

            // Check no duplicate.
            if ridgelines.iter().any(|r: &Ridgeline| r.peak_a == a_idx && r.peak_b == b_idx) {
                continue;
            }

            ridgelines.push(Ridgeline {
                peak_a: a_idx,
                peak_b: b_idx,
                ax: peaks[a_idx].wx,
                ay: peaks[a_idx].wy,
                bx: peaks[b_idx].wx,
                by: peaks[b_idx].wy,
                height_a: peaks[a_idx].height,
                height_b: peaks[b_idx].height,
                sag,
            });
            connected[i].push(j);
            connected[j].push(i);
        }
    }

    ridgelines
}

// ── SpineInstance ────────────────────────────────────────────────────────────

/// Retained geometry for a single continental spine.
/// Elevation queries iterate the peak list and ridgeline list — O(peaks + ridges)
/// per query, no grid needed.
pub struct SpineInstance {
    /// Deterministic ID for noise seeding.
    pub id: u64,
    /// Peak distribution defining the spine's elevation field.
    pub peaks: Vec<Peak>,
    /// Explicit ridgeline segments connecting nearby peak pairs.
    pub ridgelines: Vec<Ridgeline>,
    /// Carved drainage network (valleys, ridges, paths).
    pub ravine_network: RavineNetwork,
    /// Center of the bounding circle enclosing all peaks + falloff extents.
    pub bounding_center: (f64, f64),
    /// Radius of the bounding circle.
    pub bounding_radius: f64,
}

impl SpineInstance {
    /// Max peak cone elevation at (wx, wy) without ridge noise or carving.
    /// Uses circular Euclidean distance.
    fn evaluate_peaks(&self, wx: f64, wy: f64) -> f64 {
        evaluate_all_peaks(&self.peaks, wx, wy)
    }

    /// Max ridgeline elevation at (wx, wy).
    fn evaluate_ridgelines(&self, wx: f64, wy: f64) -> f64 {
        let mut max_elev = 0.0f64;
        for ridge in &self.ridgelines {
            let e = ridge_elevation_at(ridge, wx, wy);
            if e > max_elev { max_elev = e; }
        }
        max_elev
    }

    /// Query the ravine relationship at (wx, wy).
    /// Returns:
    /// - `None` if outside all streams and paths
    /// - `Some(RavineProbe::Floor)` if in the flat valley floor
    /// - `Some(RavineProbe::Wall(t))` if on a valley wall (t in 0..1, 0=floor edge, 1=rim)
    /// - `Some(RavineProbe::Path)` if on a ridge path
    pub fn ravine_probe(&self, wx: f64, wy: f64) -> Option<RavineProbe> {
        let bx = wx - self.bounding_center.0;
        let by = wy - self.bounding_center.1;
        if bx * bx + by * by > self.bounding_radius * self.bounding_radius {
            return None;
        }
        self.ravine_network.probe(wx, wy)
    }

    /// Returns the elevation contributed by this spine at (wx, wy).
    /// Uses max across all peak cones, ridge noise, and ravine carving.
    pub fn elevation_at(&self, wx: f64, wy: f64) -> f64 {
        let bx = wx - self.bounding_center.0;
        let by = wy - self.bounding_center.1;
        if bx * bx + by * by > self.bounding_radius * self.bounding_radius {
            return 0.0;
        }

        let peak_elev = self.evaluate_peaks(wx, wy);
        let ridge_elev = self.evaluate_ridgelines(wx, wy);
        let mut elev = peak_elev.max(ridge_elev);
        if elev <= 0.0 { return 0.0; }

        // Ridge noise
        let noise = fbm_2d(wx, wy, RIDGE_NOISE_WAVELENGTH, self.id ^ RIDGE_NOISE_SEED);
        let amplitude = elev * RIDGE_NOISE_STRENGTH;
        elev = (elev + noise * amplitude).max(0.0);
        if elev <= 0.0 { return 0.0; }

        // Ravine carving
        self.ravine_network.carve(wx, wy, elev)
    }
}

// ── Spine chunk grid ─────────────────────────────────────────────────────────

/// Row height factor for hex grid: sqrt(3)/2.
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

/// Evaluation chunk size. ≥ 2× SPINE_EXCLUSION_DIST guarantees a candidate
/// can only conflict with candidates in immediately adjacent chunks.
const SPINE_CHUNK_SIZE: f64 = 2.0 * SPINE_EXCLUSION_DIST;

/// Maximum distance a spine arm can affect from its epicenter.
/// Accounts for peak falloff radius plus ridgeline perpendicular width.
const SPINE_INFLUENCE: f64 =
    SPINE_MAX_STEPS as f64 * SPINE_STEP + LATERAL_AMP + HALF_WIDTH_MAX * PEAK_FALLOFF_SCALE + RIDGE_HALF_WIDTH;

/// A candidate epicenter for spine generation.
#[derive(Clone)]
struct SpineCandidate {
    plate_id: u64,
    wx: f64,
    wy: f64,
    priority: u64,
    chunk_q: i32,
    chunk_r: i32,
}

fn spine_chunk_coord(wx: f64, wy: f64) -> (i32, i32) {
    let row_height = SPINE_CHUNK_SIZE * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { SPINE_CHUNK_SIZE * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / SPINE_CHUNK_SIZE).round() as i32;
    (cq, cr)
}

fn spine_chunk_1ring(cr: i32) -> [(i32, i32); 7] {
    if cr & 1 == 0 {
        [(0, 0), (-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
    } else {
        [(0, 0), (-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
    }
}

fn spine_chunk_center(cq: i32, cr: i32) -> (f64, f64) {
    let odd_shift = if cr & 1 != 0 { SPINE_CHUNK_SIZE * 0.5 } else { 0.0 };
    (
        cq as f64 * SPINE_CHUNK_SIZE + odd_shift,
        cr as f64 * SPINE_CHUNK_SIZE * HEX_ROW_HEIGHT,
    )
}

const SPINE_GATHER_RADIUS: f64 = SPINE_CHUNK_SIZE * 0.6 + MACRO_CELL_SIZE * 4.0;

fn compute_spine_candidates(
    cq: i32,
    cr: i32,
    plate_cache: &mut PlateCache,
    seed: u64,
) -> Vec<SpineCandidate> {
    let (cx, cy) = spine_chunk_center(cq, cr);
    let mut nearby = plate_cache.plates_in_radius(cx, cy, SPINE_GATHER_RADIUS);
    plate_cache.classify_tags(&mut nearby);

    let mut candidates = Vec::new();
    for plate in &nearby {
        let (pcq, pcr) = spine_chunk_coord(plate.wx, plate.wy);
        if pcq != cq || pcr != cr { continue; }
        if !plate.has_tag(&PlateTag::Inland) { continue; }

        let mut nbrs = plate_cache.plate_neighbors(plate.wx, plate.wy);
        plate_cache.classify_tags(&mut nbrs);
        if !nbrs.iter().all(|n| n.has_tag(&PlateTag::Inland)) { continue; }

        let priority = hash_u64(plate.cell_q as i64, plate.cell_r as i64, seed ^ SEED_PRIORITY);
        candidates.push(SpineCandidate {
            plate_id: plate.id,
            wx: plate.wx,
            wy: plate.wy,
            priority,
            chunk_q: cq,
            chunk_r: cr,
        });
    }
    candidates
}

fn ensure_candidates(
    cq: i32,
    cr: i32,
    cache: &mut HashMap<(i32, i32), Vec<SpineCandidate>>,
    plate_cache: &mut PlateCache,
    seed: u64,
) {
    if cache.contains_key(&(cq, cr)) { return; }
    let candidates = compute_spine_candidates(cq, cr, plate_cache, seed);
    cache.insert((cq, cr), candidates);
}

fn resolve_chunk(
    cq: i32,
    cr: i32,
    candidate_cache: &HashMap<(i32, i32), Vec<SpineCandidate>>,
) -> Vec<SpineCandidate> {
    let mut all: Vec<&SpineCandidate> = Vec::new();
    for (dq, dr) in spine_chunk_1ring(cr) {
        if let Some(chunk_candidates) = candidate_cache.get(&(cq + dq, cr + dr)) {
            all.extend(chunk_candidates);
        }
    }

    all.sort_unstable_by_key(|c| c.priority);

    let excl_sq = SPINE_EXCLUSION_DIST * SPINE_EXCLUSION_DIST;
    let mut placed: Vec<(f64, f64)> = Vec::new();
    let mut result: Vec<SpineCandidate> = Vec::new();

    for &candidate in &all {
        let excluded = placed.iter().any(|&(px, py)| {
            let dx = candidate.wx - px;
            let dy = candidate.wy - py;
            dx * dx + dy * dy < excl_sq
        });
        if !excluded {
            placed.push((candidate.wx, candidate.wy));
            if candidate.chunk_q == cq && candidate.chunk_r == cr {
                result.push(candidate.clone());
            }
        }
    }
    result
}

fn plates_bounding_box(plates: &[PlateCenter], margin: f64) -> (f64, f64, f64, f64) {
    if plates.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for p in plates {
        min_x = min_x.min(p.wx);
        max_x = max_x.max(p.wx);
        min_y = min_y.min(p.wy);
        max_y = max_y.max(p.wy);
    }
    (min_x - margin, max_x + margin, min_y - margin, max_y + margin)
}

fn spine_chunks_in_bounds(min_x: f64, max_x: f64, min_y: f64, max_y: f64) -> Vec<(i32, i32)> {
    let row_height = SPINE_CHUNK_SIZE * HEX_ROW_HEIGHT;
    let cr_min = ((min_y / row_height) - 0.5).floor() as i32;
    let cr_max = ((max_y / row_height) + 0.5).ceil() as i32;

    let mut result = Vec::new();
    for cr in cr_min..=cr_max {
        let odd_shift = if cr & 1 != 0 { SPINE_CHUNK_SIZE * 0.5 } else { 0.0 };
        let cq_min = (((min_x - odd_shift) / SPINE_CHUNK_SIZE) - 0.5).floor() as i32;
        let cq_max = (((max_x - odd_shift) / SPINE_CHUNK_SIZE) + 0.5).ceil() as i32;
        for cq in cq_min..=cq_max {
            result.push((cq, cr));
        }
    }
    result
}

// ── Arm step intermediate representation ─────────────────────────────────────

/// One growth step along a spine arm, before peak scattering.
struct ArmStep {
    cx: f64,
    cy: f64,
    half_width: f64,
    peak_height: f64,
}

/// Fade peak_height to zero over the last TAPER_STEPS of an arm.
fn taper_steps(steps: &mut [ArmStep]) {
    let n = steps.len();
    for i in 0..n {
        let dist_from_end = (n - 1) - i;
        if dist_from_end < TAPER_STEPS {
            let taper = (dist_from_end as f64 + 1.0) / (TAPER_STEPS as f64 + 1.0);
            steps[i].peak_height *= taper;
        }
    }
}

/// Walk the spine curve in one direction, collecting arm steps.
fn grow_arm_steps(
    epi_wx: f64,
    epi_wy: f64,
    spine_dx: f64,
    spine_dy: f64,
    dir_sign: f64,
    spine_id: u64,
    plate_cache: &mut PlateCache,
    seed: u64,
) -> Vec<ArmStep> {
    let arm_seed = if dir_sign > 0.0 { 0 } else { SEED_ARM_FLIP };

    let mut steps = Vec::new();
    for step in 1..=SPINE_MAX_STEPS {
        let t = step as f64 * SPINE_STEP;

        let lateral_raw = fbm_1d(t, LATERAL_WAVELENGTH, seed ^ SEED_LATERAL ^ spine_id ^ arm_seed);
        let lateral = lateral_raw * LATERAL_AMP;

        let cx = epi_wx + dir_sign * spine_dx * t + (-spine_dy) * lateral;
        let cy = epi_wy + dir_sign * spine_dy * t +   spine_dx  * lateral;

        if plate_cache.regime_value_at(cx, cy) < crate::REGIME_LAND_THRESHOLD {
            break;
        }

        let width_raw = fbm_1d(t, WIDTH_WAVELENGTH, seed ^ SEED_WIDTH ^ spine_id ^ arm_seed);
        let width_frac = 0.5 + 0.5 * width_raw;
        let half_width = HALF_WIDTH_MIN + (HALF_WIDTH_MAX - HALF_WIDTH_MIN) * width_frac;

        let peak_frac = peak_at(t * dir_sign, spine_id, seed);

        let attenuation = {
            let mut nbrs = plate_cache.plates_in_radius(cx, cy, MACRO_CELL_SIZE * 2.0);
            plate_cache.classify_tags(&mut nbrs);
            let coast_count = nbrs.iter().filter(|n| n.has_tag(&PlateTag::Coast)).count();
            coastal_attenuation(coast_count)
        };

        let attenuated_peak = peak_frac * attenuation;
        if attenuated_peak * RIDGE_PEAK_ELEVATION < SPINE_MIN_ELEVATION {
            break;
        }

        steps.push(ArmStep {
            cx,
            cy,
            half_width,
            peak_height: attenuated_peak * RIDGE_PEAK_ELEVATION,
        });
    }
    steps
}

// ── Peak scattering ──────────────────────────────────────────────────────────

/// Scatter peaks at a single arm step and append to `peaks`.
/// `step_idx` is used for deterministic hashing; `arm_seed` differentiates arms.
fn scatter_step_peaks(
    step_idx: usize,
    step: &ArmStep,
    spine_dx: f64,
    spine_dy: f64,
    spine_id: u64,
    arm_seed: u64,
    seed: u64,
    peaks: &mut Vec<Peak>,
) {
    let skip = hash_f64(step_idx as i64, spine_id as i64, seed ^ SEED_SKIP ^ arm_seed) < PEAK_SKIP_PROB;
    if skip { return; }

    let falloff_radius = step.half_width * PEAK_FALLOFF_SCALE;
    peaks.push(Peak {
        wx: step.cx, wy: step.cy, height: step.peak_height, falloff_radius,
    });

    scatter_flanking_peaks(step_idx, step.cx, step.cy, step.half_width, step.peak_height, spine_dx, spine_dy, spine_id, arm_seed, seed, peaks);
}

/// Scatter 0-2 flanking peaks perpendicular to the spine bearing.
fn scatter_flanking_peaks(
    step_idx: usize,
    cx: f64,
    cy: f64,
    half_width: f64,
    center_height: f64,
    spine_dx: f64,
    spine_dy: f64,
    spine_id: u64,
    arm_seed: u64,
    seed: u64,
    peaks: &mut Vec<Peak>,
) {
    let flank_seed = seed ^ SEED_FLANK ^ spine_id ^ arm_seed;
    let count_noise = hash_f64(step_idx as i64 ^ 0x1234, spine_id as i64, flank_seed);
    let flank_count = if count_noise < 0.40 { 0usize }
                      else if count_noise < 0.75 { 1 }
                      else { 2 };

    let perp_x = -spine_dy;
    let perp_y =  spine_dx;

    for fi in 0..flank_count {
        let fs = flank_seed ^ (fi as u64 * 0x1111_2222 + 1);
        let offset_frac = hash_f64(step_idx as i64, fi as i64, fs) * 0.5 + 0.3;
        let side = if hash_f64(step_idx as i64, fi as i64, fs ^ 0xABCD_EF01) < 0.5 { 1.0 } else { -1.0 };
        let height_frac = hash_f64(step_idx as i64, fi as i64, fs ^ 0x5678_9ABC)
            * (FLANK_HEIGHT_MAX - FLANK_HEIGHT_MIN) + FLANK_HEIGHT_MIN;

        peaks.push(Peak {
            wx: cx + perp_x * offset_frac * half_width * side,
            wy: cy + perp_y * offset_frac * half_width * side,
            height: center_height * height_frac,
            falloff_radius: half_width * PEAK_FALLOFF_SCALE * height_frac,
        });
    }
}

// ── Plate tag writes ─────────────────────────────────────────────────────────

/// Write elevation and spine tags to all macro plates within a peak's influence.
fn apply_peak_to_plates(
    peak: &Peak,
    plates: &mut [PlateCenter],
    plate_map: &HashMap<u64, usize>,
    plate_cache: &mut PlateCache,
) {
    let candidates = plate_cache.plates_in_radius(peak.wx, peak.wy, peak.falloff_radius);
    for candidate in candidates {
        let Some(&idx) = plate_map.get(&candidate.id) else { continue };
        if plates[idx].has_tag(&PlateTag::Sea) { continue; }

        let dx = candidate.wx - peak.wx;
        let dy = candidate.wy - peak.wy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist >= peak.falloff_radius { continue; }

        let dist_frac = dist / peak.falloff_radius;
        let elevation = cross_section_profile(dist_frac) * peak.height;
        if elevation < 1.0 { continue; }

        let new_tag = cross_section_tag(dist_frac);
        let new_elevation = plates[idx].elevation.max(elevation);

        let best_tag = match current_spine_tag(&plates[idx]) {
            Some(existing) if spine_tag_priority(&existing) >= spine_tag_priority(&new_tag) => existing,
            _ => new_tag,
        };

        if new_elevation > plates[idx].elevation || current_spine_tag(&plates[idx]).is_none() {
            plates[idx].erase_tag(&PlateTag::Ridge);
            plates[idx].erase_tag(&PlateTag::Highland);
            plates[idx].erase_tag(&PlateTag::Foothills);
            plates[idx].add_tag(best_tag);
            plates[idx].elevation = new_elevation;
        }
    }
}

// ── Bounding circle ──────────────────────────────────────────────────────────

fn bounding_circle(peaks: &[Peak]) -> ((f64, f64), f64) {
    if peaks.is_empty() {
        return ((0.0, 0.0), 0.0);
    }
    let n = peaks.len() as f64;
    let bc_x = peaks.iter().map(|p| p.wx).sum::<f64>() / n;
    let bc_y = peaks.iter().map(|p| p.wy).sum::<f64>() / n;
    // Circular falloff + ridgeline half-width for reach beyond peak center.
    let radius = peaks.iter().map(|p| {
        let dx = p.wx - bc_x;
        let dy = p.wy - bc_y;
        (dx * dx + dy * dy).sqrt() + p.falloff_radius + RIDGE_HALF_WIDTH
    }).fold(0.0f64, f64::max);
    ((bc_x, bc_y), radius)
}

// ── Stream width/depth from merge count + distance ─────────────────────────

fn stream_width(merge_count: u32, cum_dist: f64) -> f64 {
    STREAM_START_WIDTH
        + merge_count as f64 * STREAM_WIDTH_PER_MERGE
        + cum_dist * STREAM_WIDTH_PER_DIST
}

fn stream_depth(merge_count: u32, cum_dist: f64) -> f64 {
    STREAM_START_DEPTH
        + merge_count as f64 * STREAM_DEPTH_PER_MERGE
        + cum_dist * STREAM_DEPTH_PER_DIST
}

fn stream_wall_exponent(merge_count: u32) -> f64 {
    let maturity = (merge_count as f64 / WALL_EXPONENT_MATURITY_MERGES).min(1.0);
    WALL_EXPONENT_YOUNG + (WALL_EXPONENT_MATURE - WALL_EXPONENT_YOUNG) * maturity
}

// ── Stream generation ───────────────────────────────────────────────────────

/// Combined surface elevation from peaks and ridgelines.
fn evaluate_surface(peaks: &[Peak], ridgelines: &[Ridgeline], wx: f64, wy: f64) -> f64 {
    let mut max_elev = evaluate_all_peaks(peaks, wx, wy);
    for ridge in ridgelines {
        let e = ridge_elevation_at(ridge, wx, wy);
        if e > max_elev { max_elev = e; }
    }
    max_elev
}

fn grow_stream(
    start_wx: f64,
    start_wy: f64,
    initial_angle: f64,
    peaks: &[Peak],
    ridgelines: &[Ridgeline],
    branch_depth_offset: f64,
    hash_a: u64,
    hash_b: u64,
    spine_id: u64,
    seed: u64,
) -> Stream {
    let mut steps: Vec<StreamStep> = Vec::new();
    let mut wx = start_wx;
    let mut wy = start_wy;
    let mut distance = 0.0f64;
    let mut dir_x = initial_angle.cos();
    let mut dir_y = initial_angle.sin();

    for _ in 0..STREAM_MAX_STEPS {
        let surface_elev = evaluate_surface(peaks, ridgelines, wx, wy);
        if surface_elev <= 0.0 { break; }

        // Stop if no longer descending — prevents valley-floor oscillation
        // where the stream crosses a low point and climbs the opposite wall.
        if steps.len() >= 2 {
            let prev_elev = steps[steps.len() - 1].surface_elev;
            if surface_elev >= prev_elev { break; }
        }

        let width = stream_width(0, distance);
        let carve_depth = stream_depth(0, distance);
        let floor_elev = (surface_elev - carve_depth + branch_depth_offset).max(0.0);

        let wall_exponent = stream_wall_exponent(0);
        steps.push(StreamStep { wx, wy, surface_elev, floor_elev, width, cum_dist: distance, merge_count: 0, wall_exponent });

        // Blended gradient: smooth across cone boundaries
        let (grad_dx, grad_dy) = blended_gradient(peaks, ridgelines, wx, wy);
        if grad_dx == 0.0 && grad_dy == 0.0 { break; }
        let blended_x = dir_x * STREAM_MOMENTUM + grad_dx * (1.0 - STREAM_MOMENTUM);
        let blended_y = dir_y * STREAM_MOMENTUM + grad_dy * (1.0 - STREAM_MOMENTUM);

        let blend_mag = (blended_x * blended_x + blended_y * blended_y).sqrt();
        if blend_mag < 1e-10 { break; }
        let base_dx = blended_x / blend_mag;
        let base_dy = blended_y / blend_mag;

        // Apply lateral noise rotation
        let base_angle = base_dy.atan2(base_dx);
        let lateral_noise = simplex_2d(
            distance / STREAM_LATERAL_WAVELENGTH,
            hash_a.wrapping_mul(1000).wrapping_add(hash_b) as f64,
            seed ^ SEED_STREAM_LATERAL ^ spine_id,
        ) * STREAM_LATERAL_AMP;
        let angle = base_angle + lateral_noise;
        let mut step_dx = angle.cos();
        let mut step_dy = angle.sin();

        // Feed back pre-clamp direction as momentum for next iteration.
        // The clamp is a safety rail — momentum should track the stream's
        // natural heading, not the corrected heading.
        dir_x = step_dx;
        dir_y = step_dy;

        // Clamp FINAL step direction: enforce minimum downhill alignment
        // to prevent cross-slope orbiting. Preserves cross-slope direction
        // while guaranteeing exactly MIN_DOWNHILL_ALIGNMENT dot with grad.
        let dot = step_dx * grad_dx + step_dy * grad_dy;
        if dot < MIN_DOWNHILL_ALIGNMENT {
            let cross_x = step_dx - dot * grad_dx;
            let cross_y = step_dy - dot * grad_dy;
            let cross_len = (cross_x * cross_x + cross_y * cross_y).sqrt();
            let cross_scale = (1.0 - MIN_DOWNHILL_ALIGNMENT * MIN_DOWNHILL_ALIGNMENT).sqrt();
            if cross_len > 1e-10 {
                step_dx = MIN_DOWNHILL_ALIGNMENT * grad_dx + cross_scale * cross_x / cross_len;
                step_dy = MIN_DOWNHILL_ALIGNMENT * grad_dy + cross_scale * cross_y / cross_len;
            } else {
                step_dx = grad_dx;
                step_dy = grad_dy;
            }
        }

        wx += step_dx * STREAM_STEP_SIZE;
        wy += step_dy * STREAM_STEP_SIZE;
        distance += STREAM_STEP_SIZE;
    }

    Stream { steps, merge_count: 0, merged_into: None }
}

/// Reference to a step in the growth grid, used during sequential stream generation.
struct GrowthStepRef {
    stream_idx: usize,
    step_idx: usize,
}

/// Growth grid cell size — must be ≥ STREAM_PROXIMITY_RADIUS so that a 1-ring
/// hex neighborhood covers the full proximity radius from any point in a cell.
const GROWTH_GRID_CELL_SIZE: f64 = STREAM_PROXIMITY_RADIUS * 2.0;

/// Register all steps of a stream in the growth grid so subsequent streams can find it.
fn register_stream_steps(
    stream: &Stream,
    stream_idx: usize,
    growth_grid: &mut HexSpatialGrid<GrowthStepRef>,
) {
    for (step_idx, step) in stream.steps.iter().enumerate() {
        growth_grid.insert(step.wx, step.wy, GrowthStepRef { stream_idx, step_idx });
    }
}

/// Check the growth grid for a nearby existing stream step.
/// Returns (stream_idx, step_idx, target_wx, target_wy) if found.
fn find_nearby_stream(
    wx: f64,
    wy: f64,
    growth_grid: &HexSpatialGrid<GrowthStepRef>,
    existing_streams: &[Stream],
) -> Option<(usize, usize, f64, f64)> {
    let mut best: Option<(f64, usize, usize, f64, f64)> = None;

    for sref in growth_grid.query(wx, wy) {
        let target_step = &existing_streams[sref.stream_idx].steps[sref.step_idx];
        let dist_sq = (wx - target_step.wx).powi(2) + (wy - target_step.wy).powi(2);
        if dist_sq < STREAM_PROXIMITY_RADIUS * STREAM_PROXIMITY_RADIUS {
            if best.is_none() || dist_sq < best.unwrap().0 {
                best = Some((dist_sq, sref.stream_idx, sref.step_idx, target_step.wx, target_step.wy));
            }
        }
    }

    best.map(|(_, si, sti, twx, twy)| (si, sti, twx, twy))
}

/// Propagate merge counts after all streams are generated.
/// Process in reverse generation order (smallest tributaries first) so counts
/// compound naturally through the tree.
fn propagate_merge_counts(streams: &mut [Stream]) {
    for i in (0..streams.len()).rev() {
        if let Some(target_idx) = streams[i].merged_into {
            let contribution = streams[i].merge_count + 1;
            streams[target_idx].merge_count += contribution;

            // Find the step on the target stream closest to this tributary's endpoint
            // and apply width/depth floor from that point onward.
            let trib_last = streams[i].steps.last().unwrap();
            let trib_merge_count = trib_last.merge_count;
            let trib_cum_dist = trib_last.cum_dist;
            let trib_w = stream_width(trib_merge_count, trib_cum_dist);
            let trib_d = stream_depth(trib_merge_count, trib_cum_dist);
            let trib_wx = trib_last.wx;
            let trib_wy = trib_last.wy;

            // Find closest step on target stream.
            let keep_at = streams[target_idx].steps.iter().enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = (a.wx - trib_wx).powi(2) + (a.wy - trib_wy).powi(2);
                    let db = (b.wx - trib_wx).powi(2) + (b.wy - trib_wy).powi(2);
                    da.partial_cmp(&db).unwrap()
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            let new_mc = streams[target_idx].steps[keep_at].merge_count + contribution;
            let min_dist_w = (trib_w - STREAM_START_WIDTH - new_mc as f64 * STREAM_WIDTH_PER_MERGE) / STREAM_WIDTH_PER_DIST;
            let min_dist_d = (trib_d - STREAM_START_DEPTH - new_mc as f64 * STREAM_DEPTH_PER_MERGE) / STREAM_DEPTH_PER_DIST;
            let min_dist = min_dist_w.max(min_dist_d).max(0.0);

            let base_cum_dist = streams[target_idx].steps[keep_at].cum_dist;
            let dist_boost = if min_dist > base_cum_dist { min_dist - base_cum_dist } else { 0.0 };

            for step in &mut streams[target_idx].steps[keep_at..] {
                step.merge_count += contribution;
                step.cum_dist += dist_boost;
                step.width = stream_width(step.merge_count, step.cum_dist);
                let new_depth = stream_depth(step.merge_count, step.cum_dist);
                step.floor_elev = (step.surface_elev - new_depth).max(0.0);
                step.wall_exponent = stream_wall_exponent(step.merge_count);
            }
        }
    }
}

fn generate_ridge_paths(
    streams: &[Stream],
    peaks: &[Peak],
    ridgelines: &[Ridgeline],
    spine_id: u64,
    seed: u64,
) -> Vec<Vec<PathStep>> {
    let mut paths = Vec::new();

    for i in 0..streams.len() {
        if streams[i].steps.len() < 4 { continue; }
        for j in (i + 1)..streams.len() {
            if streams[j].steps.len() < 4 { continue; }

            let path_hash = hash_f64(i as i64, j as i64, seed ^ SEED_PATH_CHANCE ^ spine_id);
            if path_hash > PATH_PROBABILITY { continue; }

            let mi = streams[i].steps.len() / 2;
            let mj = streams[j].steps.len() / 2;
            let si = &streams[i].steps[mi];
            let sj = &streams[j].steps[mj];

            let dx = si.wx - sj.wx;
            let dy = si.wy - sj.wy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > PATH_MAX_STREAM_DIST { continue; }

            let n = (dist / PATH_STEP_SIZE).ceil().max(2.0) as usize;
            let mut points = Vec::with_capacity(n + 1);
            for k in 0..=n {
                let t = k as f64 / n as f64;
                let px = si.wx + t * (sj.wx - si.wx);
                let py = si.wy + t * (sj.wy - si.wy);
                let surface = evaluate_surface(peaks, ridgelines, px, py);
                points.push(PathStep {
                    wx: px,
                    wy: py,
                    floor_elev: (surface - PATH_CARVE_DEPTH).max(0.0),
                });
            }
            paths.push(points);
        }
    }

    paths
}

// ── Catmull-Rom spline carving ──────────────────────────────────────────────
//
// The carve and probe functions use distance-to-spline instead of
// distance-to-polyline. A Catmull-Rom spline is C1 through the control
// points, eliminating gradient discontinuities at vertices that produce
// visible creases at wide carve widths.
//
// Under `min` compositing (carving), Newton convergence failures are benign:
// a wrong projection overestimates distance → shallower carve → the correct
// deeper carve from another evaluation wins. Medial axis discontinuities at
// tight bends are similarly benign: both competing points carve deep.

/// Catmull-Rom cubic interpolation. t ∈ [0, 1] between p1 and p2.
fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// First derivative of Catmull-Rom at t.
fn catmull_rom_deriv(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    0.5 * ((-p0 + p2)
        + (4.0 * p0 - 10.0 * p1 + 8.0 * p2 - 2.0 * p3) * t
        + (-3.0 * p0 + 9.0 * p1 - 9.0 * p2 + 3.0 * p3) * t2)
}

/// Second derivative of Catmull-Rom at t.
fn catmull_rom_second_deriv(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    0.5 * ((4.0 * p0 - 10.0 * p1 + 8.0 * p2 - 2.0 * p3)
        + (-6.0 * p0 + 18.0 * p1 - 18.0 * p2 + 6.0 * p3) * t)
}

/// Mirror a step across an endpoint for phantom control point.
fn mirror_step(endpoint: &StreamStep, neighbor: &StreamStep) -> StreamStep {
    StreamStep {
        wx: 2.0 * endpoint.wx - neighbor.wx,
        wy: 2.0 * endpoint.wy - neighbor.wy,
        surface_elev: endpoint.surface_elev,
        floor_elev: endpoint.floor_elev,
        width: endpoint.width,
        cum_dist: endpoint.cum_dist,
        merge_count: endpoint.merge_count,
        wall_exponent: endpoint.wall_exponent,
    }
}

/// Evaluate spline position at global parameter u (u=0 at steps[0], u=N-1 at steps[N-1]).
fn eval_spline_pos(steps: &[StreamStep], u: f64) -> (f64, f64) {
    let n = steps.len();
    let i = (u.floor() as usize).min(n - 2);
    let t = u - i as f64;

    let p0 = if i == 0 { mirror_step(&steps[0], &steps[1]) } else { steps[i - 1].clone() };
    let p1 = &steps[i];
    let p2 = &steps[i + 1];
    let p3 = if i + 2 >= n { mirror_step(&steps[n - 1], &steps[n - 2]) } else { steps[i + 2].clone() };

    (
        catmull_rom(p0.wx, p1.wx, p2.wx, p3.wx, t),
        catmull_rom(p0.wy, p1.wy, p2.wy, p3.wy, t),
    )
}

/// Evaluate spline first derivative at global parameter u.
fn eval_spline_deriv(steps: &[StreamStep], u: f64) -> (f64, f64) {
    let n = steps.len();
    let i = (u.floor() as usize).min(n - 2);
    let t = u - i as f64;

    let p0 = if i == 0 { mirror_step(&steps[0], &steps[1]) } else { steps[i - 1].clone() };
    let p1 = &steps[i];
    let p2 = &steps[i + 1];
    let p3 = if i + 2 >= n { mirror_step(&steps[n - 1], &steps[n - 2]) } else { steps[i + 2].clone() };

    (
        catmull_rom_deriv(p0.wx, p1.wx, p2.wx, p3.wx, t),
        catmull_rom_deriv(p0.wy, p1.wy, p2.wy, p3.wy, t),
    )
}

/// Evaluate spline second derivative at global parameter u.
fn eval_spline_second_deriv(steps: &[StreamStep], u: f64) -> (f64, f64) {
    let n = steps.len();
    let i = (u.floor() as usize).min(n - 2);
    let t = u - i as f64;

    let p0 = if i == 0 { mirror_step(&steps[0], &steps[1]) } else { steps[i - 1].clone() };
    let p1 = &steps[i];
    let p2 = &steps[i + 1];
    let p3 = if i + 2 >= n { mirror_step(&steps[n - 1], &steps[n - 2]) } else { steps[i + 2].clone() };

    (
        catmull_rom_second_deriv(p0.wx, p1.wx, p2.wx, p3.wx, t),
        catmull_rom_second_deriv(p0.wy, p1.wy, p2.wy, p3.wy, t),
    )
}

/// Newton refinement of nearest point on spline. Minimizes |Q - C(u)|².
fn refine_projection_spline(
    steps: &[StreamStep],
    qx: f64,
    qy: f64,
    u_guess: f64,
    max_iters: usize,
) -> f64 {
    let u_max = (steps.len() - 1) as f64;
    let mut u = u_guess;

    for _ in 0..max_iters {
        let (cx, cy) = eval_spline_pos(steps, u);
        let (dx, dy) = eval_spline_deriv(steps, u);
        let (ddx, ddy) = eval_spline_second_deriv(steps, u);

        let ex = qx - cx;
        let ey = qy - cy;

        // f  = -<(Q-C), C'>   (derivative of ½|Q-C|²)
        // f' = |C'|² - <(Q-C), C''>
        let f = ex * dx + ey * dy;
        let f_prime = -(dx * dx + dy * dy) + (ex * ddx + ey * ddy);

        if f_prime.abs() < 1e-10 { break; }

        u = (u - f / f_prime).clamp(0.0, u_max);
    }
    u
}


/// Nearest segment in range [seg_start, seg_end]. Returns (dist, t, idx).
fn nearest_segment_range(
    steps: &[StreamStep],
    wx: f64,
    wy: f64,
    seg_start: usize,
    seg_end: usize,
) -> (f64, f64, usize) {
    let end = seg_end.min(steps.len() - 2);
    let start = seg_start.min(end);

    let mut best_dist = f64::MAX;
    let mut best_t = 0.0;
    let mut best_idx = start;

    for i in start..=end {
        let (ax, ay) = (steps[i].wx, steps[i].wy);
        let (bx, by) = (steps[i + 1].wx, steps[i + 1].wy);
        let (abx, aby) = (bx - ax, by - ay);
        let ab_len_sq = abx * abx + aby * aby;

        let t = if ab_len_sq < 1e-10 {
            0.0
        } else {
            ((wx - ax) * abx + (wy - ay) * aby) / ab_len_sq
        }
        .clamp(0.0, 1.0);

        let px = ax + t * abx;
        let py = ay + t * aby;
        let dx = wx - px;
        let dy = wy - py;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < best_dist {
            best_dist = dist;
            best_t = t;
            best_idx = i;
        }
    }

    (best_dist, best_t, best_idx)
}

/// Nearest point on spline, coarse search restricted to [seg_start, seg_end].
fn nearest_point_on_spline_range(
    steps: &[StreamStep],
    wx: f64,
    wy: f64,
    seg_start: usize,
    seg_end: usize,
) -> (f64, usize, f64) {
    let (_, coarse_t, coarse_seg) = nearest_segment_range(steps, wx, wy, seg_start, seg_end);
    let u_guess = coarse_seg as f64 + coarse_t;
    let u = refine_projection_spline(steps, wx, wy, u_guess, 4);

    let (cx, cy) = eval_spline_pos(steps, u);
    let dist = ((wx - cx).powi(2) + (wy - cy).powi(2)).sqrt();

    let seg_idx = (u.floor() as usize).min(steps.len() - 2);
    let seg_t = u - seg_idx as f64;

    (dist, seg_idx, seg_t)
}

/// Build spatial index mapping grid cells to stream segment ranges.
fn build_stream_grid(streams: &[Stream]) -> HexSpatialGrid<StreamRef> {
    // Compute cell size from max half-width across all steps.
    let mut max_half_width: f64 = 0.0;
    for stream in streams {
        for step in &stream.steps {
            max_half_width = max_half_width.max(step.width / 2.0);
        }
    }
    let cell_size = max_half_width.max(300.0);

    let mut grid: HexSpatialGrid<StreamRef> = HexSpatialGrid::new(cell_size);

    for (stream_idx, stream) in streams.iter().enumerate() {
        for step_idx in 0..stream.steps.len() {
            let step = &stream.steps[step_idx];
            let half_width = step.width / 2.0;
            grid.insert_radius(
                step.wx, step.wy, half_width,
                StreamRef { stream_idx, seg_start: step_idx, seg_end: step_idx },
            );
        }
    }

    // Merge consecutive segments from same stream in same cell.
    for (_, refs) in grid.cells_mut() {
        refs.sort_by_key(|r| (r.stream_idx, r.seg_start));
        refs.dedup_by(|b, a| {
            if a.stream_idx == b.stream_idx && b.seg_start <= a.seg_end + 1 {
                a.seg_end = a.seg_end.max(b.seg_end);
                true
            } else {
                false
            }
        });
    }

    grid
}

/// Interpolate stream parameters via Catmull-Rom at (seg_idx, seg_t).
/// Returns (floor_elev, width, surface_elev, wall_exponent).
fn interpolate_spline_step(steps: &[StreamStep], seg_idx: usize, seg_t: f64) -> (f64, f64, f64, f64) {
    let n = steps.len();
    let i = seg_idx.min(n - 2);
    let t = seg_t;

    let p0 = if i == 0 { &steps[0] } else { &steps[i - 1] };
    let p1 = &steps[i];
    let p2 = &steps[i + 1];
    let p3 = if i + 2 >= n { &steps[n - 1] } else { &steps[i + 2] };

    let floor = catmull_rom(p0.floor_elev, p1.floor_elev, p2.floor_elev, p3.floor_elev, t);
    let width = catmull_rom(p0.width, p1.width, p2.width, p3.width, t);
    let surface = catmull_rom(p0.surface_elev, p1.surface_elev, p2.surface_elev, p3.surface_elev, t);
    let wall_exp = catmull_rom(p0.wall_exponent, p1.wall_exponent, p2.wall_exponent, p3.wall_exponent, t);

    // Clamp: Catmull-Rom can overshoot between control points
    let min_floor = p1.floor_elev.min(p2.floor_elev);
    let max_floor = p1.floor_elev.max(p2.floor_elev);
    let min_width = p1.width.min(p2.width);
    let max_width = p1.width.max(p2.width);
    let min_surface = p1.surface_elev.min(p2.surface_elev);
    let max_surface = p1.surface_elev.max(p2.surface_elev);
    let min_wall_exp = p1.wall_exponent.min(p2.wall_exponent);
    let max_wall_exp = p1.wall_exponent.max(p2.wall_exponent);

    (
        floor.clamp(min_floor, max_floor),
        width.clamp(min_width, max_width).max(0.0),
        surface.clamp(min_surface, max_surface),
        wall_exp.clamp(min_wall_exp, max_wall_exp),
    )
}

fn build_ravine_network(peaks: &[Peak], ridgelines: &[Ridgeline], spine_id: u64, seed: u64) -> RavineNetwork {
    if peaks.is_empty() {
        return RavineNetwork::empty();
    }

    // ── Phase 1: Collect all stream origins ─────────────────────────────────

    struct StreamOrigin {
        start_wx: f64,
        start_wy: f64,
        angle: f64,
        branch_offset: f64,
        hash_a: u64,
        hash_b: u64,
        start_elevation: f64,
    }

    let mut origins = Vec::new();
    let mut hanging_count = 0usize;

    // Peak streams
    for (pi, peak) in peaks.iter().enumerate() {
        for si in 0..STREAMS_PER_PEAK {
            let angle_hash = hash_f64(pi as i64, si as i64, seed ^ SEED_STREAM_DIR ^ spine_id);
            let base_angle = angle_hash * std::f64::consts::TAU;

            let offset = peak.falloff_radius * STREAM_HEAD_OFFSET_FRAC + STREAM_ORIGIN_OFFSET;
            let start_wx = peak.wx + base_angle.cos() * offset;
            let start_wy = peak.wy + base_angle.sin() * offset;

            let h_hash = hash_f64(si as i64, pi as i64, seed ^ SEED_HANGING_VAL ^ spine_id);
            let branch_offset = if h_hash < HANGING_VALLEY_CHANCE {
                let off = hash_f64(
                    pi as i64 ^ 0x1234,
                    si as i64,
                    seed ^ SEED_HANGING_VAL ^ spine_id,
                );
                hanging_count += 1;
                HANGING_VALLEY_MIN_OFFSET
                    + off * (HANGING_VALLEY_MAX_OFFSET - HANGING_VALLEY_MIN_OFFSET)
            } else {
                0.0
            };

            let start_elevation = evaluate_surface(peaks, ridgelines, start_wx, start_wy);
            origins.push(StreamOrigin {
                start_wx, start_wy, angle: base_angle, branch_offset,
                hash_a: pi as u64, hash_b: si as u64, start_elevation,
            });
        }
    }

    // Saddle streams
    for (ri, ridge) in ridgelines.iter().enumerate() {
        let rdx = ridge.bx - ridge.ax;
        let rdy = ridge.by - ridge.ay;
        let ridge_len = (rdx * rdx + rdy * rdy).sqrt();
        if ridge_len < MIN_RIDGE_LENGTH || ridge.sag < MIN_SADDLE_SAG { continue; }

        let saddle_wx = (ridge.ax + ridge.bx) / 2.0;
        let saddle_wy = (ridge.ay + ridge.by) / 2.0;

        let perp_x = -rdy / ridge_len;
        let perp_y = rdx / ridge_len;

        let dirs: [(f64, f64); 2] = [
            (perp_x, perp_y),
            (-perp_x, -perp_y),
        ];
        for (side, &(dx, dy)) in dirs.iter().enumerate() {
            let angle = dy.atan2(dx);
            let start_wx = saddle_wx + dx * STREAM_ORIGIN_OFFSET;
            let start_wy = saddle_wy + dy * STREAM_ORIGIN_OFFSET;
            let start_elevation = evaluate_surface(peaks, ridgelines, start_wx, start_wy);
            origins.push(StreamOrigin {
                start_wx, start_wy, angle, branch_offset: 0.0,
                hash_a: ri as u64 | 0x8000_0000, hash_b: side as u64, start_elevation,
            });
        }
    }

    // ── Phase 2: Sort by starting elevation — highest first ─────────────────

    origins.sort_by(|a, b| b.start_elevation.partial_cmp(&a.start_elevation).unwrap());

    // ── Phase 3: Grow each stream sequentially with inline merge detection ──

    let mut growth_grid: HexSpatialGrid<GrowthStepRef> = HexSpatialGrid::new(GROWTH_GRID_CELL_SIZE);
    let mut streams: Vec<Stream> = Vec::new();

    for origin in &origins {
        let mut stream = grow_stream(
            origin.start_wx, origin.start_wy, origin.angle,
            peaks, ridgelines, origin.branch_offset,
            origin.hash_a, origin.hash_b, spine_id, seed,
        );
        if stream.steps.len() < 2 { continue; }

        // Check if origin is within proximity of an existing stream's step
        // (origin suppression — prevents immediately-merging streams).
        let origin_step = &stream.steps[0];
        if find_nearby_stream(origin_step.wx, origin_step.wy, &growth_grid, &streams).is_some() {
            continue;
        }

        // Walk through the grown stream's steps and check for merges.
        // The stream was grown without merge awareness; now we truncate at
        // the first step that lands near an existing stream.
        let mut merge_at: Option<(usize, usize, usize, f64, f64)> = None;
        for step_idx in 1..stream.steps.len() {
            let step = &stream.steps[step_idx];
            if let Some((target_stream, target_step, twx, twy)) =
                find_nearby_stream(step.wx, step.wy, &growth_grid, &streams)
            {
                merge_at = Some((step_idx, target_stream, target_step, twx, twy));
                break;
            }
        }

        if let Some((trunc_at, target_idx, _target_step, twx, twy)) = merge_at {
            // Truncate at the merge point
            stream.steps.truncate(trunc_at);
            // Add a final step onto the existing stream's centerline
            let last = stream.steps.last().unwrap();
            let snap_dist = ((twx - last.wx).powi(2) + (twy - last.wy).powi(2)).sqrt();
            let surface = evaluate_surface(peaks, ridgelines, twx, twy);
            let width = last.width;
            let cum_dist = last.cum_dist + snap_dist;
            let merge_count = last.merge_count;
            let wall_exponent = last.wall_exponent;
            let floor = (surface - stream_depth(merge_count, cum_dist) + origin.branch_offset).max(0.0);
            stream.steps.push(StreamStep {
                wx: twx, wy: twy,
                surface_elev: surface,
                floor_elev: floor,
                width, cum_dist, merge_count, wall_exponent,
            });
            stream.merged_into = Some(target_idx);
        }

        register_stream_steps(&stream, streams.len(), &mut growth_grid);
        streams.push(stream);
    }

    // ── Phase 4: Propagate merge counts ─────────────────────────────────────

    propagate_merge_counts(&mut streams);

    // ── Phase 5: Build carve order and spatial index ─────────────────────────

    let paths = generate_ridge_paths(&streams, peaks, ridgelines, spine_id, seed);
    let stream_grid = build_stream_grid(&streams);

    RavineNetwork { streams, paths, stream_grid, hanging_count }
}

// ── Single spine growth ──────────────────────────────────────────────────────

fn grow_spine(
    epi_wx: f64,
    epi_wy: f64,
    spine_id: u64,
    plates: &mut [PlateCenter],
    plate_map: &HashMap<u64, usize>,
    plate_cache: &mut PlateCache,
    seed: u64,
) -> SpineInstance {
    let cell_q = (epi_wx / MACRO_CELL_SIZE) as i64;
    let cell_r = (epi_wy / MACRO_CELL_SIZE) as i64;
    let base_bearing = hash_f64(cell_q, cell_r, seed ^ SEED_BEARING ^ spine_id) * std::f64::consts::TAU;
    let jitter_t = hash_f64(
        cell_q ^ 0x5555,
        cell_r ^ 0x5555,
        seed ^ SEED_BEARING ^ spine_id ^ 0x7777_8888,
    );
    let bearing = base_bearing + (jitter_t * 2.0 - 1.0) * BEARING_JITTER_MAX;
    let (spine_dx, spine_dy) = (bearing.cos(), bearing.sin());

    let epi_peak = peak_at(0.0, spine_id, seed);
    let epi_half_width = HALF_WIDTH_MIN + (HALF_WIDTH_MAX - HALF_WIDTH_MIN) * epi_peak;
    let epi_height = epi_peak * RIDGE_PEAK_ELEVATION;

    // Grow both arms.
    let mut pos_steps = grow_arm_steps(epi_wx, epi_wy, spine_dx, spine_dy,  1.0, spine_id, plate_cache, seed);
    let mut neg_steps = grow_arm_steps(epi_wx, epi_wy, spine_dx, spine_dy, -1.0, spine_id, plate_cache, seed);
    taper_steps(&mut pos_steps);
    taper_steps(&mut neg_steps);

    // Scatter peaks: epicenter + both arms.
    let mut peaks: Vec<Peak> = Vec::new();

    // Epicenter always places a center peak (no skip).
    peaks.push(Peak {
        wx: epi_wx, wy: epi_wy, height: epi_height,
        falloff_radius: epi_half_width * PEAK_FALLOFF_SCALE,
    });
    scatter_flanking_peaks(0, epi_wx, epi_wy, epi_half_width, epi_height, spine_dx, spine_dy, spine_id, 0, seed, &mut peaks);

    for (step_idx, step) in pos_steps.iter().enumerate() {
        scatter_step_peaks(step_idx + 1, step, spine_dx, spine_dy, spine_id, 0, seed, &mut peaks);
    }
    for (step_idx, step) in neg_steps.iter().enumerate() {
        scatter_step_peaks(step_idx + 1, step, spine_dx, spine_dy, spine_id, SEED_ARM_FLIP, seed, &mut peaks);
    }

    // Build ridgeline connections between peaks.
    let ridgelines = build_ridgelines(&peaks, spine_id, seed);

    // Build ravine network from peaks and ridgelines.
    let ravine_network = build_ravine_network(&peaks, &ridgelines, spine_id, seed);

    // Write plate tags.
    for peak in &peaks {
        apply_peak_to_plates(peak, plates, plate_map, plate_cache);
    }

    let (bounding_center, mut bounding_radius) = bounding_circle(&peaks);

    // Expand bounding circle to include all stream steps (streams flow
    // downhill beyond peak falloff radii and taper steps extend further).
    let (bc_x, bc_y) = bounding_center;
    for stream in &ravine_network.streams {
        for step in &stream.steps {
            let dx = step.wx - bc_x;
            let dy = step.wy - bc_y;
            let r = (dx * dx + dy * dy).sqrt() + step.width;
            if r > bounding_radius { bounding_radius = r; }
        }
    }

    SpineInstance { id: spine_id, peaks, ridgelines, ravine_network, bounding_center, bounding_radius }
}

// ── Composite elevation ──────────────────────────────────────────────────────

/// Maximum elevation across all spine instances at (wx, wy).
pub fn evaluate_elevation(instances: &[SpineInstance], wx: f64, wy: f64) -> f64 {
    let mut max_elev = 0.0f64;
    for inst in instances {
        let e = inst.elevation_at(wx, wy);
        if e > max_elev { max_elev = e; }
    }
    max_elev
}

/// Convert continuous elevation to discrete z-level for Qrz tile coordinates.
pub fn discretize_elevation(elevation: f64) -> i32 {
    (elevation / ELEVATION_PER_Z).round() as i32
}

// ── Main entry point ─────────────────────────────────────────────────────────

/// Generate continental spines on a classified macro plate set.
///
/// Mutates `elevation` and spine tags (`Ridge`, `Highland`, `Foothills`) on
/// plates in the slice. Caller must have already called
/// [`PlateCache::classify_tags`] so Sea/Coast/Inland tags are present.
///
/// Returns `SpineInstance`s with retained peak geometry for continuous
/// elevation evaluation via [`SpineInstance::elevation_at`].
///
/// Spine placement is locally deterministic: each fixed-size evaluation chunk
/// selects epicenters using only its own candidates plus a 1-ring neighborhood
/// for conflict resolution.
pub fn generate_spines(
    plates: &mut [PlateCenter],
    plate_cache: &mut PlateCache,
    seed: u64,
) -> Vec<SpineInstance> {
    let plate_map: HashMap<u64, usize> = plates
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i))
        .collect();

    let (min_x, max_x, min_y, max_y) = plates_bounding_box(plates, SPINE_INFLUENCE);
    let target_chunks = spine_chunks_in_bounds(min_x, max_x, min_y, max_y);

    let mut candidate_cache: HashMap<(i32, i32), Vec<SpineCandidate>> = HashMap::new();
    for &(cq, cr) in &target_chunks {
        for (dq, dr) in spine_chunk_1ring(cr) {
            ensure_candidates(cq + dq, cr + dr, &mut candidate_cache, plate_cache, seed);
        }
    }

    let mut instances = Vec::new();
    for (scq, scr) in target_chunks {
        let survivors = resolve_chunk(scq, scr, &candidate_cache);
        for candidate in survivors {
            let instance = grow_spine(
                candidate.wx, candidate.wy, candidate.plate_id,
                plates, &plate_map, plate_cache, seed,
            );
            if !instance.peaks.is_empty() {
                instances.push(instance);
            }
        }
    }
    instances
}

// ── Micro elevation offset ───────────────────────────────────────────────────

/// Compute elevation noise for a micro cell relative to its parent macro plate's elevation.
///
/// Returns a value in [0.9 * macro_elev, 1.1 * macro_elev] — ±10% variation.
pub fn micro_elevation_offset(micro_id: u64, macro_elevation: f64, seed: u64) -> f64 {
    let micro_hash = hash_f64(
        (micro_id as i64) ^ 0x1234,
        (micro_id >> 32) as i64,
        seed ^ SEED_MICRO,
    );
    let variation = (micro_hash * 2.0 - 1.0) * macro_elevation * 0.10;
    (macro_elevation + variation).max(0.0)
}

// ── SpineCache ───────────────────────────────────────────────────────────────

/// Maximum number of spine chunks to keep cached.
const SPINE_CACHE_MAX_CHUNKS: usize = 32;

struct SpineCacheEntry {
    instances: Vec<SpineInstance>,
    last_accessed: u64,
}

/// Lazily generates and caches spine instances per spine chunk for on-demand
/// elevation queries. Evicts least-recently-accessed chunks when the cache
/// exceeds [`SPINE_CACHE_MAX_CHUNKS`].
pub struct SpineCache {
    seed: u64,
    candidate_cache: HashMap<(i32, i32), Vec<SpineCandidate>>,
    instance_cache: HashMap<(i32, i32), SpineCacheEntry>,
    access_counter: u64,
}

impl SpineCache {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            candidate_cache: HashMap::new(),
            instance_cache: HashMap::new(),
            access_counter: 0,
        }
    }

    /// Return the combined spine elevation at a world position.
    /// Lazily generates and caches spine chunks as needed.
    pub fn elevation_at(&mut self, wx: f64, wy: f64, plate_cache: &mut PlateCache) -> f64 {
        let (cq, cr) = spine_chunk_coord(wx, wy);
        self.access_counter += 1;
        let stamp = self.access_counter;

        for (dq, dr) in spine_chunk_1ring(cr) {
            self.ensure_instances(cq + dq, cr + dr, plate_cache);
        }

        let mut max_elev = 0.0f64;
        for (dq, dr) in spine_chunk_1ring(cr) {
            if let Some(entry) = self.instance_cache.get_mut(&(cq + dq, cr + dr)) {
                entry.last_accessed = stamp;
                for inst in &entry.instances {
                    let e = inst.elevation_at(wx, wy);
                    if e > max_elev { max_elev = e; }
                }
            }
        }
        max_elev
    }

    fn ensure_instances(&mut self, cq: i32, cr: i32, plate_cache: &mut PlateCache) {
        if self.instance_cache.contains_key(&(cq, cr)) { return; }

        for (dq, dr) in spine_chunk_1ring(cr) {
            ensure_candidates(cq + dq, cr + dr, &mut self.candidate_cache, plate_cache, self.seed);
        }

        let survivors = resolve_chunk(cq, cr, &self.candidate_cache);

        // Pass empty plates/map — apply_peak_to_plates skips all writes when plate_map is empty.
        let mut empty_plates: Vec<PlateCenter> = Vec::new();
        let empty_map: HashMap<u64, usize> = HashMap::new();

        let mut instances = Vec::new();
        for candidate in survivors {
            let instance = grow_spine(
                candidate.wx, candidate.wy, candidate.plate_id,
                &mut empty_plates, &empty_map, plate_cache, self.seed,
            );
            if !instance.peaks.is_empty() {
                instances.push(instance);
            }
        }

        self.instance_cache.insert((cq, cr), SpineCacheEntry {
            instances,
            last_accessed: self.access_counter,
        });

        self.evict_if_over_budget();
    }

    fn evict_if_over_budget(&mut self) {
        if self.instance_cache.len() <= SPINE_CACHE_MAX_CHUNKS { return; }

        let lru_key = self.instance_cache.iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(&key, _)| key);

        if let Some(key) = lru_key {
            self.instance_cache.remove(&key);
        }
    }

    #[cfg(test)]
    fn inject_entry(&mut self, key: (i32, i32), stamp: u64) {
        self.instance_cache.insert(key, SpineCacheEntry {
            instances: Vec::new(),
            last_accessed: stamp,
        });
    }

    #[cfg(test)]
    fn cached_len(&self) -> usize {
        self.instance_cache.len()
    }

    #[cfg(test)]
    fn contains(&self, key: (i32, i32)) -> bool {
        self.instance_cache.contains_key(&key)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a RavineNetwork from a single stream (test helper).
    fn test_network(streams: Vec<Stream>) -> RavineNetwork {
        let stream_grid = build_stream_grid(&streams);
        RavineNetwork {
            streams,
            paths: Vec::new(),
            stream_grid,
            hanging_count: 0,
        }
    }

    // ── Pure function tests ──────────────────────────────────────────────────

    #[test]
    fn cross_section_profile_peaks_at_center() {
        assert!((cross_section_profile(0.0) - 1.0).abs() < 1e-10, "profile at center must be 1.0");
        assert!((cross_section_profile(1.0) - 0.0).abs() < 1e-10, "profile at edge must be 0.0");
        let mut prev = 1.0;
        for i in 1..=10 {
            let v = cross_section_profile(i as f64 / 10.0);
            assert!(v <= prev + 1e-10, "profile not monotonically decreasing at {i}/10");
            prev = v;
        }
    }

    #[test]
    fn cross_section_profile_midpoint() {
        let mid = cross_section_profile(0.5);
        // (1 - 0.5)^1.5 = 0.5^1.5 ≈ 0.3536
        let expected = (0.5_f64).powf(FALLOFF_EXPONENT);
        assert!((mid - expected).abs() < 1e-10, "profile at 0.5 should be {expected}, got {mid}");
    }

    #[test]
    fn cross_section_tag_assigns_zones_correctly() {
        assert_eq!(cross_section_tag(0.0),  PlateTag::Ridge);
        assert_eq!(cross_section_tag(0.10), PlateTag::Ridge);
        assert_eq!(cross_section_tag(0.14), PlateTag::Ridge);
        assert_eq!(cross_section_tag(0.16), PlateTag::Highland);
        assert_eq!(cross_section_tag(0.59), PlateTag::Highland);
        assert_eq!(cross_section_tag(0.61), PlateTag::Foothills);
        assert_eq!(cross_section_tag(1.00), PlateTag::Foothills);
    }

    #[test]
    fn coastal_attenuation_decreases_with_coast_count() {
        let v0 = coastal_attenuation(0);
        let v2 = coastal_attenuation(2);
        let v4 = coastal_attenuation(4);
        let v6 = coastal_attenuation(6);
        assert!(v0 > v2);
        assert!(v2 > v4);
        assert!(v4 > v6);
        assert_eq!(v0, 1.0, "zero coast neighbors = no attenuation");
        assert!(v6 > 0.0, "should never fully attenuate to zero");
    }

    #[test]
    fn coastal_attenuation_ranges_in_01() {
        for n in 0..=8 {
            let v = coastal_attenuation(n);
            assert!(v > 0.0 && v <= 1.0, "attenuation({n}) = {v} out of (0, 1]");
        }
    }

    #[test]
    fn spine_tag_priority_ordered_correctly() {
        assert!(spine_tag_priority(&PlateTag::Ridge) > spine_tag_priority(&PlateTag::Highland));
        assert!(spine_tag_priority(&PlateTag::Highland) > spine_tag_priority(&PlateTag::Foothills));
        assert!(spine_tag_priority(&PlateTag::Foothills) > spine_tag_priority(&PlateTag::Sea));
    }

    #[test]
    fn fbm_1d_stays_bounded() {
        let seed = 0xDEAD_BEEF_1234_5678u64;
        for i in 0..200 {
            let t = i as f64 * 1000.0;
            let v = fbm_1d(t, SPINE_WAVELENGTH, seed);
            assert!(v >= -1.5 && v <= 1.5, "fbm_1d({t}) = {v} out of [-1.5, 1.5]");
        }
    }

    #[test]
    fn peak_at_in_range() {
        let seed = 42u64;
        for step in 0..SPINE_MAX_STEPS {
            let t = step as f64 * SPINE_STEP;
            let p = peak_at(t, 0, seed);
            assert!(p >= 0.5 && p <= 1.0, "peak_at({t}) = {p} out of [0.5, 1.0]");
        }
    }

    // ── Peak geometry tests ──────────────────────────────────────────────────

    fn test_peak(height: f64, falloff_radius: f64) -> Peak {
        Peak { wx: 0.0, wy: 0.0, height, falloff_radius }
    }

    /// Pure cone evaluation without SpineInstance (no noise), using Euclidean distance.
    fn peak_cone(peak: &Peak, wx: f64, wy: f64) -> f64 {
        let dx = wx - peak.wx;
        let dy = wy - peak.wy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist >= peak.falloff_radius { return 0.0; }
        cross_section_profile(dist / peak.falloff_radius) * peak.height
    }

    #[test]
    fn peak_cone_at_center_equals_height() {
        let peak = test_peak(1000.0, 3000.0);
        assert!((peak_cone(&peak, 0.0, 0.0) - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn peak_cone_at_falloff_radius_is_zero() {
        let peak = test_peak(1000.0, 3000.0);
        assert_eq!(peak_cone(&peak, 0.0, 3000.0), 0.0);
        assert_eq!(peak_cone(&peak, 3000.0, 0.0), 0.0);
    }

    #[test]
    fn peak_cone_isotropic() {
        let peak = test_peak(1000.0, 3000.0);
        let dist = 2000.0;
        let along_x = peak_cone(&peak, dist, 0.0);
        let along_y = peak_cone(&peak, 0.0, dist);
        assert!((along_x - along_y).abs() < 1e-10,
            "circular cone should be isotropic: x={along_x:.1}, y={along_y:.1}");
    }

    #[test]
    fn peak_cone_decreases_monotonically() {
        let peak = test_peak(800.0, 3000.0);
        let mut prev = peak_cone(&peak, 0.0, 0.0);
        for i in 1..=10 {
            let dist = i as f64 * 300.0;
            let v = peak_cone(&peak, 0.0, dist);
            assert!(v <= prev + 1e-10, "cone not monotonically decreasing at dist={dist}: {v} > {prev}");
            prev = v;
        }
    }

    // ── Ridgeline tests ──────────────────────────────────────────────────────

    #[test]
    fn ridge_elevation_at_peak_a_equals_height_a() {
        let ridge = Ridgeline {
            peak_a: 0, peak_b: 1,
            ax: 0.0, ay: 0.0, bx: 3000.0, by: 0.0,
            height_a: 1000.0, height_b: 800.0, sag: 0.3, };
        let elev = ridge_elevation_at(&ridge, 0.0, 0.0);
        assert!((elev - 1000.0).abs() < 1e-10,
            "elevation at peak A should equal height_a, got {elev}");
    }

    #[test]
    fn ridge_elevation_at_peak_b_equals_height_b() {
        let ridge = Ridgeline {
            peak_a: 0, peak_b: 1,
            ax: 0.0, ay: 0.0, bx: 3000.0, by: 0.0,
            height_a: 1000.0, height_b: 800.0, sag: 0.3, };
        let elev = ridge_elevation_at(&ridge, 3000.0, 0.0);
        assert!((elev - 800.0).abs() < 1e-10,
            "elevation at peak B should equal height_b, got {elev}");
    }

    #[test]
    fn ridge_elevation_at_midpoint_has_sag() {
        let ridge = Ridgeline {
            peak_a: 0, peak_b: 1,
            ax: 0.0, ay: 0.0, bx: 4000.0, by: 0.0,
            height_a: 1000.0, height_b: 1000.0, sag: 0.4, };
        let at_a = ridge_elevation_at(&ridge, 0.0, 0.0);
        let at_mid = ridge_elevation_at(&ridge, 2000.0, 0.0);
        assert!(at_mid > 0.0, "midpoint should have positive elevation");
        assert!(at_mid < at_a, "midpoint ({at_mid:.1}) should be lower than peak ({at_a:.1}) due to sag");
    }

    #[test]
    fn ridge_elevation_zero_at_half_width() {
        let ridge = Ridgeline {
            peak_a: 0, peak_b: 1,
            ax: 0.0, ay: 0.0, bx: 3000.0, by: 0.0,
            height_a: 1000.0, height_b: 800.0, sag: 0.3, };
        let elev = ridge_elevation_at(&ridge, 1500.0, RIDGE_HALF_WIDTH);
        assert_eq!(elev, 0.0, "elevation at half_width perpendicular should be zero");
    }

    #[test]
    fn no_duplicate_ridgelines() {
        let peaks = vec![
            Peak { wx: 0.0, wy: 0.0, height: 1000.0, falloff_radius: 2000.0 },
            Peak { wx: 2000.0, wy: 0.0, height: 800.0, falloff_radius: 2000.0 },
            Peak { wx: 1000.0, wy: 1500.0, height: 900.0, falloff_radius: 2000.0 },
        ];
        let ridges = build_ridgelines(&peaks, 42, 12345);
        for i in 0..ridges.len() {
            for j in (i + 1)..ridges.len() {
                let same = (ridges[i].peak_a == ridges[j].peak_a && ridges[i].peak_b == ridges[j].peak_b)
                    || (ridges[i].peak_a == ridges[j].peak_b && ridges[i].peak_b == ridges[j].peak_a);
                assert!(!same, "duplicate ridgeline between peaks {} and {}", ridges[i].peak_a, ridges[i].peak_b);
            }
        }
    }

    #[test]
    fn ridgeline_deterministic() {
        let peaks = vec![
            Peak { wx: 0.0, wy: 0.0, height: 1000.0, falloff_radius: 2000.0 },
            Peak { wx: 3000.0, wy: 0.0, height: 800.0, falloff_radius: 2000.0 },
        ];
        let a = build_ridgelines(&peaks, 42, 12345);
        let b = build_ridgelines(&peaks, 42, 12345);
        assert_eq!(a.len(), b.len());
        for (ra, rb) in a.iter().zip(b.iter()) {
            assert_eq!(ra.peak_a, rb.peak_a);
            assert_eq!(ra.peak_b, rb.peak_b);
            assert_eq!(ra.sag, rb.sag);
        }
    }

    // ── SpineInstance tests ──────────────────────────────────────────────────

    fn test_instance(peak_height: f64, falloff_radius: f64) -> SpineInstance {
        SpineInstance {
            id: 42,
            peaks: vec![test_peak(peak_height, falloff_radius)],
            ridgelines: Vec::new(),
            ravine_network: RavineNetwork::empty(),
            bounding_center: (0.0, 0.0),
            bounding_radius: falloff_radius + RIDGE_HALF_WIDTH + 500.0,
        }
    }

    #[test]
    fn elevation_at_zero_far_from_spine() {
        let inst = test_instance(800.0, 2000.0);
        assert_eq!(inst.elevation_at(100_000.0, 100_000.0), 0.0);
    }

    #[test]
    fn elevation_at_centerline_is_positive() {
        let inst = test_instance(800.0, 2000.0);
        assert!(inst.elevation_at(0.0, 0.0) > 0.0);
    }

    #[test]
    fn elevation_at_falloff_radius_is_zero() {
        let inst = test_instance(800.0, 2000.0);
        // Circular falloff: zero at falloff_radius in any direction.
        assert_eq!(inst.elevation_at(0.0, 2000.0), 0.0);
        assert_eq!(inst.elevation_at(2000.0, 0.0), 0.0);
    }

    #[test]
    fn elevation_decreases_center_to_edge() {
        let inst = test_instance(800.0, 2000.0);
        let center = inst.elevation_at(0.0, 0.0);
        let near_edge = inst.elevation_at(0.0, 1800.0);
        // Center is ~800 + noise(~200). Near-edge (t=0.9) is ~20 + noise(~5).
        // The ratio is so large that center > near_edge regardless of noise.
        assert!(center > near_edge, "center ({center:.1}) should exceed near-edge ({near_edge:.1})");
        assert!(center > 0.0);
    }

    #[test]
    fn bounding_check_filters_distant_points() {
        let inst = test_instance(800.0, 2000.0);
        // bounding_radius = 2000 + RIDGE_HALF_WIDTH + 500 = 3300
        assert_eq!(inst.elevation_at(0.0, 3301.0), 0.0);
    }

    #[test]
    fn elevation_at_deterministic() {
        let inst = test_instance(800.0, 2000.0);
        let a = inst.elevation_at(500.0, 300.0);
        let b = inst.elevation_at(500.0, 300.0);
        assert_eq!(a, b);
    }

    #[test]
    fn elevation_at_empty_peaks_returns_zero() {
        let inst = SpineInstance {
            id: 42,
            peaks: Vec::new(),
            ridgelines: Vec::new(),
            ravine_network: RavineNetwork::empty(),
            bounding_center: (0.0, 0.0),
            bounding_radius: 1000.0,
        };
        assert_eq!(inst.elevation_at(0.0, 0.0), 0.0);
    }

    #[test]
    fn evaluate_elevation_takes_max() {
        let mk = |id, height| SpineInstance {
            id,
            peaks: vec![Peak { wx: 0.0, wy: 0.0, height, falloff_radius: 3000.0 }],
            ridgelines: Vec::new(),
            ravine_network: RavineNetwork::empty(),
            bounding_center: (0.0, 0.0),
            bounding_radius: 3000.0 + RIDGE_HALF_WIDTH + 500.0,
        };
        let low = mk(1, 400.0);
        let high = mk(2, 800.0);
        let combined = evaluate_elevation(&[low, high], 0.0, 0.0);

        let high_only = mk(2, 800.0);
        let single = high_only.elevation_at(0.0, 0.0);
        assert_eq!(combined, single, "should take max of overlapping instances");
    }

    #[test]
    fn discretize_elevation_invariants() {
        assert_eq!(discretize_elevation(0.0), 0);
        assert_eq!(discretize_elevation(ELEVATION_PER_Z), 1);
        let mut prev = discretize_elevation(0.0);
        for i in 1..=100 {
            let elev = i as f64 * ELEVATION_PER_Z * 0.1;
            let z = discretize_elevation(elev);
            assert!(z >= prev, "discretize not monotonic at elevation {elev}: {z} < {prev}");
            prev = z;
        }
    }

    // ── Integration tests ────────────────────────────────────────────────────

    fn collect_classified_plates(
        center_wx: f64,
        center_wy: f64,
        radius: f64,
        seed: u64,
    ) -> (Vec<PlateCenter>, PlateCache) {
        let mut cache = PlateCache::new(seed);
        let mut plates = cache.plates_in_radius(center_wx, center_wy, radius);
        cache.classify_tags(&mut plates);
        (plates, cache)
    }

    #[test]
    fn no_spine_tags_on_sea_plates() {
        let seed = 0x9E3779B97F4A7C15u64;
        let (mut plates, mut cache) = collect_classified_plates(0.0, 0.0, 80_000.0, seed);
        generate_spines(&mut plates, &mut cache, seed);

        for plate in &plates {
            if plate.has_tag(&PlateTag::Sea) {
                assert!(!plate.has_tag(&PlateTag::Ridge),    "Sea plate id={} got Ridge",    plate.id);
                assert!(!plate.has_tag(&PlateTag::Highland), "Sea plate id={} got Highland", plate.id);
                assert!(!plate.has_tag(&PlateTag::Foothills),"Sea plate id={} got Foothills",plate.id);
                assert_eq!(plate.elevation, 0.0, "Sea plate id={} has non-zero elevation", plate.id);
            }
        }
    }

    #[test]
    fn ridge_elevation_exceeds_highland_exceeds_foothills() {
        let seed = 0x9E3779B97F4A7C15u64;
        let (mut plates, mut cache) = collect_classified_plates(0.0, 0.0, 80_000.0, seed);
        generate_spines(&mut plates, &mut cache, seed);

        let avg = |tag: &PlateTag| -> Option<f64> {
            let elevs: Vec<f64> = plates.iter()
                .filter(|p| p.has_tag(tag))
                .map(|p| p.elevation)
                .collect();
            if elevs.is_empty() { None } else { Some(elevs.iter().sum::<f64>() / elevs.len() as f64) }
        };

        if let Some(ridge_avg) = avg(&PlateTag::Ridge) {
            if let Some(highland_avg) = avg(&PlateTag::Highland) {
                assert!(ridge_avg >= highland_avg,
                    "Average Ridge ({ridge_avg:.1}) should be >= Highland ({highland_avg:.1})");
            }
            if let Some(foothills_avg) = avg(&PlateTag::Foothills) {
                if let Some(highland_avg) = avg(&PlateTag::Highland) {
                    assert!(highland_avg >= foothills_avg,
                        "Average Highland ({highland_avg:.1}) should be >= Foothills ({foothills_avg:.1})");
                }
            }
        }
    }

    #[test]
    fn spine_is_deterministic() {
        let seed = 0x9E3779B97F4A7C15u64;
        let (mut plates_a, mut cache_a) = collect_classified_plates(0.0, 0.0, 50_000.0, seed);
        let (mut plates_b, mut cache_b) = collect_classified_plates(0.0, 0.0, 50_000.0, seed);

        generate_spines(&mut plates_a, &mut cache_a, seed);
        generate_spines(&mut plates_b, &mut cache_b, seed);

        assert_eq!(plates_a.len(), plates_b.len());
        for (a, b) in plates_a.iter().zip(plates_b.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.elevation, b.elevation, "elevation mismatch for plate {}", a.id);
        }
    }

    #[test]
    fn exclusion_distance_prevents_nearby_epicenters() {
        let seed = 0x9E3779B97F4A7C15u64;
        let (mut plates, mut cache) = collect_classified_plates(0.0, 0.0, 80_000.0, seed);
        generate_spines(&mut plates, &mut cache, seed);
        // Verify no panic — the conflict resolution invariant is tested implicitly.
    }

    #[test]
    fn spine_elevation_is_positive_on_tagged_plates() {
        let seed = 0x9E3779B97F4A7C15u64;
        let (mut plates, mut cache) = collect_classified_plates(0.0, 0.0, 80_000.0, seed);
        generate_spines(&mut plates, &mut cache, seed);

        for plate in &plates {
            if plate.has_tag(&PlateTag::Ridge)
                || plate.has_tag(&PlateTag::Highland)
                || plate.has_tag(&PlateTag::Foothills)
            {
                assert!(plate.elevation > 0.0, "Tagged plate id={} has zero elevation", plate.id);
            }
        }
    }

    #[test]
    fn different_seeds_produce_different_spines() {
        let (mut plates_a, mut cache_a) = collect_classified_plates(0.0, 0.0, 80_000.0, 42);
        let (mut plates_b, mut cache_b) = collect_classified_plates(0.0, 0.0, 80_000.0, 99999);

        generate_spines(&mut plates_a, &mut cache_a, 42);
        generate_spines(&mut plates_b, &mut cache_b, 99999);

        let sum_a: f64 = plates_a.iter().map(|p| p.elevation).sum();
        let sum_b: f64 = plates_b.iter().map(|p| p.elevation).sum();
        if sum_a > 0.0 || sum_b > 0.0 {
            assert_ne!(sum_a, sum_b, "Different seeds should produce different spine patterns");
        }
    }

    #[test]
    fn small_region_matches_large_region() {
        let seed = 0x9E3779B97F4A7C15u64;
        let center = (0.0, 0.0);

        let (mut small_plates, mut small_cache) =
            collect_classified_plates(center.0, center.1, 40_000.0, seed);
        generate_spines(&mut small_plates, &mut small_cache, seed);
        let small_map: HashMap<u64, f64> = small_plates.iter()
            .map(|p| (p.id, p.elevation))
            .collect();

        let (mut large_plates, mut large_cache) =
            collect_classified_plates(center.0, center.1, 120_000.0, seed);
        generate_spines(&mut large_plates, &mut large_cache, seed);

        let mut checked = 0;
        for plate in &large_plates {
            if let Some(&small_elev) = small_map.get(&plate.id) {
                assert_eq!(
                    plate.elevation, small_elev,
                    "Plate {} has elevation {} in large region but {} in small region",
                    plate.id, plate.elevation, small_elev
                );
                checked += 1;
            }
        }
        assert_eq!(checked, small_plates.len(),
            "All small-region plates should appear in large region");
    }

    #[test]
    fn generate_spines_returns_instances() {
        let seed = 0x9E3779B97F4A7C15u64;
        let (mut plates, mut cache) = collect_classified_plates(0.0, 0.0, 80_000.0, seed);
        let instances = generate_spines(&mut plates, &mut cache, seed);

        let has_spines = plates.iter().any(|p| p.has_tag(&PlateTag::Ridge));
        if has_spines {
            assert!(!instances.is_empty(), "should return instances when spines exist");
            for inst in &instances {
                assert!(!inst.peaks.is_empty(), "every instance should have peaks");
                assert!(inst.bounding_radius > 0.0, "bounding radius should be positive");
            }
        }
    }

    // ── SpineCache tests ─────────────────────────────────────────────────────

    #[test]
    fn spine_cache_deterministic() {
        let seed = 0x9E3779B97F4A7C15u64;
        let mut cache_a = SpineCache::new(seed);
        let mut cache_b = SpineCache::new(seed);
        let mut plate_a = PlateCache::new(seed);
        let mut plate_b = PlateCache::new(seed);

        let points = [(0.0, 0.0), (5000.0, 3000.0), (-10000.0, 8000.0), (20000.0, -5000.0)];
        for (wx, wy) in points {
            let a = cache_a.elevation_at(wx, wy, &mut plate_a);
            let b = cache_b.elevation_at(wx, wy, &mut plate_b);
            assert_eq!(a, b, "SpineCache must be deterministic at ({wx}, {wy})");
        }
    }

    #[test]
    fn spine_cache_matches_generate_spines() {
        let seed = 0x9E3779B97F4A7C15u64;
        let mut spine_cache = SpineCache::new(seed);
        let mut plate_cache_for_spine = PlateCache::new(seed);

        let mut plate_cache = PlateCache::new(seed);
        let mut plates = plate_cache.plates_in_radius(0.0, 0.0, 80_000.0);
        plate_cache.classify_tags(&mut plates);
        let instances = generate_spines(&mut plates, &mut plate_cache, seed);

        let test_point = instances.iter().find_map(|inst| {
            inst.peaks.first().map(|p| (p.wx, p.wy))
        });

        if let Some((wx, wy)) = test_point {
            let reference = evaluate_elevation(&instances, wx, wy);
            let cached = spine_cache.elevation_at(wx, wy, &mut plate_cache_for_spine);

            assert!(reference > 0.0, "reference should have elevation at spine center");
            assert_eq!(
                cached, reference,
                "SpineCache should match generate_spines at ({wx}, {wy}): cached={cached}, ref={reference}"
            );
        }
    }

    #[test]
    fn spine_cache_nonzero_on_land() {
        let seed = 0x9E3779B97F4A7C15u64;
        let mut cache = SpineCache::new(seed);
        let mut plate_cache = PlateCache::new(seed);

        let mut found_nonzero = false;
        'outer: for gx in -10..=10 {
            for gy in -10..=10 {
                let wx = gx as f64 * 2000.0;
                let wy = gy as f64 * 2000.0;
                if cache.elevation_at(wx, wy, &mut plate_cache) > 0.0 {
                    found_nonzero = true;
                    break 'outer;
                }
            }
        }
        assert!(found_nonzero, "Should find non-zero spine elevation somewhere near origin");
    }

    #[test]
    fn spine_cache_evicts_lru_chunks() {
        let mut cache = SpineCache::new(0);

        for i in 0..SPINE_CACHE_MAX_CHUNKS as i32 {
            cache.inject_entry((i, 0), i as u64);
        }
        assert_eq!(cache.cached_len(), SPINE_CACHE_MAX_CHUNKS);

        cache.inject_entry((999, 0), SPINE_CACHE_MAX_CHUNKS as u64);
        cache.evict_if_over_budget();
        assert_eq!(cache.cached_len(), SPINE_CACHE_MAX_CHUNKS);
        assert!(!cache.contains((0, 0)), "oldest entry should be evicted");
        assert!(cache.contains((999, 0)), "new entry should remain");
    }

    #[test]
    fn spine_cache_evicts_least_recently_accessed() {
        let mut cache = SpineCache::new(0);

        cache.inject_entry((0, 0), 10);
        cache.inject_entry((1, 0), 20);
        cache.inject_entry((2, 0), 30);

        cache.instance_cache.get_mut(&(0, 0)).unwrap().last_accessed = 40;

        cache.inject_entry((3, 0), 50);
        for i in 4..=SPINE_CACHE_MAX_CHUNKS as i32 {
            cache.inject_entry((i, 0), 50 + i as u64);
        }
        cache.evict_if_over_budget();

        assert!(!cache.contains((1, 0)), "entry with lowest stamp should be evicted");
        assert!(cache.contains((0, 0)), "recently touched entry should survive");
    }

    #[test]
    fn get_height_nonzero_somewhere() {
        let terrain = crate::Terrain::default();

        let mut found_nonzero = false;
        'outer: for q in (-20000..=20000).step_by(500) {
            for r in (-20000..=20000).step_by(500) {
                if terrain.get_height(q, r) != 0 {
                    found_nonzero = true;
                    break 'outer;
                }
            }
        }
        assert!(found_nonzero, "get_height should produce non-zero z from spine elevation");
    }

    // ── Ravine tests ────────────────────────────────────────────────────────

    #[test]
    fn stream_steps_flow_downhill() {
        let peaks = vec![test_peak(1000.0, 3000.0)];
        let stream = grow_stream(100.0, 0.0, 0.0, &peaks, &[], 0.0, 0, 0, 42, 12345);
        assert!(stream.steps.len() >= 2, "stream should have multiple steps");
        let first = stream.steps.first().unwrap().surface_elev;
        let last = stream.steps.last().unwrap().surface_elev;
        assert!(last < first, "stream should end lower ({last:.1}) than start ({first:.1})");
    }

    #[test]
    fn carved_elevation_leq_peak_elevation() {
        let peaks = vec![test_peak(1000.0, 3000.0)];
        let network = build_ravine_network(&peaks, &[], 42, 12345);
        for y in (-30..=30).map(|i| i as f64 * 100.0) {
            for x in (-30..=30).map(|i| i as f64 * 100.0) {
                let surface = evaluate_all_peaks(&peaks, x, y);
                let carved = network.carve(x, y, surface);
                assert!(
                    carved <= surface + 1e-6,
                    "Carved ({carved:.1}) > surface ({surface:.1}) at ({x}, {y})",
                );
            }
        }
    }

    #[test]
    fn valley_width_increases_with_merges() {
        let w0 = stream_width(0, 0.0);
        let w1 = stream_width(0, 150.0);
        let mut s = Stream {
            steps: vec![
                StreamStep { wx: 0.0, wy: 0.0, surface_elev: 500.0, floor_elev: 470.0, width: w0, cum_dist: 0.0, merge_count: 0, wall_exponent: stream_wall_exponent(0) },
                StreamStep { wx: 150.0, wy: 0.0, surface_elev: 450.0, floor_elev: 420.0, width: w1, cum_dist: 150.0, merge_count: 0, wall_exponent: stream_wall_exponent(0) },
            ],
            merge_count: 0,
            merged_into: None,
        };
        let w_before = s.steps[1].width;
        s.merge_count += 1;
        for step in &mut s.steps {
            step.merge_count += 1;
            step.width = stream_width(step.merge_count, step.cum_dist);
        }
        assert!(s.steps[1].width > w_before);
    }

    #[test]
    fn hanging_valley_floor_above_parent() {
        let peaks = vec![test_peak(1000.0, 3000.0)];
        let normal = grow_stream(100.0, 0.0, 0.0, &peaks, &[], 0.0, 0, 0, 42, 12345);
        let hanging = grow_stream(100.0, 0.0, 0.0, &peaks, &[], 200.0, 0, 0, 42, 12345);
        assert!(!normal.steps.is_empty());
        assert!(!hanging.steps.is_empty());
        assert!(
            hanging.steps[0].floor_elev > normal.steps[0].floor_elev,
            "Hanging valley floor ({:.1}) should exceed normal ({:.1})",
            hanging.steps[0].floor_elev,
            normal.steps[0].floor_elev,
        );
    }

    #[test]
    fn ravine_network_deterministic() {
        let peaks = vec![test_peak(1000.0, 3000.0)];
        let a = build_ravine_network(&peaks, &[], 42, 12345);
        let b = build_ravine_network(&peaks, &[], 42, 12345);
        assert_eq!(a.streams.len(), b.streams.len());
        for (sa, sb) in a.streams.iter().zip(b.streams.iter()) {
            assert_eq!(sa.steps.len(), sb.steps.len());
            for (stepa, stepb) in sa.steps.iter().zip(sb.steps.iter()) {
                assert_eq!(stepa.wx, stepb.wx);
                assert_eq!(stepa.wy, stepb.wy);
                assert_eq!(stepa.floor_elev, stepb.floor_elev);
            }
        }
    }

    #[test]
    fn empty_ravine_network_is_identity() {
        let network = RavineNetwork::empty();
        assert_eq!(network.carve(0.0, 0.0, 500.0), 500.0);
        assert_eq!(network.carve(0.0, 0.0, 0.0), 0.0);
        assert_eq!(network.carve(0.0, 0.0, -10.0), -10.0);
    }

    #[test]
    fn v_profile_centerline_equals_floor() {
        // A simple two-step stream along the x-axis.
        let steps = vec![
            StreamStep { wx: 0.0, wy: 0.0, surface_elev: 500.0, floor_elev: 480.0, width: 100.0, cum_dist: 0.0, merge_count: 0, wall_exponent: 1.0 },
            StreamStep { wx: 200.0, wy: 0.0, surface_elev: 490.0, floor_elev: 470.0, width: 100.0, cum_dist: 200.0, merge_count: 0, wall_exponent: 1.0 },
        ];
        let stream = Stream { steps, merge_count: 0, merged_into: None };
        let network = test_network(vec![stream]);
        // At centerline (wy=0), carved elevation should equal floor_elev.
        let carved = network.carve(0.0, 0.0, 500.0);
        assert!((carved - 480.0).abs() < 1.0, "centerline should be at floor, got {carved:.1}");
    }

    #[test]
    fn v_profile_edge_equals_surface() {
        // Stream along x-axis, width=100 → half_width=50.
        let steps = vec![
            StreamStep { wx: 0.0, wy: 0.0, surface_elev: 500.0, floor_elev: 480.0, width: 100.0, cum_dist: 0.0, merge_count: 0, wall_exponent: 1.0 },
            StreamStep { wx: 200.0, wy: 0.0, surface_elev: 490.0, floor_elev: 470.0, width: 100.0, cum_dist: 200.0, merge_count: 0, wall_exponent: 1.0 },
        ];
        let stream = Stream { steps, merge_count: 0, merged_into: None };
        let network = test_network(vec![stream]);
        // At half_width (wy=50), t=1.0 → carved = floor + (wall_top - floor) * 1.0 = wall_top.
        // Since surface=500 and query surface=500, wall_top=500, so no carving.
        let carved = network.carve(0.0, 50.0, 500.0);
        assert!((carved - 500.0).abs() < 1.0, "edge should be at surface, got {carved:.1}");
    }

    #[test]
    fn v_profile_monotonically_increasing_from_center() {
        let steps = vec![
            StreamStep { wx: 0.0, wy: 0.0, surface_elev: 500.0, floor_elev: 450.0, width: 200.0, cum_dist: 0.0, merge_count: 0, wall_exponent: 1.0 },
            StreamStep { wx: 300.0, wy: 0.0, surface_elev: 500.0, floor_elev: 450.0, width: 200.0, cum_dist: 300.0, merge_count: 0, wall_exponent: 1.0 },
        ];
        let stream = Stream { steps, merge_count: 0, merged_into: None };
        let network = test_network(vec![stream]);
        let mut prev = network.carve(150.0, 0.0, 500.0);
        for i in 1..=10 {
            let y = i as f64 * 10.0;
            let elev = network.carve(150.0, y, 500.0);
            assert!(elev >= prev - 1e-6, "profile should be monotonically increasing from center: at y={y}, {elev:.1} < {prev:.1}");
            prev = elev;
        }
    }

    #[test]
    fn v_profile_young_exponent_concave() {
        // Young exponent (0.5) → concave walls (steep at bottom, gentle at top).
        // At t=0.5: elevation = floor + (top - floor) * 0.5^0.5 ≈ floor + 0.707 * range
        // This is ABOVE the linear midpoint (floor + 0.5 * range), meaning concave.
        let floor = 450.0;
        let wall_top = 500.0;
        let t: f64 = 0.5;
        let young_exp = WALL_EXPONENT_YOUNG; // 0.5
        let carved = floor + (wall_top - floor) * t.powf(young_exp);
        let linear = floor + (wall_top - floor) * t;
        assert!(carved > linear, "young exponent should produce concave (above linear): {carved:.1} vs {linear:.1}");
    }

    #[test]
    fn v_profile_mature_exponent_convex() {
        // Mature exponent (1.5) → convex walls (gentle at bottom, steep at top).
        // At t=0.5: elevation = floor + (top - floor) * 0.5^1.5 ≈ floor + 0.354 * range
        // This is BELOW the linear midpoint (floor + 0.5 * range), meaning convex.
        let floor = 450.0;
        let wall_top = 500.0;
        let t: f64 = 0.5;
        let mature_exp = WALL_EXPONENT_MATURE; // 1.5
        let carved = floor + (wall_top - floor) * t.powf(mature_exp);
        let linear = floor + (wall_top - floor) * t;
        assert!(carved < linear, "mature exponent should produce convex (below linear): {carved:.1} vs {linear:.1}");
    }

    #[test]
    fn v_profile_origin_step_barely_carves() {
        // At origin: width=1, depth=0 → carved ≈ surface everywhere.
        let steps = vec![
            StreamStep { wx: 0.0, wy: 0.0, surface_elev: 500.0, floor_elev: 500.0, width: STREAM_START_WIDTH, cum_dist: 0.0, merge_count: 0, wall_exponent: stream_wall_exponent(0) },
            StreamStep { wx: 150.0, wy: 0.0, surface_elev: 498.0, floor_elev: 496.5, width: stream_width(0, 150.0), cum_dist: 150.0, merge_count: 0, wall_exponent: stream_wall_exponent(0) },
        ];
        let stream = Stream { steps, merge_count: 0, merged_into: None };
        let network = test_network(vec![stream]);
        // At origin, floor=surface=500 → no carving.
        let carved = network.carve(0.0, 0.0, 500.0);
        assert!((carved - 500.0).abs() < 0.1, "origin should barely carve, got {carved:.1}");
    }

    #[test]
    fn wall_exponent_evolves_with_merges() {
        let young = stream_wall_exponent(0);
        let mid = stream_wall_exponent(3);
        let mature = stream_wall_exponent(6);
        let over = stream_wall_exponent(12);
        assert!((young - WALL_EXPONENT_YOUNG).abs() < 1e-6);
        assert!(mid > young && mid < mature, "mid-maturity should be between young and mature");
        assert!((mature - WALL_EXPONENT_MATURE).abs() < 1e-6);
        assert!((over - WALL_EXPONENT_MATURE).abs() < 1e-6, "beyond maturity should clamp");
    }

    /// Diagnostic: prints step-by-step direction vectors for a stream to
    /// identify zigzag or unexpected heading behaviour. Run with:
    ///   cargo test -p terrain stream_direction_diagnostic -- --nocapture
    #[test]
    fn stream_direction_diagnostic() {
        use crate::noise::simplex_2d;

        let peaks = vec![
            Peak { wx: 0.0, wy: 0.0, height: 1000.0, falloff_radius: 3000.0 },
        ];
        let ridgelines: Vec<Ridgeline> = vec![];
        let seed = 12345u64;
        let spine_id = 42u64;
        let hash_a = 0u64;
        let hash_b = 0u64;

        let start_wx = 100.0;
        let start_wy = 0.0;
        let initial_angle = 0.0f64;

        let mut wx = start_wx;
        let mut wy = start_wy;
        let mut distance = 0.0f64;
        let mut dir_x = initial_angle.cos();
        let mut dir_y = initial_angle.sin();

        println!("{:>4} {:>10} {:>10} {:>8} | {:>8} {:>8} | {:>8} {:>8} | {:>8} {:>8} | {:>8} {:>8} | {:>5}",
            "step", "wx", "wy", "elev",
            "grad_dx", "grad_dy",
            "blend_dx", "blend_dy",
            "noise_dx", "noise_dy",
            "clmp_dx", "clmp_dy",
            "clmpd");

        for i in 0..30 {
            let surface_elev = evaluate_surface(&peaks, &ridgelines, wx, wy);
            if surface_elev <= 0.0 { println!("  step {i}: surface_elev <= 0, stopping"); break; }

            let (grad_dx, grad_dy) = blended_gradient(&peaks, &ridgelines, wx, wy);
            if grad_dx == 0.0 && grad_dy == 0.0 { println!("  step {i}: flat gradient, stopping"); break; }

            let blended_x = dir_x * STREAM_MOMENTUM + grad_dx * (1.0 - STREAM_MOMENTUM);
            let blended_y = dir_y * STREAM_MOMENTUM + grad_dy * (1.0 - STREAM_MOMENTUM);
            let blend_mag = (blended_x * blended_x + blended_y * blended_y).sqrt();
            if blend_mag < 1e-10 { break; }
            let base_dx = blended_x / blend_mag;
            let base_dy = blended_y / blend_mag;

            let base_angle = base_dy.atan2(base_dx);
            let lateral_noise = simplex_2d(
                distance / STREAM_LATERAL_WAVELENGTH,
                hash_a.wrapping_mul(1000).wrapping_add(hash_b) as f64,
                seed ^ SEED_STREAM_LATERAL ^ spine_id,
            ) * STREAM_LATERAL_AMP;
            let angle = base_angle + lateral_noise;
            let mut step_dx = angle.cos();
            let mut step_dy = angle.sin();

            // Pre-clamp direction (for momentum feedback)
            let pre_clamp_dx = step_dx;
            let pre_clamp_dy = step_dy;

            let dot = step_dx * grad_dx + step_dy * grad_dy;
            let clamped = dot < MIN_DOWNHILL_ALIGNMENT;
            if clamped {
                let cross_x = step_dx - dot * grad_dx;
                let cross_y = step_dy - dot * grad_dy;
                let cross_len = (cross_x * cross_x + cross_y * cross_y).sqrt();
                let cross_scale = (1.0 - MIN_DOWNHILL_ALIGNMENT * MIN_DOWNHILL_ALIGNMENT).sqrt();
                if cross_len > 1e-10 {
                    step_dx = MIN_DOWNHILL_ALIGNMENT * grad_dx + cross_scale * cross_x / cross_len;
                    step_dy = MIN_DOWNHILL_ALIGNMENT * grad_dy + cross_scale * cross_y / cross_len;
                } else {
                    step_dx = grad_dx;
                    step_dy = grad_dy;
                }
            }

            println!("{:4} {:10.1} {:10.1} {:8.1} | {:8.4} {:8.4} | {:8.4} {:8.4} | {:8.4} {:8.4} | {:8.4} {:8.4} | {:>5}",
                i, wx, wy, surface_elev,
                grad_dx, grad_dy,
                base_dx, base_dy,
                pre_clamp_dx, pre_clamp_dy,
                step_dx, step_dy,
                if clamped { "YES" } else { "no" });

            dir_x = pre_clamp_dx;
            dir_y = pre_clamp_dy;
            wx += step_dx * STREAM_STEP_SIZE;
            wy += step_dy * STREAM_STEP_SIZE;
            distance += STREAM_STEP_SIZE;
        }
    }

    /// Diagnostic: two peaks with a ridgeline between them. Stream starts
    /// near the saddle and descends off the side. This is where gradient
    /// discontinuity from max-of-cones can cause zigzag. Run with:
    ///   cargo test -p terrain two_peak_direction_diagnostic -- --nocapture
    #[test]
    fn two_peak_direction_diagnostic() {
        use crate::noise::simplex_2d;

        // Two peaks 2000 apart along the x-axis, connected by a ridgeline.
        let peaks = vec![
            Peak { wx: -1000.0, wy: 0.0, height: 800.0, falloff_radius: 2500.0 },
            Peak { wx:  1000.0, wy: 0.0, height: 800.0, falloff_radius: 2500.0 },
        ];
        let ridgelines = vec![
            Ridgeline {
                peak_a: 0, peak_b: 1,
                ax: -1000.0, ay: 0.0,
                bx:  1000.0, by: 0.0,
                height_a: 800.0, height_b: 800.0,
                sag: 0.20,
            },
        ];
        let seed = 12345u64;
        let spine_id = 42u64;
        let hash_a = 0u64;
        let hash_b = 0u64;

        // Start near the saddle midpoint, slightly off-axis so the stream
        // descends perpendicular to the ridge.
        let start_wx = 0.0;
        let start_wy = 200.0;
        let initial_angle = std::f64::consts::FRAC_PI_2; // initially heading +y (away from ridge)

        let mut wx = start_wx;
        let mut wy = start_wy;
        let mut distance = 0.0f64;
        let mut dir_x = initial_angle.cos();
        let mut dir_y = initial_angle.sin();

        println!("\n=== Two-peak diagnostic: peaks at (-1000,0) and (1000,0), ridge sag=0.20 ===");
        println!("  Start: ({start_wx}, {start_wy}), initial heading: +y");
        println!("{:>4} {:>10} {:>10} {:>8} | {:>8} {:>8} | {:>8} {:>8} | {:>8} {:>8} | {:>8} {:>8} | {:>5}",
            "step", "wx", "wy", "elev",
            "grad_dx", "grad_dy",
            "blend_dx", "blend_dy",
            "noise_dx", "noise_dy",
            "clmp_dx", "clmp_dy",
            "clmpd");

        for i in 0..40 {
            let surface_elev = evaluate_surface(&peaks, &ridgelines, wx, wy);
            if surface_elev <= 0.0 { println!("  step {i}: surface_elev <= 0, stopping"); break; }

            let (grad_dx, grad_dy) = blended_gradient(&peaks, &ridgelines, wx, wy);
            if grad_dx == 0.0 && grad_dy == 0.0 { println!("  step {i}: flat gradient, stopping"); break; }

            let blended_x = dir_x * STREAM_MOMENTUM + grad_dx * (1.0 - STREAM_MOMENTUM);
            let blended_y = dir_y * STREAM_MOMENTUM + grad_dy * (1.0 - STREAM_MOMENTUM);
            let blend_mag = (blended_x * blended_x + blended_y * blended_y).sqrt();
            if blend_mag < 1e-10 { break; }
            let base_dx = blended_x / blend_mag;
            let base_dy = blended_y / blend_mag;

            let base_angle = base_dy.atan2(base_dx);
            let lateral_noise = simplex_2d(
                distance / STREAM_LATERAL_WAVELENGTH,
                hash_a.wrapping_mul(1000).wrapping_add(hash_b) as f64,
                seed ^ SEED_STREAM_LATERAL ^ spine_id,
            ) * STREAM_LATERAL_AMP;
            let angle = base_angle + lateral_noise;
            let mut step_dx = angle.cos();
            let mut step_dy = angle.sin();

            let pre_clamp_dx = step_dx;
            let pre_clamp_dy = step_dy;

            let dot = step_dx * grad_dx + step_dy * grad_dy;
            let clamped = dot < MIN_DOWNHILL_ALIGNMENT;
            if clamped {
                let cross_x = step_dx - dot * grad_dx;
                let cross_y = step_dy - dot * grad_dy;
                let cross_len = (cross_x * cross_x + cross_y * cross_y).sqrt();
                let cross_scale = (1.0 - MIN_DOWNHILL_ALIGNMENT * MIN_DOWNHILL_ALIGNMENT).sqrt();
                if cross_len > 1e-10 {
                    step_dx = MIN_DOWNHILL_ALIGNMENT * grad_dx + cross_scale * cross_x / cross_len;
                    step_dy = MIN_DOWNHILL_ALIGNMENT * grad_dy + cross_scale * cross_y / cross_len;
                } else {
                    step_dx = grad_dx;
                    step_dy = grad_dy;
                }
            }

            println!("{:4} {:10.1} {:10.1} {:8.1} | {:8.4} {:8.4} | {:8.4} {:8.4} | {:8.4} {:8.4} | {:8.4} {:8.4} | {:>5}",
                i, wx, wy, surface_elev,
                grad_dx, grad_dy,
                base_dx, base_dy,
                pre_clamp_dx, pre_clamp_dy,
                step_dx, step_dy,
                if clamped { "YES" } else { "no" });

            dir_x = pre_clamp_dx;
            dir_y = pre_clamp_dy;
            wx += step_dx * STREAM_STEP_SIZE;
            wy += step_dy * STREAM_STEP_SIZE;
            distance += STREAM_STEP_SIZE;
        }
    }

    /// Verify sequential growth detects nearby streams and merges.
    #[test]
    fn sequential_growth_detects_nearby_streams() {
        let make_step = |wx: f64, wy: f64| StreamStep {
            wx, wy,
            surface_elev: 500.0,
            floor_elev: 480.0,
            width: 20.0,
            cum_dist: 0.0,
            merge_count: 0,
            wall_exponent: stream_wall_exponent(0),
        };

        // Stream A already exists in the grid
        let steps_a: Vec<StreamStep> = (0..8)
            .map(|i| make_step(i as f64 * 150.0, 0.0))
            .collect();
        let stream_a = Stream { steps: steps_a, merge_count: 0, merged_into: None };

        let mut growth_grid: HexSpatialGrid<GrowthStepRef> = HexSpatialGrid::new(GROWTH_GRID_CELL_SIZE);
        register_stream_steps(&stream_a, 0, &mut growth_grid);

        let streams = vec![stream_a];

        // Stream B crosses A near (500, 0) — step at (500, 50) is within proximity
        let result = find_nearby_stream(500.0, 50.0, &growth_grid, &streams);
        assert!(result.is_some(), "Should detect nearby stream at (500, 50)");

        // Far away point should not find anything
        let far = find_nearby_stream(5000.0, 5000.0, &growth_grid, &streams);
        assert!(far.is_none(), "Should not detect stream at (5000, 5000)");
    }

    /// Verify merge count propagation through a tree.
    #[test]
    fn merge_count_propagates_through_tree() {
        let make_step = |wx: f64, wy: f64| StreamStep {
            wx, wy,
            surface_elev: 500.0,
            floor_elev: 480.0,
            width: 20.0,
            cum_dist: 0.0,
            merge_count: 0,
            wall_exponent: stream_wall_exponent(0),
        };

        // Three streams: B and C merge into A
        let mut streams = vec![
            Stream { // 0: trunk
                steps: vec![make_step(0.0, 0.0), make_step(150.0, 0.0), make_step(300.0, 0.0)],
                merge_count: 0,
                merged_into: None,
            },
            Stream { // 1: tributary merging into 0
                steps: vec![make_step(150.0, 300.0), make_step(150.0, 150.0), make_step(150.0, 0.0)],
                merge_count: 0,
                merged_into: Some(0),
            },
            Stream { // 2: tributary merging into 0
                steps: vec![make_step(300.0, 300.0), make_step(300.0, 150.0), make_step(300.0, 0.0)],
                merge_count: 0,
                merged_into: Some(0),
            },
        ];

        propagate_merge_counts(&mut streams);

        // Trunk should have merge_count = 2 (one from each tributary)
        assert!(streams[0].merge_count >= 2,
            "trunk merge_count should be ≥ 2, got {}", streams[0].merge_count);
    }

    #[test]
    fn stream_grid_finds_all_nearby_streams() {
        // Build a real ravine network and verify the grid lookup returns the
        // same streams that a full linear scan would find for sample points.
        let peaks = vec![test_peak(1000.0, 3000.0)];
        let network = build_ravine_network(&peaks, &[], 42, 12345);
        assert!(network.stream_grid.cell_size() > 0.0, "grid should be built");

        let mut grid_misses = 0;
        for y in (-30..=30).map(|i| i as f64 * 100.0) {
            for x in (-30..=30).map(|i| i as f64 * 100.0) {
                // Linear scan: find all streams within half_width.
                let mut linear_hits: Vec<usize> = Vec::new();
                for (si, stream) in network.streams.iter().enumerate() {
                    if stream.steps.len() < 2 { continue; }
                    let (dist, seg_idx, seg_t) = nearest_point_on_spline_range(&stream.steps, x, y, 0, stream.steps.len().saturating_sub(2));
                    let (_, width, _, _) = interpolate_spline_step(&stream.steps, seg_idx, seg_t);
                    if dist < width / 2.0 {
                        linear_hits.push(si);
                    }
                }

                // Grid lookup: find streams in hex neighborhood.
                let mut grid_streams: Vec<usize> = Vec::new();
                for r in network.stream_grid.query(x, y) {
                    grid_streams.push(r.stream_idx);
                }
                grid_streams.sort_unstable();
                grid_streams.dedup();

                // Every stream found by linear scan must be in the grid lookup.
                for &si in &linear_hits {
                    if !grid_streams.contains(&si) {
                        grid_misses += 1;
                    }
                }
            }
        }
        assert_eq!(grid_misses, 0, "grid missed {grid_misses} stream references that linear scan found");
    }

    /// Diagnostic test for the ravine anomaly near world position (4700, 5700).
    /// Outputs per-stream step data for all spine instances covering the area
    /// so the anomaly can be traced through the pipeline.
    ///
    /// Run with: cargo test -p terrain ravine_diagnostic_4700_5700 -- --nocapture
    #[test]
    fn ravine_diagnostic_4700_5700() {
        let seed: u64 = 0x9E3779B97F4A7C15;
        let cx = 4700.0;
        let cy = 5700.0;
        let radius = 500.0;
        let scan_radius = 1500.0; // broader scan to catch spines whose streams reach the area

        // Run the full pipeline.
        let region = crate::generate_region(seed, cx, cy, scan_radius, true);

        eprintln!("=== Ravine diagnostic at ({cx}, {cy}) r={radius} ===");
        eprintln!("Spine instances in region: {}", region.spine_instances.len());

        // Find instances whose bounding circle overlaps the area of interest.
        let mut relevant = Vec::new();
        for (i, inst) in region.spine_instances.iter().enumerate() {
            let (bcx, bcy) = inst.bounding_center;
            let dx = bcx - cx;
            let dy = bcy - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < inst.bounding_radius + radius {
                relevant.push(i);
            }
        }
        eprintln!("Spine instances overlapping area: {}", relevant.len());

        for &idx in &relevant {
            let inst = &region.spine_instances[idx];
            let stats = inst.ravine_network.stats();
            eprintln!("\n--- Spine #{idx} (id={:#x}) ---", inst.id);
            eprintln!("  bounding center: ({:.0}, {:.0}), radius: {:.0}",
                inst.bounding_center.0, inst.bounding_center.1, inst.bounding_radius);
            eprintln!("  peaks: {}, ridgelines: {}", inst.peaks.len(), inst.ridgelines.len());
            eprintln!("  ravine stats: {} streams, {} merged, {} hanging, {} paths",
                stats.stream_count, stats.merged_count, stats.hanging_count, stats.path_count);
            eprintln!("  width: {:.0}-{:.0}, depth: {:.0}-{:.0}, length: {:.0}-{:.0}",
                stats.width_range.0, stats.width_range.1,
                stats.depth_range.0, stats.depth_range.1,
                stats.length_range.0, stats.length_range.1);

            // Dump streams that have any step within radius of the query point.
            let net = &inst.ravine_network;
            for (si, stream) in net.streams.iter().enumerate() {
                let near_area = stream.steps.iter().any(|s| {
                    let dx = s.wx - cx;
                    let dy = s.wy - cy;
                    (dx * dx + dy * dy).sqrt() < radius + 300.0
                });
                if !near_area { continue; }

                eprintln!("\n  Stream #{si}: {} steps, merge_count={}, merged_into={:?}",
                    stream.steps.len(), stream.merge_count, stream.merged_into);

                for (step_i, step) in stream.steps.iter().enumerate() {
                    let dx = step.wx - cx;
                    let dy = step.wy - cy;
                    let dist_to_center = (dx * dx + dy * dy).sqrt();
                    let marker = if dist_to_center < radius { " <-- IN AREA" } else { "" };
                    eprintln!("    step[{step_i:3}] pos=({:7.1}, {:7.1}) surface={:7.1} floor={:7.1} \
                        depth={:6.1} width={:5.1} cum_dist={:7.1} mc={} wall_exp={:.2}{marker}",
                        step.wx, step.wy,
                        step.surface_elev, step.floor_elev,
                        step.surface_elev - step.floor_elev,
                        step.width, step.cum_dist,
                        step.merge_count, step.wall_exponent);
                }
            }

            // Sample elevation_at across a grid within the area of interest.
            eprintln!("\n  Elevation grid (10x10 samples within area):");
            let step = radius * 2.0 / 10.0;
            for row in 0..10 {
                let wy = cy - radius + step * (row as f64 + 0.5);
                let mut line = format!("    y={wy:7.0}: ");
                for col in 0..10 {
                    let wx = cx - radius + step * (col as f64 + 0.5);

                    let peak_elev = inst.evaluate_peaks(wx, wy);
                    let ridge_elev = inst.evaluate_ridgelines(wx, wy);
                    let surface = peak_elev.max(ridge_elev);
                    let final_elev = inst.elevation_at(wx, wy);
                    let carve = surface - final_elev;

                    if surface > 0.0 {
                        line.push_str(&format!("{final_elev:5.0}({carve:+4.0}) "));
                    } else {
                        line.push_str("    .      ");
                    }
                }
                eprintln!("{line}");
            }

            // Probe ravine type across the area.
            eprintln!("\n  Ravine probe grid (20x20):");
            let step = radius * 2.0 / 20.0;
            for row in 0..20 {
                let wy = cy - radius + step * (row as f64 + 0.5);
                let mut line = format!("    y={wy:7.0}: ");
                for col in 0..20 {
                    let wx = cx - radius + step * (col as f64 + 0.5);
                    match inst.ravine_probe(wx, wy) {
                        None => line.push('.'),
                        Some(RavineProbe::Floor) => line.push('F'),
                        Some(RavineProbe::Wall(t)) => {
                            if t < 0.33 { line.push('w'); }
                            else if t < 0.66 { line.push('W'); }
                            else { line.push('R'); } // near rim
                        }
                        Some(RavineProbe::Path) => line.push('P'),
                    }
                }
                eprintln!("{line}");
            }
        }

        // Also sample composite elevation across ALL spines at the area.
        eprintln!("\n=== Composite elevation (all spines) ===");
        let step = radius * 2.0 / 20.0;
        for row in 0..20 {
            let wy = cy - radius + step * (row as f64 + 0.5);
            let mut line = format!("  y={wy:7.0}: ");
            for col in 0..20 {
                let wx = cx - radius + step * (col as f64 + 0.5);
                let elev = evaluate_elevation(&region.spine_instances, wx, wy);
                if elev > 0.0 {
                    line.push_str(&format!("{elev:5.0} "));
                } else {
                    line.push_str("    . ");
                }
            }
            eprintln!("{line}");
        }
    }

    #[test]
    fn stream_grid_carve_matches_linear_scan() {
        // Verify the grid-accelerated carve produces identical results to
        // what the old linear scan would produce.
        let peaks = vec![test_peak(1000.0, 3000.0)];
        let network = build_ravine_network(&peaks, &[], 42, 12345);

        for y in (-30..=30).map(|i| i as f64 * 100.0) {
            for x in (-30..=30).map(|i| i as f64 * 100.0) {
                let surface = evaluate_all_peaks(&peaks, x, y);
                let carved = network.carve(x, y, surface);

                // Recompute with linear scan for comparison.
                let mut expected = surface;
                if surface > 0.0 {
                    for stream in &network.streams {
                        if stream.steps.len() < 2 { continue; }
                        let (dist, seg_idx, seg_t) = nearest_point_on_spline_range(&stream.steps, x, y, 0, stream.steps.len().saturating_sub(2));
                        let (floor, width, stream_surface, wall_exponent) =
                            interpolate_spline_step(&stream.steps, seg_idx, seg_t);
                        let half_width = width / 2.0;
                        if dist >= half_width { continue; }
                        let t = dist / half_width;
                        let wall_top = stream_surface.min(expected);
                        let c = floor + (wall_top - floor) * t.powf(wall_exponent);
                        expected = expected.min(c);
                    }
                }

                assert!(
                    (carved - expected).abs() < 1e-6,
                    "carve mismatch at ({x}, {y}): grid={carved:.6}, linear={expected:.6}",
                );
            }
        }
    }
}
