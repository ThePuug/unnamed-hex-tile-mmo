use std::collections::HashMap;

use common::{ArrayVec, PlateTag, Tagged, MAX_PLATE_TAGS};
use crate::noise::{hash_u64, hash_f64, simplex_2d};
use crate::{MACRO_CELL_SIZE, JITTER_NOISE_WAVELENGTH, JITTER_MIN, JITTER_MAX,
            SUPPRESSION_RATE_MIN, SUPPRESSION_RATE_MAX, OCEAN_SUPPRESSION_BOOST,
            REGIME_LAND_THRESHOLD,
            WARP_NOISE_WAVELENGTH, WARP_PRIME_B, WARP_PRIME_C, WARP_PRIME_D,
            WARP_STRENGTH_MIN, WARP_STRENGTH_MAX,
            GRAD_STEP, REGIME_SIGMOID_MIDPOINT, REGIME_SIGMOID_STEEPNESS, MAX_ELONGATION,
            COASTAL_WARP_THRESHOLD,
            WORLD_GATE_SIGMOID_MIDPOINT, WORLD_GATE_SIGMOID_STEEPNESS,
            CONTINENT_CELL_SIZE, CONTINENT_JITTER,
            CONTINENT_WARP_AMPLITUDE, CONTINENT_WARP_WAVELENGTH,
            REGIONAL_CHARACTER_WAVELENGTH, REGIONAL_MOD_MIN, REGIONAL_MOD_MAX};

/// Row height factor for hex grid: sqrt(3)/2.
/// Odd rows are shifted right by half a cell width.
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

const SUPPRESS_SEED: u64 = 0xDEAD_CAFE_0000;

/// A macro plate center with position, grid cell identity, and unique ID.
#[derive(Clone, Debug, PartialEq)]
pub struct PlateCenter {
    pub wx: f64,
    pub wy: f64,
    pub cell_q: i32,
    pub cell_r: i32,
    pub id: u64,
    /// Tags assigned by generation and the event system. Starts empty;
    /// populated by [`PlateCache::classify_tags`].
    pub tags: ArrayVec<[PlateTag; MAX_PLATE_TAGS]>,
    /// Ground-level elevation in world units. Starts at 0.0;
    /// written by terrain events (e.g. continental spine generation).
    pub elevation: f64,
}

impl Tagged for PlateCenter {
    fn tags(&self) -> &ArrayVec<[PlateTag; MAX_PLATE_TAGS]> { &self.tags }
    fn tags_mut(&mut self) -> &mut ArrayVec<[PlateTag; MAX_PLATE_TAGS]> { &mut self.tags }
}

// ──── Hex grid helpers ────


// ──── Grid cell → plate center ────

/// Jitter factor at a world position. Simplex noise at very low frequency
/// modulates how far plate centers deviate from cell centers.
/// Returns a value in [JITTER_MIN, JITTER_MAX].
fn jitter_at(wx: f64, wy: f64, seed: u64) -> f64 {
    let noise_seed = seed ^ 0xA1B2C3D4E5F6;
    let n = simplex_2d(wx / JITTER_NOISE_WAVELENGTH, wy / JITTER_NOISE_WAVELENGTH, noise_seed);
    let t = (n + 1.0) * 0.5;
    JITTER_MIN + t * (JITTER_MAX - JITTER_MIN)
}

// ──── Warped Voronoi distance ────

const WARP_NOISE_SEED: u64 = 0xCCCC_DDDD_0001;
const WARP_STRENGTH_SEED_B: u64 = 0xCCCC_DDDD_0003;
const WARP_STRENGTH_SEED_C: u64 = 0xCCCC_DDDD_0004;
const WARP_STRENGTH_SEED_D: u64 = 0xCCCC_DDDD_0005;
const CONTINENT_JITTER_SEED_X: u64 = 0xBEEF_FACE_0001;
const CONTINENT_JITTER_SEED_Y: u64 = 0xBEEF_FACE_0002;
const CONTINENT_WARP_SEED_X: u64   = 0xBEEF_FACE_0003;
const CONTINENT_WARP_SEED_Y: u64   = 0xBEEF_FACE_0004;
const REGIONAL_MOD_SEED: u64       = 0xBEEF_FACE_0005;

/// Controls how quickly local fBm is attenuated toward continental interiors.
/// Higher = local detail persists deeper inland before smoothing.
/// Lower = smoothing kicks in closer to the coast.
/// 2.0: moderate — local fBm mostly gone by mid-interior
/// 3.0: gentle — local detail extends further inland
/// 1.0: aggressive — only the immediate coastline gets full local variation
const INTERIOR_SMOOTHING: f64 = 1.0;

// Local fBm weights. The cellular world gate defines continental topology; B/C/D add
// coastal texture (bays, peninsulas, island chains) at the continental margins.
const LOCAL_WEIGHT_B: f64 = 1.0;
const LOCAL_WEIGHT_C: f64 = 0.5;
const LOCAL_WEIGHT_D: f64 = 0.5;
const LOCAL_DIVISOR: f64 = LOCAL_WEIGHT_B + LOCAL_WEIGHT_C + LOCAL_WEIGHT_D; // 2.0 — normalized

// ──── Cellular world gate ────

/// Convert world position to continental hex grid cell (odd-r offset).
fn continent_cell_for(wx: f64, wy: f64) -> (i32, i32) {
    let cr = (wy / (CONTINENT_CELL_SIZE * HEX_ROW_HEIGHT)).round() as i32;
    let odd_shift = if cr & 1 != 0 { CONTINENT_CELL_SIZE * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / CONTINENT_CELL_SIZE).round() as i32;
    (cq, cr)
}

/// World position of the continental feature point for hex cell (cq, cr).
/// Jitter displaces the point up to CONTINENT_JITTER × CONTINENT_CELL_SIZE from the cell center.
fn continent_seed_point(cq: i32, cr: i32, seed: u64) -> (f64, f64) {
    let odd_shift = if cr & 1 != 0 { CONTINENT_CELL_SIZE * 0.5 } else { 0.0 };
    let cx = cq as f64 * CONTINENT_CELL_SIZE + odd_shift;
    let cy = cr as f64 * CONTINENT_CELL_SIZE * HEX_ROW_HEIGHT;
    // hash_f64 ∈ [0, 1) → remap to [-1, 1] for symmetric displacement
    let jx = (hash_f64(cq as i64, cr as i64, seed ^ CONTINENT_JITTER_SEED_X) * 2.0 - 1.0)
              * CONTINENT_JITTER * CONTINENT_CELL_SIZE;
    let jy = (hash_f64(cq as i64, cr as i64, seed ^ CONTINENT_JITTER_SEED_Y) * 2.0 - 1.0)
              * CONTINENT_JITTER * CONTINENT_CELL_SIZE;
    (cx + jx, cy + jy)
}

