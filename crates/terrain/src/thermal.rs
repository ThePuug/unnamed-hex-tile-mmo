use std::collections::HashMap;

use crate::hotspots::{
    hotspot_center, hotspot_phase_offset, hotspot_lifecycle,
    HOTSPOT_THRESHOLD, HOTSPOT_GRID_SPACING, HOTSPOT_CYCLE_TICKS,
};
use crate::material::material_density_cart;
use crate::{cart_to_hex, div_floor};

/// Gaussian plume spread (cartesian units). Larger = wider plumes.
/// σ=1200 → ~7200 tile diameter influence (3σ each side).
const THERMAL_SIGMA: f64 = 1_200.0;

/// Maximum intensity a single thermal source can emit.
/// At 0.12, a strong margin cluster of ~5 peaking cells reaches 0.3–0.5
/// at center — visible bright plume. Isolated cells stay subtle.
pub(crate) const MAX_SOURCE_INTENSITY: f64 = 0.12;

/// How aggressively the lid suppresses heat reaching the surface.
/// Higher = thicker lid blocks more. Controls margin-vs-interior contrast.
///   lid 0.01 (thin margin):   penetration = 0.92
///   lid 0.05 (mid margin):    penetration = 0.67
///   lid 0.10 (inner margin):  penetration = 0.45
///   lid 0.25 (deep interior): penetration = 0.14
///   lid 0.30 (deepest):       penetration = 0.09
const LID_SUPPRESSION: f64 = 8.0;

/// Sources below this intensity are filtered out (performance).
const NOISE_FLOOR: f64 = 0.001;

/// Thermal chunk size in hex tiles.
/// Must be >= 3 * THERMAL_SIGMA so plumes from 2+ chunks away contribute < 1%.
/// At 4500 (1.25× the 3σ=3600 footprint), ring-2 hex chunks are well beyond cutoff.
const THERMAL_CHUNK_SIZE: i64 = 4_500;

/// Hotspot grid cells per thermal chunk along each axis.
/// THERMAL_CHUNK_SIZE / HOTSPOT_GRID_SPACING = 4500 / 750 = 6.
const GRID_CELLS_PER_CHUNK: i32 = 6;

/// Six hex-direction neighbor offsets (pointy-top hex grid, axial coordinates).
const HEX_NEIGHBORS: [(i32, i32); 6] = [
    (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1),
];

// ──── Source Intensity ────

/// Thermal source intensity at a hotspot grid cell.
/// Returns None if below density threshold or below noise floor.
///
/// Continuous penetration model: lid thickness controls how much heat
/// reaches the surface (exponential suppression), lifecycle controls
/// instantaneous energy. No threshold gates beyond the density boundary.
pub(crate) fn thermal_source_intensity(cell_q: i32, cell_r: i32, seed: u64, world_tick: u64) -> Option<f64> {
    let (cx, cy) = hotspot_center(cell_q, cell_r);
    let density = material_density_cart(cx, cy, seed);

    if density < HOTSPOT_THRESHOLD {
        return None;
    }

    let lid_thickness = density - HOTSPOT_THRESHOLD;

    let offset = hotspot_phase_offset(cell_q, cell_r, seed);
    let tick_phase = if HOTSPOT_CYCLE_TICKS > 0 {
        (world_tick % HOTSPOT_CYCLE_TICKS) as f64 / HOTSPOT_CYCLE_TICKS as f64
    } else {
        0.0
    };
    let phase = (tick_phase + offset) % 1.0;
    let lifecycle = hotspot_lifecycle(phase);

    let penetration = (-lid_thickness * LID_SUPPRESSION).exp();
    let intensity = penetration * lifecycle * MAX_SOURCE_INTENSITY;

    if intensity > NOISE_FLOOR { Some(intensity) } else { None }
}

/// Evaluate whether a grid cell produces a thermal source.
/// Returns Some((center_x, center_y, intensity)) if active, None otherwise.
pub(crate) fn thermal_source_at(cq: i32, cr: i32, seed: u64, world_tick: u64) -> Option<(f64, f64, f64)> {
    let (cx, cy) = hotspot_center(cq, cr);
    thermal_source_intensity(cq, cr, seed, world_tick).map(|i| (cx, cy, i))
}

// ──── Gaussian Diffusion ────

