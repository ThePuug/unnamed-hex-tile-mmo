use std::cell::RefCell;
use std::collections::HashMap;

use noise::{NoiseFn, Perlin};

// ──── Tuning Parameters ────

/// Continental plate grid cell size in tiles.
pub const CONTINENTAL_GRID_SIZE: i32 = 2_000;

/// Regional plate grid cell size in tiles.
pub const REGIONAL_GRID_SIZE: i32 = 600;

/// Continental plate jitter (fixed fraction, no noise dependency).
const CONTINENTAL_JITTER: f64 = 0.3;

/// Regional plate jitter range (variable, noise-driven).
const MAX_JITTER_FRACTION: f64 = 0.45;
const MIN_JITTER_FRACTION: f64 = 0.15;

/// Scale of jitter variation noise field (tile coordinates).
const JITTER_NOISE_SCALE: f64 = 1.0 / 1_000.0;

/// Scale of drift direction noise (tile coordinates).
const DRIFT_NOISE_SCALE: f64 = 1.0 / 2_500.0;

/// Continental plate base elevation range.
const CONTINENTAL_ELEV_MIN: f64 = 200.0;
const CONTINENTAL_ELEV_MAX: f64 = 600.0;

/// Oceanic plate base elevation range.
const OCEANIC_ELEV_MIN: f64 = 25.0;
const OCEANIC_ELEV_MAX: f64 = 100.0;

/// Target fraction of plates that are continental.
const CONTINENTAL_RATIO: f64 = 0.6;

/// Domain warp: distort Voronoi input coordinates for organic boundaries.
const CONT_WARP_AMPLITUDE: f64 = 300.0;
const CONT_WARP_SCALE: f64 = 1.0 / 1_500.0;
const REG_WARP_AMPLITUDE: f64 = 75.0;
const REG_WARP_SCALE: f64 = 1.0 / 400.0;

/// Regional skip probability range.
/// High jitter (chaotic) → low skip → more regional plates.
/// Low jitter (stable) → high skip → macro dominates.
const REGIONAL_MIN_SKIP: f64 = 0.0;
const REGIONAL_MAX_SKIP: f64 = 0.8;

/// Seed offset to decorrelate regional grid from continental grid.
const REGIONAL_SEED_OFFSET: u64 = 0xA77E_C701_1C00_0001;

/// Continental boundary influence distances (tiles).
const CONT_CONVERGENT_MAX_DIST: f64 = 750.0;
const CONT_DIVERGENT_MAX_DIST: f64 = 300.0;
const CONT_TRANSFORM_MAX_DIST: f64 = 800.0;

/// Continental peak elevation contributions at boundary center.
const CONT_CONVERGENT_PEAK: f64 = 400.0;
const CONT_DIVERGENT_PEAK: f64 = -150.0;

/// Continental transform crumple zone parameters.
const CONT_TRANSFORM_BIAS_SCALE: f64 = 50.0;
const CONT_TRANSFORM_DISRUPTION_SCALE: f64 = 40.0;
const CONT_TRANSFORM_CRUMPLE_WIDTH: f64 = 500.0;
const CONT_TRANSFORM_NOISE_SCALE: f64 = 1.0 / 150.0;

/// Regional boundary influence distances (tiles).
const REG_CONVERGENT_MAX_DIST: f64 = 200.0;
const REG_DIVERGENT_MAX_DIST: f64 = 100.0;
const REG_TRANSFORM_MAX_DIST: f64 = 250.0;

/// Regional peak elevation contributions at boundary center.
const REG_CONVERGENT_PEAK: f64 = 50.0;
const REG_DIVERGENT_PEAK: f64 = -40.0;

/// Regional transform crumple zone parameters.
const REG_TRANSFORM_BIAS_SCALE: f64 = 15.0;
const REG_TRANSFORM_DISRUPTION_SCALE: f64 = 12.0;
const REG_TRANSFORM_CRUMPLE_WIDTH: f64 = 150.0;
const REG_TRANSFORM_NOISE_SCALE: f64 = 1.0 / 50.0;

/// Per-tile boundary intensity modulation noise scales.
/// Continental: ~4 cycles per boundary (~4,000 tile edges).
/// Regional: ~4 cycles per boundary (~1,200 tile edges).
const CONT_INTENSITY_NOISE_SCALE: f64 = 1.0 / 500.0;
const REG_INTENSITY_NOISE_SCALE: f64 = 1.0 / 150.0;

// ──── Hex Geometry ────

const SQRT_3: f64 = 1.7320508075688772;
const HEX_SPACING: f64 = SQRT_3;

fn hex_to_cartesian(q: f64, r: f64) -> (f64, f64) {
    (SQRT_3 * q + SQRT_3 / 2.0 * r, 1.5 * r)
}

/// Squared distance in hex axial coordinates (accounts for 60° between q and r axes).
fn hex_dist_sq(dq: f64, dr: f64) -> f64 {
    dq * dq + dq * dr + dr * dr
}

/// Convert hex axial coordinates to isotropic Cartesian for noise sampling.
/// Unit-hex-spacing metric: adjacent hex centers are 1.0 apart, so noise
/// scale constants in "tiles" (e.g. `1/3000.0`) work directly.
fn hex_to_noise(q: f64, r: f64) -> (f64, f64) {
    (q + r * 0.5, r * SQRT_3 / 2.0)
}

// ──── Hashing ────

fn hash_u64(a: i64, b: i64, seed: u64) -> u64 {
    let mut h = seed ^ 0x517cc1b727220a95;
    h = h.wrapping_mul(0x517cc1b727220a95).wrapping_add(a as u64);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd).wrapping_add(b as u64);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
}

fn hash_f64(a: i64, b: i64, seed: u64, channel: u64) -> f64 {
    let h = hash_u64(a, b, seed ^ channel.wrapping_mul(0x9E3779B97F4A7C15));
    (h >> 11) as f64 / (1u64 << 53) as f64
}