/// Cellular (Worley) world gate: 1.0 at continent centers, 0.0 at ocean midpoints.
///
/// Inverted F1 Voronoi on a jittered hex lattice with domain warp.
/// Domain warp displaces the query point before Voronoi lookup, creating irregular
/// coastlines (peninsulas, bays) without distorting seed positions.
/// The 7-cell check (self + 6 hex neighbors) finds the nearest feature point.
fn cellular_world_gate(wx: f64, wy: f64, seed: u64) -> f64 {
    // Domain warp: displace query point for irregular coastline shapes.
    let warp_x = simplex_2d(wx / CONTINENT_WARP_WAVELENGTH, wy / CONTINENT_WARP_WAVELENGTH,
                            seed ^ CONTINENT_WARP_SEED_X) * CONTINENT_WARP_AMPLITUDE;
    let warp_y = simplex_2d(wx / CONTINENT_WARP_WAVELENGTH, wy / CONTINENT_WARP_WAVELENGTH,
                            seed ^ CONTINENT_WARP_SEED_Y) * CONTINENT_WARP_AMPLITUDE;
    let qx = wx + warp_x;
    let qy = wy + warp_y;

    let (cq, cr) = continent_cell_for(qx, qy);

    // 6 hex neighbors in odd-r offset; which side the diagonal neighbors fall on
    // depends on whether the current row is even or odd.
    let neighbors: [(i32, i32); 6] = if cr & 1 == 0 {
        [(cq-1, cr), (cq+1, cr), (cq-1, cr-1), (cq, cr-1), (cq-1, cr+1), (cq, cr+1)]
    } else {
        [(cq-1, cr), (cq+1, cr), (cq,   cr-1), (cq+1, cr-1), (cq, cr+1), (cq+1, cr+1)]
    };

    let mut min_dist_sq = {
        let (fx, fy) = continent_seed_point(cq, cr, seed);
        let dx = qx - fx; let dy = qy - fy;
        dx * dx + dy * dy
    };
    for &(nq, nr) in &neighbors {
        let (fx, fy) = continent_seed_point(nq, nr, seed);
        let dx = qx - fx; let dy = qy - fy;
        min_dist_sq = min_dist_sq.min(dx * dx + dy * dy);
    }

    let f1 = min_dist_sq.sqrt();
    (1.0 - f1 / (CONTINENT_CELL_SIZE * 0.5)).clamp(0.0, 1.0)
}

// ──── Regime noise ────

/// Multiplicative cellular-gated regime noise, normalized to [0, 1].
///
/// Three factors compose the final value:
/// - `world_gate`: sigmoid(cellular F1 Voronoi with domain warp) — disconnected continental topology
/// - `regional_mod`: low-frequency simplex in [REGIONAL_MOD_MIN, REGIONAL_MOD_MAX] — size variation between worlds
/// - `local`: three-wavelength fBm in [0, 1] — coastal detail (bays, peninsulas, island chains)
fn raw_regime_noise(wx: f64, wy: f64, seed: u64) -> f64 {
    let raw_gate = cellular_world_gate(wx, wy, seed);
    let world_gate = sigmoid(raw_gate, WORLD_GATE_SIGMOID_MIDPOINT, WORLD_GATE_SIGMOID_STEEPNESS);
    let b = simplex_2d(wx / WARP_PRIME_B, wy / WARP_PRIME_B, seed ^ WARP_STRENGTH_SEED_B);
    let c = simplex_2d(wx / WARP_PRIME_C, wy / WARP_PRIME_C, seed ^ WARP_STRENGTH_SEED_C);
    let d = simplex_2d(wx / WARP_PRIME_D, wy / WARP_PRIME_D, seed ^ WARP_STRENGTH_SEED_D);
    let local_raw = (b * LOCAL_WEIGHT_B + c * LOCAL_WEIGHT_C + d * LOCAL_WEIGHT_D) / LOCAL_DIVISOR; // [-1, 1]
    let local = (local_raw + 1.0) / 2.0; // [0, 1]
    // Regional character: low-frequency simplex controls world size variation.
    // Remapped from [-1,1] to [REGIONAL_MOD_MIN, REGIONAL_MOD_MAX] so every world
    // has at least some land (min > 0) but worlds vary in character.
    let raw_regional = simplex_2d(wx / REGIONAL_CHARACTER_WAVELENGTH, wy / REGIONAL_CHARACTER_WAVELENGTH,
                                  seed ^ REGIONAL_MOD_SEED);
    let regional_mod = REGIONAL_MOD_MIN + (REGIONAL_MOD_MAX - REGIONAL_MOD_MIN) * (raw_regional + 1.0) / 2.0;
    // Attenuate local fBm in continental interiors: blend toward 1.0 based on
    // raw cellular gate (pre-sigmoid). Using raw_gate decouples smoothing from the
    // sigmoid land-area tuning — raw_gate peaks at 1.0 in deep interiors regardless
    // of sigmoid midpoint/steepness.
    let effective_local = local + (1.0 - local) * raw_gate.powf(INTERIOR_SMOOTHING);
    effective_local * world_gate * regional_mod
}

/// UNCACHED — evaluates 4 simplex noise calls per invocation.
/// Use `PlateCache::regime_value_at` for the cached API surface.
///
/// Sigmoidized regime field — flat plateaus with sharp transition at midpoint.
/// Values cluster near 0 (water) and 1 (land). The sigmoid flattens deep
/// water and deep land regions, concentrating all gradient at the coastline.
pub fn regime_value_at(wx: f64, wy: f64, seed: u64) -> f64 {
    sigmoid(raw_regime_noise(wx, wy, seed), REGIME_SIGMOID_MIDPOINT, REGIME_SIGMOID_STEEPNESS)
}

/// Sigmoid contrast filter. Maps x through a smooth step centered at `midpoint`
/// with sharpness controlled by `steepness`.
pub(crate) fn sigmoid(x: f64, midpoint: f64, steepness: f64) -> f64 {
    1.0 / (1.0 + (-steepness * (x - midpoint)).exp())
}

/// Estimated max gradient of the raw (pre-sigmoid) regime noise per world unit.
/// Product-rule bound over local × world_gate × regional_mod:
///   - local: simplex sum, max gradient ≈ weight/wavelength per octave, /2 for [0,1] remap.
///     Scaled by REGIONAL_MOD_MAX since regional_mod multiplies local.
///   - world_gate: sigmoid(cellular_gate with domain warp).
///     Domain warp (amplitude A, wavelength λ_w) scales gradient by (1 + A/λ_w).
///     Sigmoid peak derivative = WORLD_GATE_SIGMOID_STEEPNESS/4.
///     Max dist from displaced query to nearest seed is CONTINENT_CELL_SIZE*0.5.
///     Scaled by REGIONAL_MOD_MAX since regional_mod multiplies world_gate.
///   - regional_mod: low-frequency simplex in [MIN, MAX]; max gradient ≈ (MAX-MIN)/(2*λ).
const RAW_GRAD_MAX: f64 =
    (LOCAL_WEIGHT_B / WARP_PRIME_B + LOCAL_WEIGHT_C / WARP_PRIME_C + LOCAL_WEIGHT_D / WARP_PRIME_D)
        / LOCAL_DIVISOR / 2.0 * REGIONAL_MOD_MAX
    + WORLD_GATE_SIGMOID_STEEPNESS / (4.0 * CONTINENT_CELL_SIZE * 0.5)
        * (1.0 + CONTINENT_WARP_AMPLITUDE / CONTINENT_WARP_WAVELENGTH) * REGIONAL_MOD_MAX
    + (REGIONAL_MOD_MAX - REGIONAL_MOD_MIN) / (REGIONAL_CHARACTER_WAVELENGTH * 2.0);