fn gaussian_contribution(dist_sq: f64, intensity: f64) -> f64 {
    intensity * (-dist_sq / (2.0 * THERMAL_SIGMA * THERMAL_SIGMA)).exp()
}

/// Sum Gaussian contributions from gathered sources at a world position.
pub fn temperature_at(wx: f64, wy: f64, sources: &[ThermalSource]) -> f64 {
    let cutoff_sq = (3.0 * THERMAL_SIGMA) * (3.0 * THERMAL_SIGMA);
    let mut temperature = 0.0;

    for source in sources {
        let dx = wx - source.x;
        let dy = wy - source.y;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq > cutoff_sq {
            continue;
        }
        temperature += gaussian_contribution(dist_sq, source.intensity);
    }

    temperature.min(1.0)
}

/// Brute-force temperature at a single world position (no caching).
/// Scans all grid cells within 3σ range. Used for diagnostic comparison.
#[allow(dead_code)]
pub(crate) fn temperature_at_brute(wx: f64, wy: f64, seed: u64, world_tick: u64) -> f64 {
    let (approx_q, approx_r) = cart_to_hex(wx, wy);
    let gs = HOTSPOT_GRID_SPACING as i64;
    let base_gq = div_floor(approx_q as i64, gs) as i32;
    let base_gr = div_floor(approx_r as i64, gs) as i32;

    let search = (3.0 * THERMAL_SIGMA / HOTSPOT_GRID_SPACING).ceil() as i32;
    let cutoff_sq = (3.0 * THERMAL_SIGMA) * (3.0 * THERMAL_SIGMA);

    let mut temperature = 0.0;
    for dq in -search..=search {
        for dr in -search..=search {
            let gq = base_gq + dq;
            let gr = base_gr + dr;
            if let Some((sx, sy, intensity)) = thermal_source_at(gq, gr, seed, world_tick) {
                let dx = wx - sx;
                let dy = wy - sy;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= cutoff_sq {
                    temperature += gaussian_contribution(dist_sq, intensity);
                }
            }
        }
    }

    temperature.min(1.0)
}

// ──── Chunk Cache ────

/// A single thermal radiation source.
#[derive(Clone)]
pub struct ThermalSource {
    pub x: f64,
    pub y: f64,
    pub intensity: f64,
}

struct ThermalChunk {
    sources: Vec<ThermalSource>,
}

/// Cache of precomputed thermal sources per chunk.
/// Eliminates redundant noise evaluations when rendering many tiles.
pub struct ThermalChunkCache {
    chunks: HashMap<(i32, i32), ThermalChunk>,
    seed: u64,
    world_tick: u64,
}

/// Map a hex tile to its thermal chunk coordinate using hexagonal Voronoi regions.
pub fn tile_to_thermal_chunk(q: i32, r: i32) -> (i32, i32) {
    crate::tile_to_hex_chunk(q, r, THERMAL_CHUNK_SIZE as i32)
}

/// Assign a hotspot grid cell to the thermal chunk containing its center.
fn grid_cell_to_chunk(gq: i32, gr: i32) -> (i32, i32) {
    crate::hex_round(
        (gq as f64 + 0.5) / GRID_CELLS_PER_CHUNK as f64,
        (gr as f64 + 0.5) / GRID_CELLS_PER_CHUNK as f64,
    )
}

fn compute_chunk(cq: i32, cr: i32, seed: u64, world_tick: u64) -> ThermalChunk {
    let gcpc = GRID_CELLS_PER_CHUNK;
    let center_gq = cq * gcpc;
    let center_gr = cr * gcpc;
    let reach = gcpc / 2 + 1;
    let mut sources = Vec::new();

    for dgq in -reach..=reach {
        for dgr in -reach..=reach {
            let gq = center_gq + dgq;
            let gr = center_gr + dgr;
            if grid_cell_to_chunk(gq, gr) != (cq, cr) {
                continue;
            }
            if let Some((x, y, intensity)) = thermal_source_at(gq, gr, seed, world_tick) {
                sources.push(ThermalSource { x, y, intensity });
            }
        }
    }

    ThermalChunk { sources }
}

impl ThermalChunkCache {
    pub fn new(seed: u64, world_tick: u64) -> Self {
        Self {
            chunks: HashMap::new(),
            seed,
            world_tick,
        }
    }