// ──── Domain Warping ────
// Curl noise warping: derives both displacement components from a single scalar
// noise field, rotated 90°. The resulting vector field is divergence-free —
// vectors swirl smoothly around noise features rather than converging/diverging.
// No cusps, full organic variation per tile.

/// Curl noise warp in Cartesian space. Computes finite-difference partial
/// derivatives of a single scalar noise field, then rotates 90° to get a
/// divergence-free displacement vector. Returns displacement in hex axial coords.
fn curl_warp(q: f64, r: f64, scale: f64, amplitude: f64, noise: &Perlin, offset: f64) -> (f64, f64) {
    let (x, y) = hex_to_noise(q, r);
    let sx = x * scale + offset;
    let sy = y * scale + offset;

    let eps = scale; // one tile step in noise space
    let n = noise.get([sx, sy]);
    let dn_dx = (noise.get([sx + eps, sy]) - n) / eps;
    let dn_dy = (noise.get([sx, sy + eps]) - n) / eps;

    // Curl: rotate gradient 90°
    let warp_x = dn_dy * amplitude;
    let warp_y = -dn_dx * amplitude;

    // Convert Cartesian displacement back to hex axial
    let warp_r = warp_y / (SQRT_3 / 2.0);
    let warp_q = warp_x - warp_r * 0.5;

    (warp_q, warp_r)
}

fn continental_warp(q: f64, r: f64, noise: &Perlin) -> (f64, f64) {
    let (wq, wr) = curl_warp(q, r, CONT_WARP_SCALE, CONT_WARP_AMPLITUDE, noise, 50_000.0);
    (q + wq, r + wr)
}

fn regional_warp(q: f64, r: f64, noise: &Perlin) -> (f64, f64) {
    let (wq, wr) = curl_warp(q, r, REG_WARP_SCALE, REG_WARP_AMPLITUDE, noise, 130_000.0);
    (q + wq, r + wr)
}