/// Estimated max gradient of the sigmoidized regime field per world unit.
/// The sigmoid's peak derivative is steepness/4 (at the midpoint), so
/// the contrasted field's max gradient is the raw max scaled by that factor.
pub(crate) const GRAD_MAX_ESTIMATE: f64 = RAW_GRAD_MAX * REGIME_SIGMOID_STEEPNESS / 4.0;

/// UNCACHED — evaluates 12 simplex noise calls per invocation (4× regime_value_at).
/// Use `PlateCache::warp_strength_at` for gradient-cached access.
///
/// Warp strength derived from gradient magnitude of the sigmoidized regime field.
/// The pre-gradient sigmoid flattens deep water and deep land, so only the
/// coastline transition band produces meaningful gradient.
/// Returns a value in [WARP_STRENGTH_MIN, WARP_STRENGTH_MAX].
pub fn warp_strength_at(wx: f64, wy: f64, seed: u64) -> f64 {
    let dx = regime_value_at(wx + GRAD_STEP, wy, seed)
           - regime_value_at(wx - GRAD_STEP, wy, seed);
    let dy = regime_value_at(wx, wy + GRAD_STEP, seed)
           - regime_value_at(wx, wy - GRAD_STEP, seed);
    let gradient_mag = (dx * dx + dy * dy).sqrt() / (2.0 * GRAD_STEP);
    let normalized = (gradient_mag / GRAD_MAX_ESTIMATE).clamp(0.0, 1.0);
    WARP_STRENGTH_MIN + normalized * (WARP_STRENGTH_MAX - WARP_STRENGTH_MIN)
}

/// Per-candidate warp noise at a world position.
/// Uses candidate ID as seed offset so each candidate has its own noise field.
/// Returns a value in approximately [-1, 1].
fn warp_noise(wx: f64, wy: f64, candidate_id: u64, seed: u64) -> f64 {
    simplex_2d(
        wx / WARP_NOISE_WAVELENGTH,
        wy / WARP_NOISE_WAVELENGTH,
        seed ^ WARP_NOISE_SEED ^ candidate_id,
    )
}

// ──── Anisotropic macro plate assignment ────

/// Precomputed anisotropy context at a query point.
/// Derived from regime gradient — coastlines stretch macro plates along the shore,
/// interiors stay isotropic.
#[derive(Clone, Copy)]
struct AnisoContext {
    across: (f64, f64),
    along: (f64, f64),
    elongation: f64,
}

impl AnisoContext {
    /// Anisotropic distance. Compresses the along-coast axis
    /// so macro plates stretch parallel to the shore.
    fn dist(&self, px: f64, py: f64, cx: f64, cy: f64) -> f64 {
        let dx = px - cx;
        let dy = py - cy;
        let d_across = dx * self.across.0 + dy * self.across.1;
        let d_along = (dx * self.along.0 + dy * self.along.1) / self.elongation;
        (d_across * d_across + d_along * d_along).sqrt()
    }
}

/// Merged regime gradient — computes warp strength and aniso context from
/// a single 4-point stencil (4 regime_value_at calls instead of 8).
#[derive(Clone, Copy)]
struct RegimeGradient {
    warp_strength: f64,
    ctx: AnisoContext,
}

impl RegimeGradient {
    fn at(wx: f64, wy: f64, seed: u64) -> Self {
        let gx = regime_value_at(wx + GRAD_STEP, wy, seed)
               - regime_value_at(wx - GRAD_STEP, wy, seed);
        let gy = regime_value_at(wx, wy + GRAD_STEP, seed)
               - regime_value_at(wx, wy - GRAD_STEP, seed);
        let raw_mag = (gx * gx + gy * gy).sqrt();

        let (across, along) = if raw_mag > 1e-12 {
            let ax = gx / raw_mag;
            let ay = gy / raw_mag;
            ((ax, ay), (-ay, ax))
        } else {
            ((1.0, 0.0), (0.0, 1.0))
        };

        let grad_mag = raw_mag / (2.0 * GRAD_STEP);
        let normalized = (grad_mag / GRAD_MAX_ESTIMATE).clamp(0.0, 1.0);

        let warp_strength = WARP_STRENGTH_MIN + normalized * (WARP_STRENGTH_MAX - WARP_STRENGTH_MIN);
        let elongation = 1.0 + (MAX_ELONGATION - 1.0) * normalized;

        Self {
            warp_strength,
            ctx: AnisoContext { across, along, elongation },
        }
    }
}

// ──── Hex chunk cache for plate lookups ────

/// Spatial cache chunk size. Sized so a 1-ring hex neighborhood
/// covers the maximum anisotropic search distance.
const PLATE_CHUNK_SIZE: f64 = MACRO_CELL_SIZE * MAX_ELONGATION; // 7200.0

/// All macro cell centers whose world position falls within one hex chunk.
struct PlateChunk {
    centers: Vec<PlateCenter>,
}

/// Convert world position to chunk hex coordinate (odd-r offset hex grid).
fn plate_chunk_coord(wx: f64, wy: f64) -> (i32, i32) {
    let row_height = PLATE_CHUNK_SIZE * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { PLATE_CHUNK_SIZE * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / PLATE_CHUNK_SIZE).round() as i32;
    (cq, cr)
}