    /// Temperature at a hex tile, computing and caching chunks as needed.
    pub fn temperature_at_tile(&mut self, q: i32, r: i32) -> f64 {
        let (wx, wy) = crate::hex_to_world(q, r);
        let (cq, cr) = tile_to_thermal_chunk(q, r);
        let sources = self.gather_sources(cq, cr);
        temperature_at(wx, wy, &sources)
    }

    /// Gather all thermal sources from a chunk and its 6 hex neighbors.
    pub fn gather_sources(&mut self, cq: i32, cr: i32) -> Vec<ThermalSource> {
        let seed = self.seed;
        let tick = self.world_tick;

        let mut ring = [(0i32, 0i32); 7];
        ring[0] = (cq, cr);
        for (i, &(dq, dr)) in HEX_NEIGHBORS.iter().enumerate() {
            ring[i + 1] = (cq + dq, cr + dr);
        }

        for &(q, r) in &ring {
            self.chunks.entry((q, r))
                .or_insert_with(|| compute_chunk(q, r, seed, tick));
        }

        let mut all = Vec::new();
        for &(q, r) in &ring {
            all.extend(self.chunks[&(q, r)].sources.iter().cloned());
        }
        all
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotspots::DEFAULT_WORLD_TICK;

    #[test]
    fn thin_lid_high_lifecycle_near_max() {
        // Thin lid + peak lifecycle should produce intensity near MAX_SOURCE_INTENSITY
        let seed = 42u64;
        let mut found = false;
        for cq in -50..50 {
            for cr in -50..50 {
                let (cx, cy) = hotspot_center(cq, cr);
                let density = material_density_cart(cx, cy, seed);
                let lid = density - HOTSPOT_THRESHOLD;
                // Very thin lid (penetration ≈ 1.0)
                if lid > 0.0 && lid < 0.02 {
                    // Find a tick where lifecycle is at peak (1.0)
                    let offset = hotspot_phase_offset(cq, cr, seed);
                    // Phase 0.65 → lifecycle = 1.0 (plateau)
                    let target_phase = 0.65;
                    let tick_frac = (target_phase - offset + 1.0) % 1.0;
                    let tick = (tick_frac * HOTSPOT_CYCLE_TICKS as f64) as u64;

                    if let Some(intensity) = thermal_source_intensity(cq, cr, seed, tick) {
                        let penetration = (-lid * LID_SUPPRESSION).exp();
                        assert!(intensity > MAX_SOURCE_INTENSITY * 0.8,
                            "Thin lid={:.4} pen={:.3} should give near-max intensity, got {}",
                            lid, penetration, intensity);
                        found = true;
                        break;
                    }
                }
            }
            if found { break; }
        }
        assert!(found, "No thin-lid cells found for test");
    }

    #[test]
    fn thick_lid_heavily_suppressed() {
        // Thick lid cells should be much weaker than thin lid cells
        let seed = 42u64;
        let mut found = false;
        for cq in -50..50 {
            for cr in -50..50 {
                let (cx, cy) = hotspot_center(cq, cr);
                let density = material_density_cart(cx, cy, seed);
                let lid = density - HOTSPOT_THRESHOLD;
                if lid > 0.25 {
                    // Force peak lifecycle
                    let offset = hotspot_phase_offset(cq, cr, seed);
                    let tick_frac = (0.65 - offset + 1.0) % 1.0;
                    let tick = (tick_frac * HOTSPOT_CYCLE_TICKS as f64) as u64;

                    if let Some(intensity) = thermal_source_intensity(cq, cr, seed, tick) {
                        let penetration = (-lid * LID_SUPPRESSION).exp();
                        assert!(penetration < 0.15,
                            "lid={:.3} should give low penetration, got {:.3}", lid, penetration);
                        assert!(intensity < MAX_SOURCE_INTENSITY * 0.15,
                            "Thick lid={:.3} should be heavily suppressed, got {}", lid, intensity);
                        found = true;
                        break;
                    }
                }
            }
            if found { break; }
        }
        assert!(found, "No thick-lid cells found for test");
    }

    #[test]
    fn penetration_monotonically_decreases() {
        for i in 0..20 {
            let lid_a = i as f64 * 0.02;
            let lid_b = lid_a + 0.02;
            let pen_a = (-lid_a * LID_SUPPRESSION).exp();
            let pen_b = (-lid_b * LID_SUPPRESSION).exp();
            assert!(pen_a > pen_b,
                "Penetration should decrease: lid {:.2} → {:.4}, lid {:.2} → {:.4}",
                lid_a, pen_a, lid_b, pen_b);
        }
    }

    #[test]
    fn intensity_capped_at_max() {
        let seed = 42u64;
        for tick in [0u64, 100, 250, 500, 650, 750, 900] {
            for cq in -30..30 {
                for cr in -30..30 {
                    if let Some(intensity) = thermal_source_intensity(cq, cr, seed, tick) {
                        assert!(intensity <= MAX_SOURCE_INTENSITY,
                            "Source ({}, {}) tick={} exceeds max: {}", cq, cr, tick, intensity);
                    }
                }
            }
        }
    }

    #[test]
    fn thermal_source_inactive_in_light_region() {
        let seed = 42u64;
        let mut found = false;
        for cq in -50..50 {
            for cr in -50..50 {
                let (cx, cy) = hotspot_center(cq, cr);
                let density = material_density_cart(cx, cy, seed);
                if density < HOTSPOT_THRESHOLD {
                    assert!(thermal_source_at(cq, cr, seed, DEFAULT_WORLD_TICK).is_none(),
                        "Grid cell ({}, {}) in light region (density={:.3}) should not be a thermal source",
                        cq, cr, density);
                    found = true;
                }
            }
        }
        assert!(found, "No light-region grid cells found");
    }

    #[test]
    fn thermal_source_active_in_dense_region() {
        let seed = 42u64;
        let mut any_active = false;
        for cq in -50..50 {
            for cr in -50..50 {
                let (cx, cy) = hotspot_center(cq, cr);
                let density = material_density_cart(cx, cy, seed);
                if density > HOTSPOT_THRESHOLD {
                    if let Some((_, _, intensity)) = thermal_source_at(cq, cr, seed, DEFAULT_WORLD_TICK) {
                        assert!(intensity > 0.0);
                        any_active = true;
                    }
                }
            }
        }
        assert!(any_active, "No active thermal sources found in dense regions");
    }

    #[test]
    fn gaussian_at_center_equals_intensity() {
        let intensity = 0.75;
        let contrib = gaussian_contribution(0.0, intensity);
        assert!((contrib - intensity).abs() < f64::EPSILON);
    }

    #[test]
    fn gaussian_decays_with_distance() {
        let close = gaussian_contribution(1000.0 * 1000.0, 1.0);
        let far = gaussian_contribution(3000.0 * 3000.0, 1.0);
        assert!(close > far, "Closer should be warmer: {} vs {}", close, far);
    }

    #[test]
    fn gaussian_at_3sigma_is_negligible() {
        let dist = 3.0 * THERMAL_SIGMA;
        let contrib = gaussian_contribution(dist * dist, 1.0);
        assert!(contrib < 0.02, "At 3σ contribution should be < 2%, got {}", contrib);
    }

    #[test]
    fn temperature_at_empty_sources_is_zero() {
        assert_eq!(temperature_at(0.0, 0.0, &[]), 0.0);
    }

    #[test]
    fn temperature_at_single_source_center() {
        let sources = vec![ThermalSource { x: 100.0, y: 200.0, intensity: 0.8 }];
        let temp = temperature_at(100.0, 200.0, &sources);
        assert!((temp - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn temperature_at_capped_at_one() {
        let sources = vec![
            ThermalSource { x: 0.0, y: 0.0, intensity: 0.8 },
            ThermalSource { x: 100.0, y: 0.0, intensity: 0.8 },
        ];
        let temp = temperature_at(50.0, 0.0, &sources);
        assert!(temp <= 1.0, "Temperature should be capped at 1.0, got {}", temp);
    }

    #[test]
    fn temperature_at_beyond_cutoff_is_zero() {
        let sources = vec![ThermalSource { x: 0.0, y: 0.0, intensity: 1.0 }];
        let far = 3.0 * THERMAL_SIGMA + 1.0;
        let temp = temperature_at(far, 0.0, &sources);
        assert_eq!(temp, 0.0);
    }

    #[test]
    fn temperature_range() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut cache = ThermalChunkCache::new(seed, tick);
        for q in (-10000..10000).step_by(500) {
            for r in (-10000..10000).step_by(500) {
                let temp = cache.temperature_at_tile(q, r);
                assert!(temp >= 0.0 && temp <= 1.0,
                    "Temperature {} at ({}, {}) out of [0, 1]", temp, q, r);
            }
        }
    }

    #[test]
    fn cache_is_deterministic() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut cache1 = ThermalChunkCache::new(seed, tick);
        let mut cache2 = ThermalChunkCache::new(seed, tick);

        for q in (-10000..10000).step_by(500) {
            for r in (-10000..10000).step_by(500) {
                let t1 = cache1.temperature_at_tile(q, r);
                let t2 = cache2.temperature_at_tile(q, r);
                assert_eq!(t1, t2, "Cache not deterministic at ({q}, {r})");
            }
        }
    }

    /// Prove that sources just beyond the hex 1-ring neighborhood contribute
    /// negligibly to any tile in the center chunk.
    ///
    /// With the continuous penetration model, every cell above HOTSPOT_THRESHOLD
    /// is a potential source. The worst case is a province whose entire dense
    /// region (above threshold) falls in ring-2 chunks. We bound the maximum
    /// contiguous active cells from the material field's shallowest gradient,
    /// and the maximum per-source intensity from MAX_SOURCE_INTENSITY.
    ///
    /// With hex Voronoi chunks, ring-2 is separated from the center chunk by
    /// a full ring of neighbors. The minimum cartesian distance across this
    /// gap is approximately THERMAL_CHUNK_SIZE * √3/2.
    #[test]
    fn missed_sources_beyond_neighborhood_are_negligible() {
        use std::f64::consts::PI;
        use crate::material::{MATERIAL_WAVES, MATERIAL_AMPLITUDE};

        // ── Derive worst-case cluster size ──

        // The shallowest density gradient bounds the widest contiguous band
        // of above-threshold cells. density range above threshold is at most 1.0.
        let total_weight: f64 = MATERIAL_WAVES.iter().map(|&(_, w)| w).sum();
        let (longest_wavelength, longest_weight) = MATERIAL_WAVES
            .iter()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .copied()
            .unwrap();

        let min_gradient = MATERIAL_AMPLITUDE * (longest_weight / total_weight)
            * (2.0 * PI / longest_wavelength);

        // Width of the above-threshold band: density goes from threshold
        // to max ~1.0, a span of ~(1 - HOTSPOT_THRESHOLD).
        let max_dense_width = (1.0 - HOTSPOT_THRESHOLD) / min_gradient;
        let max_cluster_size = (max_dense_width / HOTSPOT_GRID_SPACING).ceil() as usize;

        // ── Minimum distance to non-neighbor sources ──

        let min_outside_distance =
            (THERMAL_CHUNK_SIZE as f64 - HOTSPOT_GRID_SPACING) * (3.0_f64.sqrt() / 2.0);

        // ── Sum worst-case Gaussian contributions ──

        let mut total_contribution = 0.0;
        for i in 0..max_cluster_size {
            let dist = min_outside_distance + (i as f64) * HOTSPOT_GRID_SPACING;
            let dist_sq = dist * dist;
            total_contribution += gaussian_contribution(dist_sq, MAX_SOURCE_INTENSITY);
        }

        let threshold = 0.01;
        assert!(
            total_contribution < threshold,
            "Worst-case cluster of {max_cluster_size} sources \
             (dense_width={max_dense_width:.0}, min_dist={min_outside_distance:.0}) \
             contributes {total_contribution:.4} >= {threshold} (1%)."
        );
    }

    #[test]
    fn light_region_tiles_have_zero_temperature() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut cache = ThermalChunkCache::new(seed, tick);
        let mut found_zero = false;

        for q in (-50000..50000).step_by(2000) {
            for r in (-50000..50000).step_by(2000) {
                let (wx, wy) = crate::hex_to_world(q, r);
                let density = material_density_cart(wx, wy, seed);
                if density < HOTSPOT_THRESHOLD - 0.1 {
                    let temp = cache.temperature_at_tile(q, r);
                    if temp == 0.0 {
                        found_zero = true;
                    }
                }
            }
        }
        assert!(found_zero, "Some deep light region tiles should have zero temperature");
    }
}