// ──── Public Types ────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlateId {
    pub cell_q: i32,
    pub cell_r: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryScale {
    Continental,
    Regional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryKind {
    Convergent,
    Divergent,
    Transform,
}

impl BoundaryKind {
    /// Maximum influence distance (in tiles) for this boundary kind at the given scale.
    pub fn max_distance(&self, scale: BoundaryScale) -> f64 {
        match (self, scale) {
            (BoundaryKind::Convergent, BoundaryScale::Continental) => CONT_CONVERGENT_MAX_DIST,
            (BoundaryKind::Divergent, BoundaryScale::Continental) => CONT_DIVERGENT_MAX_DIST,
            (BoundaryKind::Transform, BoundaryScale::Continental) => CONT_TRANSFORM_MAX_DIST,
            (BoundaryKind::Convergent, BoundaryScale::Regional) => REG_CONVERGENT_MAX_DIST,
            (BoundaryKind::Divergent, BoundaryScale::Regional) => REG_DIVERGENT_MAX_DIST,
            (BoundaryKind::Transform, BoundaryScale::Regional) => REG_TRANSFORM_MAX_DIST,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BoundaryInfo {
    pub kind: BoundaryKind,
    pub intensity: f64,
    pub distance: f64,
}

#[derive(Clone, Debug)]
pub struct TerrainEval {
    pub height: i32,
    // Continental level
    pub continental_plate: PlateId,
    pub is_continental: bool,
    pub base_elevation: f64,
    pub continental_boundary: BoundaryInfo,
    pub continental_neighbor: PlateId,
    pub continental_neighbor_is_continental: bool,
    // Regional level (None if no regional plates in this area)
    pub regional_plate: Option<PlateId>,
    pub regional_boundary: Option<BoundaryInfo>,
}

// ──── Internal Types ────

#[derive(Clone, Debug)]
struct PlateInfo {
    cell_q: i32,
    cell_r: i32,
    center_hex_q: f64,
    center_hex_r: f64,
    center_x: f64,
    center_y: f64,
    base_elevation: f64,
    drift_x: f64,
    drift_y: f64,
    is_continental: bool,
}

struct ContinentalResult {
    plate: PlateInfo,
    neighbor: PlateInfo,
    dist_to_boundary: f64,
    junction_factor: f64,
}

/// Decomposed drift between two plates: perpendicular (across boundary)
/// and parallel (along boundary) components plus classification.
struct BoundaryClassification {
    kind: BoundaryKind,
    intensity: f64,
    /// Signed perpendicular component (positive = converging, negative = diverging).
    /// Small for transform boundaries — drives transpression/transtension bias.
    perpendicular: f64,
    /// Signed parallel component (shear). Large for transform boundaries —
    /// drives crumple disruption amplitude.
    parallel: f64,
}

// ──── Continental Plates ────

/// Hex axial center of a continental plate (cheap: hash only, no noise).
/// Used by `nearest_continental_id` for fast plate membership lookups.
fn continental_center_hex(cell_q: i32, cell_r: i32, seed: u64) -> (f64, f64) {
    let gs = CONTINENTAL_GRID_SIZE as f64;
    let cq = cell_q as i64;
    let cr = cell_r as i64;
    let center_q = cell_q as f64 * gs + gs / 2.0
        + (hash_f64(cq, cr, seed, 0) - 0.5) * 2.0 * CONTINENTAL_JITTER * gs;
    let center_r = cell_r as f64 * gs + gs / 2.0
        + (hash_f64(cq, cr, seed, 1) - 0.5) * 2.0 * CONTINENTAL_JITTER * gs;
    (center_q, center_r)
}

/// Full continental plate info for a cell (always active, never skipped).
fn continental_plate_for_cell(cell_q: i32, cell_r: i32, seed: u64, noise: &Perlin) -> PlateInfo {
    let gs = CONTINENTAL_GRID_SIZE as f64;
    let cq = cell_q as i64;
    let cr = cell_r as i64;
    let cell_center_q = cell_q as f64 * gs + gs / 2.0;
    let cell_center_r = cell_r as f64 * gs + gs / 2.0;

    let offset_q = (hash_f64(cq, cr, seed, 0) - 0.5) * 2.0 * CONTINENTAL_JITTER * gs;
    let offset_r = (hash_f64(cq, cr, seed, 1) - 0.5) * 2.0 * CONTINENTAL_JITTER * gs;
    let center_q = cell_center_q + offset_q;
    let center_r = cell_center_r + offset_r;
    let (center_x, center_y) = hex_to_cartesian(center_q, center_r);

    let is_continental = hash_f64(cq, cr, seed, 2) < CONTINENTAL_RATIO;

    let elev_t = hash_f64(cq, cr, seed, 3);
    let base_elevation = if is_continental {
        CONTINENTAL_ELEV_MIN + elev_t * (CONTINENTAL_ELEV_MAX - CONTINENTAL_ELEV_MIN)
    } else {
        OCEANIC_ELEV_MIN + elev_t * (OCEANIC_ELEV_MAX - OCEANIC_ELEV_MIN)
    };

    let (dnx, dny) = hex_to_noise(cell_center_q, cell_center_r);
    let regional_angle = noise.get([
        dnx * DRIFT_NOISE_SCALE + 3333.0,
        dny * DRIFT_NOISE_SCALE + 4444.0,
    ]) * std::f64::consts::TAU;
    let local_variation = (hash_f64(cq, cr, seed, 4) - 0.5) * std::f64::consts::PI;
    let drift_angle = regional_angle + local_variation;
    let drift_magnitude = 0.5 + hash_f64(cq, cr, seed, 5) * 0.5;

    PlateInfo {
        cell_q, cell_r,
        center_hex_q: center_q, center_hex_r: center_r,
        center_x, center_y, base_elevation,
        drift_x: drift_angle.cos() * drift_magnitude,
        drift_y: drift_angle.sin() * drift_magnitude,
        is_continental,
    }
}

/// Find nearest and second-nearest continental plates.
/// Input coordinates are domain-warped before Voronoi lookup.
/// Hex distance is used for plate ranking; Cartesian for boundary geometry.
fn continental_eval(q: i32, r: i32, seed: u64, noise: &Perlin) -> ContinentalResult {
    let (wq, wr) = continental_warp(q as f64, r as f64, noise);
    let (px, py) = hex_to_cartesian(wq, wr);
    let cell_q = (wq / CONTINENTAL_GRID_SIZE as f64).floor() as i32;
    let cell_r = (wr / CONTINENTAL_GRID_SIZE as f64).floor() as i32;

    let mut nearest = (f64::MAX, None::<PlateInfo>);
    let mut second = (f64::MAX, None::<PlateInfo>);
    let mut third_dist = f64::MAX;

    for dq in -2..=2 {
        for dr in -2..=2 {
            let plate = continental_plate_for_cell(cell_q + dq, cell_r + dr, seed, noise);
            let d2 = hex_dist_sq(wq - plate.center_hex_q, wr - plate.center_hex_r);
            if d2 < nearest.0 {
                third_dist = second.0;
                second = nearest;
                nearest = (d2, Some(plate));
            } else if d2 < second.0 {
                third_dist = second.0;
                second = (d2, Some(plate));
            } else if d2 < third_dist {
                third_dist = d2;
            }
        }
    }

    let plate = nearest.1.expect("continental eval must find nearest");
    let neighbor = second.1.expect("continental eval must find second-nearest");

    // Triple-junction dampening: fade boundary contributions where three plates meet
    let d2_sqrt = second.0.sqrt();
    let junction_factor = if d2_sqrt > 1e-10 {
        ((third_dist.sqrt() - d2_sqrt) / d2_sqrt).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mid_x = (plate.center_x + neighbor.center_x) / 2.0;
    let mid_y = (plate.center_y + neighbor.center_y) / 2.0;
    let ab_x = neighbor.center_x - plate.center_x;
    let ab_y = neighbor.center_y - plate.center_y;
    let ab_len = (ab_x * ab_x + ab_y * ab_y).sqrt();

    let dist_to_boundary = if ab_len > 1e-10 {
        let dot = (px - mid_x) * ab_x + (py - mid_y) * ab_y;
        dot.abs() / ab_len
    } else {
        0.0
    };

    ContinentalResult { plate, neighbor, dist_to_boundary, junction_factor }
}

/// Which continental plate does the given hex position belong to?
/// Input coordinates are domain-warped for consistency with continental_eval.
fn nearest_continental_id(hex_q: f64, hex_r: f64, seed: u64, noise: &Perlin) -> (i32, i32) {
    let (wq, wr) = continental_warp(hex_q, hex_r, noise);
    let cell_q = (wq / CONTINENTAL_GRID_SIZE as f64).floor() as i32;
    let cell_r = (wr / CONTINENTAL_GRID_SIZE as f64).floor() as i32;

    let mut best_dist = f64::MAX;
    let mut best_id = (cell_q, cell_r);

    for dq in -2..=2 {
        for dr in -2..=2 {
            let (cq, cr) = continental_center_hex(cell_q + dq, cell_r + dr, seed);
            let d2 = hex_dist_sq(wq - cq, wr - cr);
            if d2 < best_dist {
                best_dist = d2;
                best_id = (cell_q + dq, cell_r + dr);
            }
        }
    }
    best_id
}

thread_local! {
    /// Cache: regional grid cell → continental plate cell ID.
    /// Invalidated when the terrain seed changes.
    static REGIONAL_PARENT_CACHE: RefCell<(u64, HashMap<(i32, i32), (i32, i32)>)> =
        RefCell::new((u64::MAX, HashMap::new()));
}

/// Cached lookup of which continental plate a regional center belongs to.
/// Regional centers are deterministic from (cell_q, cell_r, seed), so their
/// continental membership is computed once and reused across all tiles that
/// consider the same regional candidate.
fn cached_continental_id(
    regional_cell_q: i32,
    regional_cell_r: i32,
    center_hex_q: f64,
    center_hex_r: f64,
    seed: u64,
    noise: &Perlin,
) -> (i32, i32) {
    REGIONAL_PARENT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.0 != seed {
            cache.0 = seed;
            cache.1.clear();
        }
        *cache.1
            .entry((regional_cell_q, regional_cell_r))
            .or_insert_with(|| nearest_continental_id(center_hex_q, center_hex_r, seed, noise))
    })
}

// ──── Regional Plates ────

/// Generate a regional plate. Returns None for skipped cells.
fn regional_plate_for_cell(cell_q: i32, cell_r: i32, seed: u64, noise: &Perlin) -> Option<PlateInfo> {
    let cq = cell_q as i64;
    let cr = cell_r as i64;
    let gs = REGIONAL_GRID_SIZE as f64;
    let cell_center_q = cell_q as f64 * gs + gs / 2.0;
    let cell_center_r = cell_r as f64 * gs + gs / 2.0;

    let (cnx, cny) = hex_to_noise(cell_center_q, cell_center_r);
    let jitter_raw = noise.get([
        cnx * JITTER_NOISE_SCALE + 1000.0,
        cny * JITTER_NOISE_SCALE + 1000.0,
    ]);
    let jitter_intensity = (jitter_raw * 0.5 + 0.5).clamp(0.0, 1.0);

    // Chaotic (high jitter) → less skipping → more regional plates
    let skip_threshold = REGIONAL_MAX_SKIP
        - (REGIONAL_MAX_SKIP - REGIONAL_MIN_SKIP) * jitter_intensity;
    if hash_f64(cq, cr, seed, 6) < skip_threshold {
        return None;
    }

    let jitter_range = MIN_JITTER_FRACTION
        + (MAX_JITTER_FRACTION - MIN_JITTER_FRACTION) * jitter_intensity;
    let offset_q = (hash_f64(cq, cr, seed, 0) - 0.5) * 2.0 * jitter_range * gs;
    let offset_r = (hash_f64(cq, cr, seed, 1) - 0.5) * 2.0 * jitter_range * gs;
    let center_q = cell_center_q + offset_q;
    let center_r = cell_center_r + offset_r;
    let (center_x, center_y) = hex_to_cartesian(center_q, center_r);

    // Regional plates contribute boundary effects only — no independent base
    // elevation. A tile's base elevation comes exclusively from its continental
    // plate. is_continental is likewise irrelevant at regional scale.

    let regional_angle = noise.get([
        cnx * DRIFT_NOISE_SCALE + 3333.0,
        cny * DRIFT_NOISE_SCALE + 4444.0,
    ]) * std::f64::consts::TAU;
    let local_variation = (hash_f64(cq, cr, seed, 4) - 0.5) * std::f64::consts::PI;
    let drift_angle = regional_angle + local_variation;
    let drift_magnitude = 0.5 + hash_f64(cq, cr, seed, 5) * 0.5;

    Some(PlateInfo {
        cell_q, cell_r,
        center_hex_q: center_q, center_hex_r: center_r,
        center_x, center_y, base_elevation: 0.0,
        drift_x: drift_angle.cos() * drift_magnitude,
        drift_y: drift_angle.sin() * drift_magnitude,
        is_continental: false,
    })
}

// ──── Boundary Classification & Elevation ────

fn classify_boundary(plate: &PlateInfo, neighbor: &PlateInfo) -> BoundaryClassification {
    let dx = neighbor.center_x - plate.center_x;
    let dy = neighbor.center_y - plate.center_y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-10 {
        return BoundaryClassification {
            kind: BoundaryKind::Transform, intensity: 0.0,
            perpendicular: 0.0, parallel: 0.0,
        };
    }
    let nx = dx / len;
    let ny = dy / len;

    let rel_x = plate.drift_x - neighbor.drift_x;
    let rel_y = plate.drift_y - neighbor.drift_y;
    let perpendicular = rel_x * nx + rel_y * ny;
    let parallel = -rel_x * ny + rel_y * nx;

    const TRANSFORM_THRESHOLD: f64 = 0.15;
    if perpendicular > TRANSFORM_THRESHOLD {
        BoundaryClassification {
            kind: BoundaryKind::Convergent,
            intensity: ((perpendicular - TRANSFORM_THRESHOLD) / 1.5).min(1.0),
            perpendicular, parallel,
        }
    } else if perpendicular < -TRANSFORM_THRESHOLD {
        BoundaryClassification {
            kind: BoundaryKind::Divergent,
            intensity: ((-perpendicular - TRANSFORM_THRESHOLD) / 1.5).min(1.0),
            perpendicular, parallel,
        }
    } else {
        BoundaryClassification {
            kind: BoundaryKind::Transform,
            intensity: parallel.abs().min(1.0),
            perpendicular, parallel,
        }
    }
}

fn boundary_elevation(
    kind: &BoundaryKind,
    scale: BoundaryScale,
    intensity: f64,
    dist_tiles: f64,
    noise: &Perlin,
    px: f64,
    py: f64,
    my_base: f64,
    neighbor_base: f64,
    junction_factor: f64,
    perpendicular: f64,
    parallel: f64,
) -> f64 {
    // Per-tile intensity variation: breaks up uniform polygon outlines into
    // peaks and passes along a boundary. Dedicated noise offset (210k/250k)
    // decorrelates from surface texture and domain warp.
    let variation_scale = match scale {
        BoundaryScale::Continental => CONT_INTENSITY_NOISE_SCALE,
        BoundaryScale::Regional => REG_INTENSITY_NOISE_SCALE,
    };
    let variation = 0.5 + 0.5 * noise.get([
        px * variation_scale + 210_000.0,
        py * variation_scale + 250_000.0,
    ]);

    match kind {
        BoundaryKind::Convergent | BoundaryKind::Divergent => {
            let (max_dist, peak_elev) = match (kind, scale) {
                (BoundaryKind::Convergent, BoundaryScale::Continental) => (CONT_CONVERGENT_MAX_DIST, CONT_CONVERGENT_PEAK),
                (BoundaryKind::Divergent, BoundaryScale::Continental) => (CONT_DIVERGENT_MAX_DIST, CONT_DIVERGENT_PEAK),
                (BoundaryKind::Convergent, BoundaryScale::Regional) => (REG_CONVERGENT_MAX_DIST, REG_CONVERGENT_PEAK),
                (BoundaryKind::Divergent, BoundaryScale::Regional) => (REG_DIVERGENT_MAX_DIST, REG_DIVERGENT_PEAK),
                _ => unreachable!(),
            };

            if dist_tiles >= max_dist {
                return 0.0;
            }

            let t = dist_tiles / max_dist;
            let falloff = 1.0 - t * t;

            match kind {
                BoundaryKind::Convergent => {
                    let uplift = peak_elev * intensity * variation * junction_factor;
                    let shared_peak = my_base.max(neighbor_base) + uplift;
                    (shared_peak - my_base) * falloff
                }
                BoundaryKind::Divergent => {
                    let depression = peak_elev.abs() * intensity * variation * junction_factor;
                    let shared_valley = my_base.min(neighbor_base) - depression;
                    (shared_valley - my_base) * falloff
                }
                _ => unreachable!(),
            }
        }
        BoundaryKind::Transform => {
            transform_crumple(
                scale, dist_tiles, noise, px, py,
                my_base, neighbor_base, junction_factor, variation,
                perpendicular, parallel,
            )
        }
    }
}

/// Transform boundary crumple zone: shear stress creates ridges and basins.
///
/// Two components:
/// 1. **Gap-aware bias**: transpression (positive perpendicular) raises the shared
///    target, transtension (negative) lowers it. Falls off with distance from boundary.
/// 2. **Disruption noise**: high-frequency crumple texture driven by total shear energy
///    (parallel + perpendicular magnitudes). Creates irregular ridges and troughs in a
///    band along the fault.
fn transform_crumple(
    scale: BoundaryScale,
    dist_tiles: f64,
    noise: &Perlin,
    px: f64,
    py: f64,
    my_base: f64,
    neighbor_base: f64,
    junction_factor: f64,
    variation: f64,
    perpendicular: f64,
    parallel: f64,
) -> f64 {
    let (falloff_dist, crumple_width, bias_scale, disruption_scale, noise_scale) = match scale {
        BoundaryScale::Continental => (
            CONT_TRANSFORM_MAX_DIST, CONT_TRANSFORM_CRUMPLE_WIDTH,
            CONT_TRANSFORM_BIAS_SCALE, CONT_TRANSFORM_DISRUPTION_SCALE,
            CONT_TRANSFORM_NOISE_SCALE,
        ),
        BoundaryScale::Regional => (
            REG_TRANSFORM_MAX_DIST, REG_TRANSFORM_CRUMPLE_WIDTH,
            REG_TRANSFORM_BIAS_SCALE, REG_TRANSFORM_DISRUPTION_SCALE,
            REG_TRANSFORM_NOISE_SCALE,
        ),
    };

    let effective_max = falloff_dist.max(crumple_width);
    if dist_tiles >= effective_max {
        return 0.0;
    }

    // 1. Gap-aware bias from transpression/transtension
    let bias = perpendicular * bias_scale;
    let shared_target = (my_base + neighbor_base) / 2.0 + bias;
    let gap_falloff = if dist_tiles < falloff_dist {
        let t = dist_tiles / falloff_dist;
        1.0 - t * t
    } else {
        0.0
    };
    let gap_contribution = (shared_target - my_base) * gap_falloff;

    // 2. Crumple disruption noise — high-frequency ridges and basins
    let shear_energy = parallel.abs() + perpendicular.abs();
    let crumple_amplitude = shear_energy * disruption_scale;
    let disruption = noise.get([
        px * noise_scale + 310_000.0,
        py * noise_scale + 350_000.0,
    ]);
    let crumple_falloff = if dist_tiles < crumple_width {
        let t = dist_tiles / crumple_width;
        1.0 - t * t
    } else {
        0.0
    };
    let crumple = disruption * crumple_amplitude * crumple_falloff;

    (gap_contribution + crumple) * variation * junction_factor
}

/// Compute perpendicular-bisector distance between two plates.
fn voronoi_boundary(
    plate: &PlateInfo,
    neighbor: &PlateInfo,
    px: f64,
    py: f64,
) -> f64 {
    let mid_x = (plate.center_x + neighbor.center_x) / 2.0;
    let mid_y = (plate.center_y + neighbor.center_y) / 2.0;
    let ab_x = neighbor.center_x - plate.center_x;
    let ab_y = neighbor.center_y - plate.center_y;
    let ab_len = (ab_x * ab_x + ab_y * ab_y).sqrt();

    if ab_len > 1e-10 {
        let dot = (px - mid_x) * ab_x + (py - mid_y) * ab_y;
        dot.abs() / ab_len
    } else {
        0.0
    }
}

// ──── Terrain Generator ────

pub struct Terrain {
    seed: u64,
    noise: Perlin,
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self { seed, noise: Perlin::new(seed as u32) }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn get_height(&self, q: i32, r: i32) -> i32 {
        self.evaluate(q, r).height
    }

    pub fn evaluate(&self, q: i32, r: i32) -> TerrainEval {
        let (px, py) = hex_to_cartesian(q as f64, r as f64);

        // ── Step 1: Continental evaluation (unchanged from original) ──
        let cont = continental_eval(q, r, self.seed, &self.noise);
        let cont_dist_tiles = cont.dist_to_boundary / HEX_SPACING;

        let base = cont.plate.base_elevation;

        // Continental boundary contribution (gap-aware, dampened at triple junctions)
        let cont_class = classify_boundary(&cont.plate, &cont.neighbor);
        let cont_elev = boundary_elevation(
            &cont_class.kind, BoundaryScale::Continental, cont_class.intensity, cont_dist_tiles,
            &self.noise, px, py,
            cont.plate.base_elevation, cont.neighbor.base_elevation, cont.junction_factor,
            cont_class.perpendicular, cont_class.parallel,
        );

        // ── Step 2: Regional evaluation (scoped to continental plate) ──
        let cont_id = (cont.plate.cell_q, cont.plate.cell_r);
        let regional_seed = self.seed ^ REGIONAL_SEED_OFFSET;
        let (rwq, rwr) = regional_warp(q as f64, r as f64, &self.noise);
        let (reg_px, reg_py) = hex_to_cartesian(rwq, rwr);
        let micro_cq = (rwq / REGIONAL_GRID_SIZE as f64).floor() as i32;
        let micro_cr = (rwr / REGIONAL_GRID_SIZE as f64).floor() as i32;

        let mut reg_nearest = (f64::MAX, None::<PlateInfo>);
        let mut reg_second = (f64::MAX, None::<PlateInfo>);
        let mut reg_third_dist = f64::MAX;

        for dq in -4..=4 {
            for dr in -4..=4 {
                if let Some(plate) = regional_plate_for_cell(
                    micro_cq + dq, micro_cr + dr, regional_seed, &self.noise,
                ) {
                    // Only consider regional plates within the same continental plate
                    let parent = cached_continental_id(
                        micro_cq + dq, micro_cr + dr,
                        plate.center_hex_q, plate.center_hex_r, self.seed,
                        &self.noise,
                    );
                    if parent != cont_id {
                        continue;
                    }

                    let d2 = hex_dist_sq(rwq - plate.center_hex_q, rwr - plate.center_hex_r);
                    if d2 < reg_nearest.0 {
                        reg_third_dist = reg_second.0;
                        reg_second = reg_nearest;
                        reg_nearest = (d2, Some(plate));
                    } else if d2 < reg_second.0 {
                        reg_third_dist = reg_second.0;
                        reg_second = (d2, Some(plate));
                    } else if d2 < reg_third_dist {
                        reg_third_dist = d2;
                    }
                }
            }
        }

        // Regional boundary contribution (dampened at triple junctions)
        let (regional_plate, regional_boundary, regional_elev) =
            match (reg_nearest.1, reg_second.1) {
                (Some(plate), Some(neighbor)) => {
                    let dist = voronoi_boundary(&plate, &neighbor, reg_px, reg_py);
                    let dist_tiles = dist / HEX_SPACING;
                    let reg_class = classify_boundary(&plate, &neighbor);
                    let d2_sqrt = reg_second.0.sqrt();
                    let reg_junction = if d2_sqrt > 1e-10 {
                        ((reg_third_dist.sqrt() - d2_sqrt) / d2_sqrt).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let elev = boundary_elevation(
                        &reg_class.kind, BoundaryScale::Regional, reg_class.intensity, dist_tiles,
                        &self.noise, px, py,
                        plate.base_elevation, neighbor.base_elevation, reg_junction,
                        reg_class.perpendicular, reg_class.parallel,
                    );
                    (
                        Some(PlateId { cell_q: plate.cell_q, cell_r: plate.cell_r }),
                        Some(BoundaryInfo { kind: reg_class.kind, intensity: reg_class.intensity, distance: dist_tiles }),
                        elev,
                    )
                }
                (Some(plate), None) => (
                    Some(PlateId { cell_q: plate.cell_q, cell_r: plate.cell_r }),
                    None,
                    0.0,
                ),
                _ => (None, None, 0.0),
            };

        // ── Combine ──
        let height = (base + cont_elev + regional_elev).round() as i32;

        TerrainEval {
            height,
            continental_plate: PlateId {
                cell_q: cont.plate.cell_q,
                cell_r: cont.plate.cell_r,
            },
            is_continental: cont.plate.is_continental,
            base_elevation: base,
            continental_boundary: BoundaryInfo {
                kind: cont_class.kind,
                intensity: cont_class.intensity,
                distance: cont_dist_tiles,
            },
            continental_neighbor: PlateId {
                cell_q: cont.neighbor.cell_q,
                cell_r: cont.neighbor.cell_r,
            },
            continental_neighbor_is_continental: cont.neighbor.is_continental,
            regional_plate,
            regional_boundary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_inputs() {
        let t = Terrain::new(42);
        assert_eq!(t.get_height(0, 0), t.get_height(0, 0));
        assert_eq!(t.get_height(100, -50), t.get_height(100, -50));
        assert_eq!(t.get_height(-5000, 3000), t.get_height(-5000, 3000));
    }

    #[test]
    fn deterministic_across_instances() {
        let t1 = Terrain::new(42);
        let t2 = Terrain::new(42);
        assert_eq!(t1.get_height(0, 0), t2.get_height(0, 0));
        assert_eq!(t1.get_height(1000, -2000), t2.get_height(1000, -2000));
        assert_eq!(t1.get_height(-3333, 7777), t2.get_height(-3333, 7777));
    }

    #[test]
    fn different_seeds_differ() {
        let t1 = Terrain::new(42);
        let t2 = Terrain::new(99);
        let diffs = (0..100)
            .filter(|&i| t1.get_height(i * 100, 0) != t2.get_height(i * 100, 0))
            .count();
        assert!(diffs > 50, "Expected mostly different heights, got {diffs}/100");
    }

    #[test]
    fn hash_deterministic() {
        assert_eq!(hash_u64(1, 2, 42), hash_u64(1, 2, 42));
        assert_ne!(hash_u64(1, 2, 42), hash_u64(1, 3, 42));
    }

    #[test]
    fn hash_f64_in_unit_range() {
        for a in -10..10 {
            for b in -10..10 {
                let v = hash_f64(a, b, 42, 0);
                assert!((0.0..1.0).contains(&v), "Expected [0,1), got {v}");
            }
        }
    }

    #[test]
    fn continental_plates_always_active() {
        let noise = Perlin::new(42);
        for cq in -3..3 {
            for cr in -3..3 {
                let _ = continental_plate_for_cell(cq, cr, 42, &noise);
                // No Option — always succeeds
            }
        }
    }

    #[test]
    fn some_regional_cells_are_skipped() {
        let noise = Perlin::new(42);
        let regional_seed = 42u64 ^ REGIONAL_SEED_OFFSET;
        let total = 400;
        let active: usize = (-10..10)
            .flat_map(|cq| (-10..10).map(move |cr| (cq, cr)))
            .filter(|&(cq, cr)| regional_plate_for_cell(cq, cr, regional_seed, &noise).is_some())
            .count();
        assert!(active < total, "Expected some skipping");
        assert!(active > total / 4, "Expected at least 25% active, got {active}/{total}");
    }

    #[test]
    fn convergent_boundary_adds_elevation() {
        let a = PlateInfo {
            cell_q: 0, cell_r: 0, center_hex_q: 0.0, center_hex_r: 0.0,
            center_x: 0.0, center_y: 0.0, base_elevation: 500.0,
            drift_x: 1.0, drift_y: 0.0, is_continental: true,
        };
        let b = PlateInfo {
            cell_q: 1, cell_r: 0, center_hex_q: 100.0, center_hex_r: 0.0,
            center_x: 100.0, center_y: 0.0, base_elevation: 500.0,
            drift_x: -1.0, drift_y: 0.0, is_continental: true,
        };
        let c = classify_boundary(&a, &b);
        assert_eq!(c.kind, BoundaryKind::Convergent);
        let elev = boundary_elevation(
            &c.kind, BoundaryScale::Continental, c.intensity, 0.0,
            &Perlin::new(0), 50.0, 0.0, 500.0, 500.0, 1.0,
            c.perpendicular, c.parallel,
        );
        assert!(elev > 0.0, "Convergent should add elevation, got {elev}");
    }

    #[test]
    fn divergent_boundary_subtracts_elevation() {
        let a = PlateInfo {
            cell_q: 0, cell_r: 0, center_hex_q: 0.0, center_hex_r: 0.0,
            center_x: 0.0, center_y: 0.0, base_elevation: 500.0,
            drift_x: -1.0, drift_y: 0.0, is_continental: true,
        };
        let b = PlateInfo {
            cell_q: 1, cell_r: 0, center_hex_q: 100.0, center_hex_r: 0.0,
            center_x: 100.0, center_y: 0.0, base_elevation: 500.0,
            drift_x: 1.0, drift_y: 0.0, is_continental: true,
        };
        let c = classify_boundary(&a, &b);
        assert_eq!(c.kind, BoundaryKind::Divergent);
        let elev = boundary_elevation(
            &c.kind, BoundaryScale::Continental, 1.0, 0.0,
            &Perlin::new(0), 50.0, 0.0, 500.0, 500.0, 1.0,
            c.perpendicular, c.parallel,
        );
        assert!(elev < 0.0, "Divergent should subtract elevation, got {elev}");
    }

    #[test]
    fn boundary_zero_at_max_distance() {
        let noise = Perlin::new(0);
        let elev = boundary_elevation(
            &BoundaryKind::Convergent, BoundaryScale::Continental, 1.0,
            CONT_CONVERGENT_MAX_DIST,
            &noise, 0.0, 0.0, 500.0, 500.0, 1.0,
            0.0, 0.0,
        );
        assert_eq!(elev, 0.0);
    }

    // ── Transform crumple zone tests ──

    /// Helper: create two plates sliding past each other (pure shear along x-axis).
    /// Plate A at x=0 drifts +y, plate B at x=100 drifts -y.
    /// Normal is (1,0), relative drift is (0, 2). perpendicular ≈ 0, parallel ≈ 2.
    fn make_transform_plates(perpendicular_bias: f64) -> (PlateInfo, PlateInfo) {
        let a = PlateInfo {
            cell_q: 0, cell_r: 0, center_hex_q: 0.0, center_hex_r: 0.0,
            center_x: 0.0, center_y: 0.0, base_elevation: 300.0,
            drift_x: perpendicular_bias / 2.0, drift_y: 1.0, is_continental: true,
        };
        let b = PlateInfo {
            cell_q: 1, cell_r: 0, center_hex_q: 100.0, center_hex_r: 0.0,
            center_x: 100.0, center_y: 0.0, base_elevation: 500.0,
            drift_x: -perpendicular_bias / 2.0, drift_y: -1.0, is_continental: true,
        };
        (a, b)
    }

    #[test]
    fn classify_transform_pure_shear() {
        let (a, b) = make_transform_plates(0.0);
        let c = classify_boundary(&a, &b);
        assert_eq!(c.kind, BoundaryKind::Transform);
        assert!(c.perpendicular.abs() < 0.01, "pure shear should have near-zero perpendicular: {}", c.perpendicular);
        assert!(c.parallel.abs() > 1.0, "pure shear should have large parallel: {}", c.parallel);
    }

    #[test]
    fn classify_returns_perpendicular_and_parallel() {
        // Convergent: strong perpendicular, some parallel
        let a = PlateInfo {
            cell_q: 0, cell_r: 0, center_hex_q: 0.0, center_hex_r: 0.0,
            center_x: 0.0, center_y: 0.0, base_elevation: 400.0,
            drift_x: 0.8, drift_y: 0.5, is_continental: true,
        };
        let b = PlateInfo {
            cell_q: 1, cell_r: 0, center_hex_q: 100.0, center_hex_r: 0.0,
            center_x: 100.0, center_y: 0.0, base_elevation: 400.0,
            drift_x: -0.8, drift_y: -0.5, is_continental: true,
        };
        let c = classify_boundary(&a, &b);
        assert_eq!(c.kind, BoundaryKind::Convergent);
        assert!(c.perpendicular > 0.0, "convergent should have positive perpendicular");
        // parallel is the shear residual — nonzero because drift has a y-component
        // but classification is convergent because perpendicular dominates
    }

    #[test]
    fn transpression_creates_uplift_bias() {
        // Small positive perpendicular (plates slightly converging while shearing)
        let (a, b) = make_transform_plates(0.1);
        let c = classify_boundary(&a, &b);
        assert_eq!(c.kind, BoundaryKind::Transform, "should classify as transform with small perpendicular");

        let noise = Perlin::new(0);
        // For plate A (base 300, neighbor 500): shared_target = 400 + bias > 400
        // gap_contribution = (shared_target - 300) * falloff > 0
        // Isolate bias by zeroing parallel (no disruption noise)
        let elev_no_shear = transform_crumple(
            BoundaryScale::Continental, 0.0, &noise, 50.0, 0.0,
            300.0, 500.0, 1.0, 1.0, c.perpendicular, 0.0,
        );
        // With transpression bias, gap should close toward the higher side + some uplift
        assert!(elev_no_shear > 0.0, "transpression should produce positive contribution for lower plate, got {elev_no_shear}");
    }

    #[test]
    fn transtension_creates_depression_bias() {
        // Small negative perpendicular (plates slightly diverging while shearing)
        let (a, b) = make_transform_plates(-0.1);
        let c = classify_boundary(&a, &b);
        assert_eq!(c.kind, BoundaryKind::Transform);

        let noise = Perlin::new(0);
        // For the HIGHER plate (base 500, neighbor 300):
        // shared_target = 400 + (negative bias) < 400
        // gap_contribution = (shared_target - 500) < 0 → depression
        let elev_no_shear = transform_crumple(
            BoundaryScale::Continental, 0.0, &noise, 50.0, 0.0,
            500.0, 300.0, 1.0, 1.0, -c.perpendicular, 0.0,
        );
        assert!(elev_no_shear < 0.0, "transtension should depress higher plate, got {elev_no_shear}");
    }

    #[test]
    fn crumple_amplitude_scales_with_shear() {
        let noise = Perlin::new(0);
        let low_shear = transform_crumple(
            BoundaryScale::Continental, 100.0, &noise, 50.0, 0.0,
            400.0, 400.0, 1.0, 1.0, 0.0, 0.3,
        );
        let high_shear = transform_crumple(
            BoundaryScale::Continental, 100.0, &noise, 50.0, 0.0,
            400.0, 400.0, 1.0, 1.0, 0.0, 0.9,
        );
        // Equal base elevations → no gap contribution → pure crumple noise
        // Higher shear = larger amplitude
        assert!(high_shear.abs() > low_shear.abs(),
            "higher shear should produce larger crumple: low={low_shear:.2}, high={high_shear:.2}");
    }

    #[test]
    fn crumple_zero_beyond_effective_max() {
        let noise = Perlin::new(0);
        let effective_max = CONT_TRANSFORM_MAX_DIST.max(CONT_TRANSFORM_CRUMPLE_WIDTH);
        let elev = transform_crumple(
            BoundaryScale::Continental, effective_max, &noise, 50.0, 0.0,
            300.0, 500.0, 1.0, 1.0, 0.05, 0.8,
        );
        assert_eq!(elev, 0.0, "should be zero at effective max distance");
    }

    #[test]
    fn crumple_decreases_with_distance() {
        let noise = Perlin::new(0);
        let near = transform_crumple(
            BoundaryScale::Continental, 10.0, &noise, 50.0, 0.0,
            400.0, 400.0, 1.0, 1.0, 0.0, 0.8,
        );
        let far = transform_crumple(
            BoundaryScale::Continental, 400.0, &noise, 50.0, 0.0,
            400.0, 400.0, 1.0, 1.0, 0.0, 0.8,
        );
        // Equal bases → pure crumple. Near should have higher amplitude than far.
        assert!(near.abs() > far.abs(),
            "crumple should decrease with distance: near={near:.2}, far={far:.2}");
    }

    #[test]
    fn transform_max_distance_reflects_crumple_zone() {
        // max_distance() should return the larger of the two influence distances
        let cont_max = BoundaryKind::Transform.max_distance(BoundaryScale::Continental);
        assert!(cont_max >= CONT_TRANSFORM_CRUMPLE_WIDTH,
            "continental max_distance should cover crumple width");
        assert!(cont_max >= CONT_TRANSFORM_MAX_DIST,
            "continental max_distance should cover gap falloff");

        let reg_max = BoundaryKind::Transform.max_distance(BoundaryScale::Regional);
        assert!(reg_max >= REG_TRANSFORM_CRUMPLE_WIDTH,
            "regional max_distance should cover crumple width");
    }

    #[test]
    fn evaluate_consistent() {
        let t = Terrain::new(42);
        for q in (-50..50).step_by(7) {
            for r in (-50..50).step_by(7) {
                assert_eq!(t.get_height(q, r), t.evaluate(q, r).height);
            }
        }
    }

    #[test]
    fn voronoi_always_finds_plates() {
        let t = Terrain::new(42);
        for q in (-15000..15000).step_by(500) {
            for r in (-15000..15000).step_by(500) {
                // Just verify evaluate doesn't panic — no range assertion
                let _ = t.evaluate(q, r);
            }
        }
    }
}