/// Center + 6 hex neighbors (odd-r offset) for chunk lookups.
fn chunk_1ring(cr: i32) -> [(i32, i32); 7] {
    if cr & 1 == 0 {
        [(0, 0), (-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
    } else {
        [(0, 0), (-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
    }
}

/// Populate a chunk by scanning macro grid cells near it.
/// Each PlateCenter is owned by the chunk containing its world position.
fn populate_chunk(cq: i32, cr: i32, seed: u64) -> PlateChunk {
    let odd_shift = if cr & 1 != 0 { PLATE_CHUNK_SIZE * 0.5 } else { 0.0 };
    let chunk_wx = cq as f64 * PLATE_CHUNK_SIZE + odd_shift;
    let chunk_wy = cr as f64 * PLATE_CHUNK_SIZE * HEX_ROW_HEIGHT;

    let (center_mcq, center_mcr) = world_to_cell(chunk_wx, chunk_wy);
    let scan = (PLATE_CHUNK_SIZE / MACRO_CELL_SIZE) as i32 + 2;

    let mut centers = Vec::new();
    for dr in -scan..=scan {
        for dq in -scan..=scan {
            if let Some(plate) = plate_center_for_cell(center_mcq + dq, center_mcr + dr, seed) {
                if plate_chunk_coord(plate.wx, plate.wy) == (cq, cr) {
                    centers.push(plate);
                }
            }
        }
    }

    PlateChunk { centers }
}

/// Effective distance for warped Voronoi assignment.
/// Uses anisotropic geometric distance + per-candidate noise perturbation.
fn effective_distance(wx: f64, wy: f64, candidate: &PlateCenter, strength: f64, ctx: &AnisoContext, seed: u64) -> f64 {
    let geo = ctx.dist(wx, wy, candidate.wx, candidate.wy);
    let noise = warp_noise(wx, wy, candidate.id, seed);
    geo + noise * strength
}

// ──── Grid cell → plate center ────

/// Compute the plate center for a specific hex grid cell (odd-r offset).
/// Returns None if the cell is suppressed.
pub(crate) fn plate_center_for_cell(cell_q: i32, cell_r: i32, seed: u64) -> Option<PlateCenter> {
    let odd_shift = if cell_r & 1 != 0 { MACRO_CELL_SIZE * 0.5 } else { 0.0 };
    let nominal_wx = cell_q as f64 * MACRO_CELL_SIZE + odd_shift;
    let nominal_wy = cell_r as f64 * MACRO_CELL_SIZE * HEX_ROW_HEIGHT;

    // Variable suppression: low at coastlines (many small plates),
    // high in deep water/land (fewer, larger plates).
    // Asymmetric: ocean side is boosted so deep ocean suppresses more than deep land.
    let hash = hash_f64(cell_q as i64, cell_r as i64, seed ^ SUPPRESS_SEED);
    let regime = regime_value_at(nominal_wx, nominal_wy, seed);
    let depth = if regime >= REGIME_LAND_THRESHOLD {
        // Land side: 0.0 at coast, 1.0 at regime=1.0
        (regime - REGIME_LAND_THRESHOLD) / (1.0 - REGIME_LAND_THRESHOLD)
    } else {
        // Ocean side: 0.0 at coast, 1.0+ at regime=0.0
        let normalized = (REGIME_LAND_THRESHOLD - regime) / REGIME_LAND_THRESHOLD;
        (normalized * OCEAN_SUPPRESSION_BOOST).min(1.0)
    };
    let suppression = SUPPRESSION_RATE_MIN + depth * (SUPPRESSION_RATE_MAX - SUPPRESSION_RATE_MIN);
    if hash < suppression {
        return None;
    }

    let jitter = jitter_at(nominal_wx, nominal_wy, seed);

    let offset_x = hash_f64(cell_q as i64, cell_r as i64, seed ^ 0x1111) - 0.5;
    let offset_y = hash_f64(cell_q as i64, cell_r as i64, seed ^ 0x2222) - 0.5;

    let wx = nominal_wx + offset_x * jitter * MACRO_CELL_SIZE;
    let wy = nominal_wy + offset_y * jitter * MACRO_CELL_SIZE;

    let id = hash_u64(cell_q as i64, cell_r as i64, seed);

    Some(PlateCenter { wx, wy, cell_q, cell_r, id, tags: ArrayVec::new(), elevation: 0.0 })
}

// ──── Grid cell lookup ────

/// Which hex grid cell contains a world position (before Voronoi assignment).
fn world_to_cell(wx: f64, wy: f64) -> (i32, i32) {
    let row_height = MACRO_CELL_SIZE * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { MACRO_CELL_SIZE * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / MACRO_CELL_SIZE).round() as i32;
    (cq, cr)
}

// ──── Public API ────

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `PlateCache::plate_at` for repeated lookups.
pub fn macro_plate_at(wx: f64, wy: f64, seed: u64) -> PlateCenter {
    PlateCache::new(seed).plate_at(wx, wy)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `PlateCache::warped_plate_at` for repeated lookups.
pub fn warped_plate_at(wx: f64, wy: f64, seed: u64) -> PlateCenter {
    PlateCache::new(seed).warped_plate_at(wx, wy)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `PlateCache::plates_in_radius` for repeated lookups.
pub fn macro_plates_in_radius(wx: f64, wy: f64, radius: f64, seed: u64) -> Vec<PlateCenter> {
    PlateCache::new(seed).plates_in_radius(wx, wy, radius)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `PlateCache::plate_neighbors` for repeated lookups.
pub fn macro_plate_neighbors(wx: f64, wy: f64, seed: u64) -> Vec<PlateCenter> {
    PlateCache::new(seed).plate_neighbors(wx, wy)
}

fn dist_sq(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

// ──── Cached API ────

/// Gradient cache grid spacing in world units. The regime gradient is smooth
/// over GRAD_STEP (100), so adjacent queries within this cell share one
/// gradient computation. Must be < GRAD_STEP to avoid sampling artifacts.
const GRAD_CACHE_CELL: f64 = 64.0;

/// Lazy cache for plate lookups. Pre-enumerates macro cell centers per chunk;
/// queries gather candidates from a 1-ring hex neighborhood (7 chunks)
/// instead of iterating individual grid cells per pixel.
pub struct PlateCache {
    chunks: HashMap<(i32, i32), PlateChunk>,
    gradients: HashMap<(i64, i64), RegimeGradient>,
    seed: u64,
}

impl PlateCache {
    pub fn new(seed: u64) -> Self {
        Self { chunks: HashMap::new(), gradients: HashMap::new(), seed }
    }

    /// Cached gradient lookup. Snaps to a coarse grid so adjacent
    /// pixels share the same gradient computation.
    fn cached_gradient(&mut self, wx: f64, wy: f64) -> (f64, AnisoContext) {
        let cx = (wx / GRAD_CACHE_CELL).floor() as i64;
        let cy = (wy / GRAD_CACHE_CELL).floor() as i64;
        let seed = self.seed;
        let rg = self.gradients.entry((cx, cy)).or_insert_with(|| {
            RegimeGradient::at(
                (cx as f64 + 0.5) * GRAD_CACHE_CELL,
                (cy as f64 + 0.5) * GRAD_CACHE_CELL,
                seed,
            )
        });
        (rg.warp_strength, rg.ctx)
    }

    /// Ensure a chunk is populated in the cache.
    fn ensure_chunk(&mut self, cq: i32, cr: i32) {
        let seed = self.seed;
        self.chunks.entry((cq, cr))
            .or_insert_with(|| populate_chunk(cq, cr, seed));
    }

    /// Cached pure Euclidean nearest-seed lookup.
    pub fn plate_at(&mut self, wx: f64, wy: f64) -> PlateCenter {
        let (cq, cr) = plate_chunk_coord(wx, wy);

        for (dq, dr) in chunk_1ring(cr) {
            self.ensure_chunk(cq + dq, cr + dr);
        }

        let mut best: Option<PlateCenter> = None;
        let mut best_dist_sq = f64::MAX;

        for (dq, dr) in chunk_1ring(cr) {
            for candidate in &self.chunks[&(cq + dq, cr + dr)].centers {
                let d = dist_sq(wx, wy, candidate.wx, candidate.wy);
                if d < best_dist_sq {
                    best = Some(candidate.clone());
                    best_dist_sq = d;
                }
            }
        }

        best.expect("no plate center found in chunk neighborhood")
    }

    /// Cached regime value at a world position. Delegates to the standalone
    /// computation (3 simplex calls). Provides a uniform cache-method API surface.
    pub fn regime_value_at(&self, wx: f64, wy: f64) -> f64 {
        regime_value_at(wx, wy, self.seed)
    }

    /// Cached warp strength via the gradient cache. Returns the warp_strength
    /// field from the cached RegimeGradient at the snapped grid position.
    pub fn warp_strength_at(&mut self, wx: f64, wy: f64) -> f64 {
        self.cached_gradient(wx, wy).0
    }

    /// All plate centers within `radius` of a world position, gathered from chunks.
    pub fn plates_in_radius(&mut self, wx: f64, wy: f64, radius: f64) -> Vec<PlateCenter> {
        let radius_sq = radius * radius;
        let row_height = PLATE_CHUNK_SIZE * HEX_ROW_HEIGHT;
        let chunk_reach = ((radius + PLATE_CHUNK_SIZE) / row_height).ceil() as i32 + 1;
        let (cq, cr) = plate_chunk_coord(wx, wy);

        let mut result = Vec::new();
        for dr in -chunk_reach..=chunk_reach {
            for dq in -chunk_reach..=chunk_reach {
                self.ensure_chunk(cq + dq, cr + dr);
                for center in &self.chunks[&(cq + dq, cr + dr)].centers {
                    if dist_sq(wx, wy, center.wx, center.wy) <= radius_sq {
                        result.push(center.clone());
                    }
                }
            }
        }
        result
    }

    /// Voronoi neighbors of the plate owning position (wx, wy).
    /// Uses midpoint sampling to test adjacency.
    pub fn plate_neighbors(&mut self, wx: f64, wy: f64) -> Vec<PlateCenter> {
        let owner = self.plate_at(wx, wy);
        let search_radius = MACRO_CELL_SIZE * 4.0;
        let candidates = self.plates_in_radius(owner.wx, owner.wy, search_radius);

        let mut neighbors = Vec::new();
        for candidate in &candidates {
            if candidate.id == owner.id { continue; }

            let mid_x = (owner.wx + candidate.wx) * 0.5;
            let mid_y = (owner.wy + candidate.wy) * 0.5;
            let at_mid = self.plate_at(mid_x, mid_y);

            if at_mid.id == owner.id || at_mid.id == candidate.id {
                neighbors.push(candidate.clone());
            }
        }
        neighbors
    }

    /// Assign Sea, Coast, or Inland tags to each plate in `plates`.
    ///
    /// - **Sea**: water plate below the regime threshold with low warp.
    /// - **Coast**: any plate with `warp_strength > COASTAL_WARP_THRESHOLD`.
    /// - **Inland**: land plate above the regime threshold with low warp.
    ///
    /// Each plate receives exactly one base tag. Call after [`Self::plates_in_radius`].
    pub fn classify_tags(&mut self, plates: &mut [PlateCenter]) {
        for plate in plates.iter_mut() {
            let regime = self.regime_value_at(plate.wx, plate.wy);
            let strength = self.warp_strength_at(plate.wx, plate.wy);
            let is_coast = strength > COASTAL_WARP_THRESHOLD;
            let tag = if is_coast {
                PlateTag::Coast
            } else if regime >= REGIME_LAND_THRESHOLD {
                PlateTag::Inland
            } else {
                PlateTag::Sea
            };
            plate.add_tag(tag);
        }
    }

    /// Cached anisotropic warped-distance plate assignment. Used by micro cell → macro
    /// assignment in the bottom-up flow. Gathers candidates from a 1-ring chunk
    /// neighborhood and uses gradient cache for anisotropy context.
    pub fn warped_plate_at(&mut self, wx: f64, wy: f64) -> PlateCenter {
        let (cq, cr) = plate_chunk_coord(wx, wy);
        let (strength, ctx) = self.cached_gradient(wx, wy);

        for (dq, dr) in chunk_1ring(cr) {
            self.ensure_chunk(cq + dq, cr + dr);
        }

        let mut best: Option<PlateCenter> = None;
        let mut best_eff_dist = f64::MAX;

        for (dq, dr) in chunk_1ring(cr) {
            for candidate in &self.chunks[&(cq + dq, cr + dr)].centers {
                let d = effective_distance(wx, wy, candidate, strength, &ctx, self.seed);
                if d < best_eff_dist {
                    best = Some(candidate.clone());
                    best_eff_dist = d;
                }
            }
        }

        best.expect("no plate center found in chunk neighborhood")
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use common::{PlateTag, Tagged};
    #[allow(unused_imports)]
    use std::collections::HashSet;

    #[test]
    fn deterministic_center_generation() {
        for cq in -10..10 {
            for cr in -10..10 {
                let a = plate_center_for_cell(cq, cr, 42);
                let b = plate_center_for_cell(cq, cr, 42);
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn different_seeds_different_centers() {
        let mut differ = 0;
        for cq in -10..10 {
            for cr in -10..10 {
                let a = plate_center_for_cell(cq, cr, 0);
                let b = plate_center_for_cell(cq, cr, 99999);
                match (&a, &b) {
                    (Some(a), Some(b)) if (a.wx - b.wx).abs() > 1e-6 || (a.wy - b.wy).abs() > 1e-6 => {
                        differ += 1;
                    }
                    (None, Some(_)) | (Some(_), None) => { differ += 1; }
                    _ => {}
                }
            }
        }
        let total = 20 * 20;
        assert!(differ > total / 2,
            "Different seeds should produce mostly different centers: {differ}/{total}");
    }

    #[test]
    fn suppressed_cells_produce_no_center() {
        let seed = 42u64;
        let mut suppressed = 0;
        let mut total = 0;
        for cq in -20..20 {
            for cr in -20..20 {
                total += 1;
                if plate_center_for_cell(cq, cr, seed).is_none() {
                    suppressed += 1;
                }
            }
        }
        let rate = suppressed as f64 / total as f64;
        // Upper bound = SUPPRESSION_RATE_MAX + 0.05 tolerance — a purely oceanic
        // test window (world gate ≈ 0) can approach MAX suppression legitimately.
        let upper = SUPPRESSION_RATE_MAX + 0.05;
        assert!(rate > SUPPRESSION_RATE_MIN && rate < upper,
            "Suppression rate {rate:.3} ({suppressed}/{total}) should be between \
             {SUPPRESSION_RATE_MIN:.2} and {upper:.2}");
    }

    #[test]
    fn macro_plate_at_returns_nearest() {
        let seed = 42u64;
        for test_x in (-5000..5000).step_by(1700) {
            for test_y in (-5000..5000).step_by(1700) {
                let wx = test_x as f64;
                let wy = test_y as f64;

                let result = macro_plate_at(wx, wy, seed);
                let result_dist = dist_sq(wx, wy, result.wx, result.wy);

                // Brute force over a wide range
                let (cq, cr) = world_to_cell(wx, wy);
                for dq in -5..=5 {
                    for dr in -5..=5 {
                        if let Some(candidate) = plate_center_for_cell(cq + dq, cr + dr, seed) {
                            let d = dist_sq(wx, wy, candidate.wx, candidate.wy);
                            assert!(result_dist <= d + 1e-6,
                                "macro_plate_at({wx}, {wy}) returned plate at ({}, {}) dist²={result_dist}, \
                                 but cell ({}, {}) has plate at ({}, {}) dist²={d}",
                                result.wx, result.wy,
                                cq + dq, cr + dr, candidate.wx, candidate.wy);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn chunk_1ring_covers_best_warped_candidate() {
        // Brute-force find the best candidate over a wide cell search,
        // then verify it lives in the chunk 1-ring.
        let seed = 42u64;
        let brute_search = (2 + MAX_ELONGATION as i32) + 2;
        for test_x in (-5000..5000).step_by(1700) {
            for test_y in (-5000..5000).step_by(1700) {
                let wx = test_x as f64;
                let wy = test_y as f64;

                let rg = RegimeGradient::at(wx, wy, seed);
                let (cq, cr) = world_to_cell(wx, wy);
                let mut best: Option<PlateCenter> = None;
                let mut best_eff = f64::MAX;
                for dr in -brute_search..=brute_search {
                    for dq in -brute_search..=brute_search {
                        if let Some(candidate) = plate_center_for_cell(cq + dq, cr + dr, seed) {
                            let d = effective_distance(wx, wy, &candidate, rg.warp_strength, &rg.ctx, seed);
                            if d < best_eff {
                                best = Some(candidate);
                                best_eff = d;
                            }
                        }
                    }
                }
                let best = best.unwrap();

                let winner_chunk = plate_chunk_coord(best.wx, best.wy);
                let (qcq, qcr) = plate_chunk_coord(wx, wy);
                let in_ring = chunk_1ring(qcr).iter().any(|&(dq, dr)| {
                    (qcq + dq, qcr + dr) == winner_chunk
                });
                assert!(in_ring,
                    "Best candidate id={} at ({:.0}, {:.0}) chunk {:?} \
                     not in 1-ring of query chunk {:?} at ({wx}, {wy})",
                    best.id, best.wx, best.wy, winner_chunk, (qcq, qcr));
            }
        }
    }

    #[test]
    fn warped_plate_at_is_deterministic() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(2000) {
            for y in (-5000..5000).step_by(2000) {
                let a = warped_plate_at(x as f64, y as f64, seed);
                let b = warped_plate_at(x as f64, y as f64, seed);
                assert_eq!(a.id, b.id);
            }
        }
    }

    #[test]
    fn warp_differs_from_pure_voronoi() {
        let seed = 42u64;
        let mut differ = 0;
        let mut total = 0;
        for x in (-15000..15000).step_by(500) {
            for y in (-15000..15000).step_by(500) {
                let wx = x as f64;
                let wy = y as f64;
                let pure = macro_plate_at(wx, wy, seed);
                let warped = warped_plate_at(wx, wy, seed);
                total += 1;
                if pure.id != warped.id {
                    differ += 1;
                }
            }
        }
        // In high-warp regions, assignments should differ from pure Voronoi
        assert!(differ > 0, "Warped assignment should differ from pure Voronoi somewhere");
        // But not everywhere — low-warp regions should be nearly identical
        let pct = differ as f64 / total as f64 * 100.0;
        assert!(pct < 50.0,
            "Warp changed {pct:.1}% of assignments — too many, expected < 50%");
    }

    #[test]
    fn no_duplicate_ids() {
        let seed = 42u64;
        let mut ids = HashSet::new();
        for cq in -50..50 {
            for cr in -50..50 {
                if let Some(plate) = plate_center_for_cell(cq, cr, seed) {
                    assert!(ids.insert(plate.id),
                        "Duplicate ID {} at cell ({cq}, {cr})", plate.id);
                }
            }
        }
    }

    #[test]
    fn jitter_modulation_creates_density_variation() {
        let seed = 42u64;
        let mut distances_stable = Vec::new();
        let mut distances_chaotic = Vec::new();

        for x in (-30000..30000).step_by(3000) {
            for y in (-30000..30000).step_by(3000) {
                let wx = x as f64;
                let wy = y as f64;
                let plate = macro_plate_at(wx, wy, seed);
                let d = dist_sq(wx, wy, plate.wx, plate.wy).sqrt();
                let j = jitter_at(wx, wy, seed);
                if j < (JITTER_MIN + JITTER_MAX) * 0.35 {
                    distances_stable.push(d);
                } else if j > (JITTER_MIN + JITTER_MAX) * 0.65 {
                    distances_chaotic.push(d);
                }
            }
        }

        assert!(!distances_stable.is_empty(), "Should find stable regions");
        assert!(!distances_chaotic.is_empty(), "Should find chaotic regions");
    }

    #[test]
    fn plates_in_radius_contains_nearest() {
        let seed = 42u64;
        let wx = 500.0;
        let wy = 500.0;
        let nearest = macro_plate_at(wx, wy, seed);
        let in_radius = macro_plates_in_radius(wx, wy, MACRO_CELL_SIZE * 2.0, seed);
        assert!(in_radius.iter().any(|p| p.id == nearest.id),
            "plates_in_radius should contain the nearest plate");
    }

    #[test]
    fn neighbors_are_symmetric() {
        let seed = 42u64;
        let owner = macro_plate_at(0.0, 0.0, seed);
        let neighbors = macro_plate_neighbors(0.0, 0.0, seed);

        assert!(!neighbors.is_empty(), "Origin plate should have neighbors");

        for neighbor in &neighbors {
            let reverse = macro_plate_neighbors(neighbor.wx, neighbor.wy, seed);
            assert!(reverse.iter().any(|p| p.id == owner.id),
                "Neighbor ({}, {}) id={} doesn't list owner id={} as its neighbor",
                neighbor.cell_q, neighbor.cell_r, neighbor.id, owner.id);
        }
    }

    #[test]
    fn neighbor_count_is_reasonable() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(2000) {
            for y in (-5000..5000).step_by(2000) {
                let neighbors = macro_plate_neighbors(x as f64, y as f64, seed);
                assert!(neighbors.len() >= 3 && neighbors.len() <= 12,
                    "Unexpected neighbor count {} at ({x}, {y})", neighbors.len());
            }
        }
    }

    #[test]
    fn typical_neighbor_count_is_six() {
        let seed = 42u64;
        let mut total_neighbors = 0;
        let mut samples = 0;
        for x in (-20000..20000).step_by(2000) {
            for y in (-20000..20000).step_by(2000) {
                let neighbors = macro_plate_neighbors(x as f64, y as f64, seed);
                total_neighbors += neighbors.len();
                samples += 1;
            }
        }
        let avg = total_neighbors as f64 / samples as f64;
        // Variable suppression creates larger plates in deep regions → fewer neighbors.
        assert!(avg >= 4.0 && avg <= 8.5,
            "Average neighbor count {avg:.2} should be near 5-7 for hex lattice with variable suppression");
    }

    #[test]
    fn edge_angles_are_not_axis_aligned() {
        let seed = 42u64;
        let mut angle_buckets = [0u32; 12];
        let mut total = 0u32;

        for x in (-15000..15000).step_by(3000) {
            for y in (-15000..15000).step_by(3000) {
                let wx = x as f64;
                let wy = y as f64;
                let owner = macro_plate_at(wx, wy, seed);
                let neighbors = macro_plate_neighbors(wx, wy, seed);

                for nbr in &neighbors {
                    let dx = nbr.wx - owner.wx;
                    let dy = nbr.wy - owner.wy;
                    let angle = dy.atan2(dx).to_degrees().rem_euclid(360.0);
                    let bucket = (angle / 30.0) as usize;
                    angle_buckets[bucket.min(11)] += 1;
                    total += 1;
                }
            }
        }

        let expected_per_bucket = total as f64 / 12.0;
        let min_threshold = expected_per_bucket * 0.3;
        for (i, &count) in angle_buckets.iter().enumerate() {
            assert!(count as f64 >= min_threshold,
                "Angle bucket {}°-{}° has only {} edges (expected ~{:.0})",
                i * 30, (i + 1) * 30, count, expected_per_bucket);
        }
    }

    #[test]
    fn plate_size_variance_from_suppression() {
        let seed = 42u64;
        // Sample a region, assign each point to its nearest plate, count per plate
        let step = 100.0;
        let half = 10000.0;
        let mut counts: HashMap<u64, u32> = HashMap::new();

        let mut x = -half;
        while x < half {
            let mut y = -half;
            while y < half {
                let plate = macro_plate_at(x, y, seed);
                *counts.entry(plate.id).or_default() += 1;
                y += step;
            }
            x += step;
        }

        // Compute coefficient of variation
        let areas: Vec<f64> = counts.values().map(|&c| c as f64).collect();
        let mean = areas.iter().sum::<f64>() / areas.len() as f64;
        let variance = areas.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / areas.len() as f64;
        let std_dev = variance.sqrt();
        let cv = std_dev / mean;

        assert!(cv > 0.20,
            "Coefficient of variation {cv:.3} should be > 0.20 with cell suppression");
    }

    #[test]
    fn regime_value_is_deterministic() {
        let seed = 42u64;
        for x in (-30000..30000).step_by(3000) {
            for y in (-30000..30000).step_by(3000) {
                let a = regime_value_at(x as f64, y as f64, seed);
                let b = regime_value_at(x as f64, y as f64, seed);
                assert_eq!(a, b, "regime_value_at({x}, {y}) not deterministic");
            }
        }
    }

    #[test]
    fn regime_value_stays_in_unit_range() {
        let seed = 42u64;
        for x in (-50000..50000).step_by(500) {
            for y in (-50000..50000).step_by(500) {
                let v = regime_value_at(x as f64, y as f64, seed);
                assert!(v >= 0.0 && v <= 1.0,
                    "regime_value_at({x}, {y}) = {v}, outside [0, 1]");
            }
        }
    }

    #[test]
    fn warp_strength_is_deterministic() {
        let seed = 42u64;
        for x in (-30000..30000).step_by(3000) {
            for y in (-30000..30000).step_by(3000) {
                let a = warp_strength_at(x as f64, y as f64, seed);
                let b = warp_strength_at(x as f64, y as f64, seed);
                assert_eq!(a, b, "warp_strength_at({x}, {y}) not deterministic");
            }
        }
    }

    #[test]
    fn warp_strength_stays_in_range() {
        let seed = 42u64;
        for x in (-50000..50000).step_by(500) {
            for y in (-50000..50000).step_by(500) {
                let v = warp_strength_at(x as f64, y as f64, seed);
                assert!(v >= WARP_STRENGTH_MIN && v <= WARP_STRENGTH_MAX,
                    "warp_strength_at({x}, {y}) = {v}, outside [{}, {}]",
                    WARP_STRENGTH_MIN, WARP_STRENGTH_MAX);
            }
        }
    }

    #[test]
    fn sigmoid_extremes() {
        // Very negative input → near 0
        assert!(sigmoid(-10.0, 0.5, 6.0) < 0.01);
        // Very positive input → near 1
        assert!(sigmoid(10.0, 0.5, 6.0) > 0.99);
        // At midpoint → exactly 0.5
        assert!((sigmoid(0.5, 0.5, 6.0) - 0.5).abs() < 1e-10);
        // Zero steepness → 0.5 everywhere
        let v = sigmoid(100.0, 0.5, 0.0);
        assert!((v - 0.5).abs() < 1e-10, "sigmoid with steepness=0 should be 0.5, got {v}");
    }

    #[test]
    fn regime_gradient_no_nan_at_zero_gradient() {
        let seed = 42u64;
        for x in (-20000..20000).step_by(2000) {
            for y in (-20000..20000).step_by(2000) {
                let rg = RegimeGradient::at(x as f64, y as f64, seed);
                assert!(rg.ctx.elongation.is_finite(), "NaN elongation at ({x}, {y})");
                assert!(rg.ctx.elongation >= 1.0, "elongation < 1.0 at ({x}, {y})");
                assert!(rg.warp_strength.is_finite(), "NaN warp_strength at ({x}, {y})");
            }
        }
    }

    #[test]
    fn interior_isotropic_matches_euclidean() {
        // At zero gradient, aniso distance should equal euclidean distance
        let ctx = AnisoContext { across: (1.0, 0.0), along: (0.0, 1.0), elongation: 1.0 };
        let d_aniso = ctx.dist(0.0, 0.0, 300.0, 400.0);
        let d_euclid = dist_sq(0.0, 0.0, 300.0, 400.0).sqrt();
        assert!((d_aniso - d_euclid).abs() < 1e-6,
            "With elongation=1.0, aniso dist {d_aniso} should equal euclidean {d_euclid}");
    }

    #[test]
    fn coastal_warped_differs_from_isotropic() {
        // At coastal points (high gradient), anisotropic warped assignment should
        // sometimes differ from what isotropic warped assignment would produce
        let seed = 42u64;
        let mut differ = 0;
        let mut total_coastal = 0;
        for x in (-30000..30000).step_by(500) {
            for y in (-30000..30000).step_by(500) {
                let wx = x as f64;
                let wy = y as f64;
                let rg = RegimeGradient::at(wx, wy, seed);
                if rg.ctx.elongation > 2.0 {
                    total_coastal += 1;
                    let aniso_result = warped_plate_at(wx, wy, seed);

                    // Isotropic warped: euclidean + warp noise (no aniso)
                    let strength = warp_strength_at(wx, wy, seed);
                    let iso_ctx = AnisoContext { across: (1.0, 0.0), along: (0.0, 1.0), elongation: 1.0 };
                    let (cq, cr) = world_to_cell(wx, wy);
                    let search = 2 + MAX_ELONGATION as i32;
                    let mut iso_best: Option<PlateCenter> = None;
                    let mut iso_best_d = f64::MAX;
                    for dr in -search..=search {
                        for dq in -search..=search {
                            if let Some(c) = plate_center_for_cell(cq + dq, cr + dr, seed) {
                                let d = effective_distance(wx, wy, &c, strength, &iso_ctx, seed);
                                if d < iso_best_d {
                                    iso_best = Some(c);
                                    iso_best_d = d;
                                }
                            }
                        }
                    }
                    if let Some(iso) = iso_best {
                        if aniso_result.id != iso.id {
                            differ += 1;
                        }
                    }
                }
            }
        }
        assert!(total_coastal > 0, "Should find coastal points with elongation > 2");
        assert!(differ > 0,
            "Anisotropic assignment should differ from isotropic at some coastal points ({differ}/{total_coastal})");
    }

    #[test]
    fn regime_gradient_matches_standalone() {
        // RegimeGradient should produce identical warp_strength as standalone
        let seed = 42u64;
        for x in (-20000..20000).step_by(2000) {
            for y in (-20000..20000).step_by(2000) {
                let wx = x as f64;
                let wy = y as f64;
                let rg = RegimeGradient::at(wx, wy, seed);
                let standalone = warp_strength_at(wx, wy, seed);
                assert!((rg.warp_strength - standalone).abs() < 1e-10,
                    "RegimeGradient warp={} vs standalone={} at ({wx}, {wy})",
                    rg.warp_strength, standalone);
            }
        }
    }

    #[test]
    fn regime_values_are_bimodal() {
        // After sigmoid, values should cluster near 0 and 1.
        // Count how many fall in the "flat" zones vs the transition band.
        let seed = 42u64;
        let mut near_zero = 0;
        let mut near_one = 0;
        let mut in_transition = 0;
        for x in (-50000..50000).step_by(500) {
            for y in (-50000..50000).step_by(500) {
                let v = regime_value_at(x as f64, y as f64, seed);
                if v < 0.1 {
                    near_zero += 1;
                } else if v > 0.9 {
                    near_one += 1;
                } else {
                    in_transition += 1;
                }
            }
        }
        let total = near_zero + near_one + in_transition;
        let plateau_pct = (near_zero + near_one) as f64 / total as f64 * 100.0;
        assert!(plateau_pct > 50.0,
            "Sigmoid should push most values to plateaus: {plateau_pct:.1}% in plateaus \
             ({near_zero} near 0, {near_one} near 1, {in_transition} in transition)");
    }

    #[test]
    fn warp_strength_near_zero_far_from_transition() {
        // Sample deep-water and deep-land points — their warp should be very low.
        let seed = 42u64;
        let mut low_warp_count = 0;
        let mut total_deep = 0;
        let threshold = WARP_STRENGTH_MAX * 0.15;
        for x in (-50000..50000).step_by(1000) {
            for y in (-50000..50000).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                let regime = regime_value_at(wx, wy, seed);
                // Only check points clearly in deep water or deep land
                if regime < 0.05 || regime > 0.95 {
                    total_deep += 1;
                    let strength = warp_strength_at(wx, wy, seed);
                    if strength < threshold {
                        low_warp_count += 1;
                    }
                }
            }
        }
        assert!(total_deep > 0, "Should find deep water/land points");
        let pct = low_warp_count as f64 / total_deep as f64 * 100.0;
        assert!(pct > 90.0,
            "Deep water/land points should have low warp: {pct:.1}% below threshold \
             ({low_warp_count}/{total_deep})");
    }

    #[test]
    fn each_plate_center_in_exactly_one_chunk() {
        let seed = 42u64;
        let mut seen: HashMap<u64, (i32, i32)> = HashMap::new();

        for cq in -5..=5 {
            for cr in -5..=5 {
                let chunk = populate_chunk(cq, cr, seed);
                for center in &chunk.centers {
                    if let Some(&prev) = seen.get(&center.id) {
                        panic!("PlateCenter id={} in chunks ({}, {}) and ({}, {})",
                            center.id, prev.0, prev.1, cq, cr);
                    }
                    seen.insert(center.id, (cq, cr));
                }
            }
        }
    }

    #[test]
    fn chunk_cache_is_deterministic() {
        let seed = 42u64;
        let mut cache_a = PlateCache::new(seed);
        let mut cache_b = PlateCache::new(seed);

        // Query in different orders
        for x in (-5000..5000).step_by(1000) {
            for y in (-5000..5000).step_by(1000) {
                cache_a.warped_plate_at(x as f64, y as f64);
            }
        }
        for y in (-5000..5000).step_by(1000) {
            for x in (-5000..5000).step_by(1000) {
                cache_b.warped_plate_at(x as f64, y as f64);
            }
        }

        // Verify same results
        for x in (-5000..5000).step_by(1000) {
            for y in (-5000..5000).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                let a = cache_a.warped_plate_at(wx, wy);
                let b = cache_b.warped_plate_at(wx, wy);
                assert_eq!(a.id, b.id, "Cache order matters at ({wx}, {wy})");
            }
        }
    }

    #[test]
    fn no_high_warp_deep_inland() {
        // No deep-land or deep-water point should have warp > 50% of max.
        let seed = 42u64;
        let high_threshold = WARP_STRENGTH_MAX * 0.5;
        for x in (-50000..50000).step_by(500) {
            for y in (-50000..50000).step_by(500) {
                let wx = x as f64;
                let wy = y as f64;
                let regime = regime_value_at(wx, wy, seed);
                if regime < 0.02 || regime > 0.98 {
                    let strength = warp_strength_at(wx, wy, seed);
                    assert!(strength < high_threshold,
                        "Deep point at ({wx}, {wy}) regime={regime:.4} has high warp={strength:.1} \
                         (threshold={high_threshold:.1})");
                }
            }
        }
    }

    #[test]
    fn suppression_varies_with_regime_depth() {
        // Cells near the regime transition should have lower suppression
        // than cells deep inland or deep in water.
        let seed = 42u64;
        let mut coastal_suppressed = 0u32;
        let mut coastal_total = 0u32;
        let mut deep_suppressed = 0u32;
        let mut deep_total = 0u32;

        for cq in -30..30 {
            for cr in -30..30 {
                let odd_shift = if cr & 1 != 0 { MACRO_CELL_SIZE * 0.5 } else { 0.0 };
                let wx = cq as f64 * MACRO_CELL_SIZE + odd_shift;
                let wy = cr as f64 * MACRO_CELL_SIZE * HEX_ROW_HEIGHT;
                let regime = regime_value_at(wx, wy, seed);
                let is_suppressed = plate_center_for_cell(cq, cr, seed).is_none();

                if (regime - REGIME_LAND_THRESHOLD).abs() < 0.15 {
                    coastal_total += 1;
                    if is_suppressed { coastal_suppressed += 1; }
                } else if regime < 0.1 || regime > 0.9 {
                    deep_total += 1;
                    if is_suppressed { deep_suppressed += 1; }
                }
            }
        }

        assert!(coastal_total > 0, "Should find coastal cells");
        assert!(deep_total > 0, "Should find deep cells");
        let coastal_rate = coastal_suppressed as f64 / coastal_total as f64;
        let deep_rate = deep_suppressed as f64 / deep_total as f64;
        assert!(deep_rate > coastal_rate,
            "Deep suppression {deep_rate:.3} should exceed coastal {coastal_rate:.3}");
    }

    #[test]
    fn deep_region_has_fewer_plate_centers() {
        // Deep regions should have lower plate center survival rates
        // than coastal regions, producing fewer, larger plates.
        let seed = 42u64;
        let mut deep_survived = 0u32;
        let mut deep_total = 0u32;
        let mut coastal_survived = 0u32;
        let mut coastal_total = 0u32;

        for cq in -50..50 {
            for cr in -50..50 {
                let odd_shift = if cr & 1 != 0 { MACRO_CELL_SIZE * 0.5 } else { 0.0 };
                let wx = cq as f64 * MACRO_CELL_SIZE + odd_shift;
                let wy = cr as f64 * MACRO_CELL_SIZE * HEX_ROW_HEIGHT;
                let regime = regime_value_at(wx, wy, seed);

                let survived = plate_center_for_cell(cq, cr, seed).is_some();

                if regime > 0.9 || regime < 0.1 {
                    deep_total += 1;
                    if survived { deep_survived += 1; }
                } else if (regime - REGIME_LAND_THRESHOLD).abs() < 0.15 {
                    coastal_total += 1;
                    if survived { coastal_survived += 1; }
                }
            }
        }

        assert!(deep_total > 0, "Should find deep cells");
        assert!(coastal_total > 0, "Should find coastal cells");

        let deep_rate = deep_survived as f64 / deep_total as f64;
        let coast_rate = coastal_survived as f64 / coastal_total as f64;

        assert!(coast_rate > deep_rate,
            "Coastal survival rate ({coast_rate:.3}) should exceed deep ({deep_rate:.3})");
    }

    // ──── classify_tags tests ────

    #[test]
    fn every_macro_plate_has_exactly_one_base_tag() {
        let seed = 0x9E3779B97F4A7C15;
        let mut cache = PlateCache::new(seed);
        let mut plates = cache.plates_in_radius(0.0, 0.0, 5000.0);
        assert!(!plates.is_empty(), "Should have plates in radius");
        cache.classify_tags(&mut plates);

        for plate in &plates {
            let count = [PlateTag::Sea, PlateTag::Coast, PlateTag::Inland]
                .iter()
                .filter(|t| plate.has_tag(t))
                .count();
            assert_eq!(
                count, 1,
                "plate {} should have exactly one base tag, got {}",
                plate.id, count
            );
        }
    }


}
