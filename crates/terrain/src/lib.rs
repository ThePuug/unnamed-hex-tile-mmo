mod types;
mod material;
mod hotspots;
mod thermal;
mod flow;

pub use types::*;
pub use hotspots::{HotspotCell, HotspotChunkCache, cart_to_grid_cell, tile_to_chunk, CHUNK_RADIUS};
pub use thermal::{ThermalChunkCache, ThermalSource, temperature_at, tile_to_thermal_chunk};
pub use flow::{flow_at, flow_at_tile, FlowChunkCache};

use hotspots::{
    nearest_hotspot, nearest_hotspot_cached, hotspot_lifecycle, hotspot_phase_offset,
    HOTSPOT_THRESHOLD, HOTSPOT_GRID_SPACING, HOTSPOT_CYCLE_TICKS, DEFAULT_WORLD_TICK,
};
use material::material_density_cart;

const SQRT_3: f64 = 1.7320508075688772;

// ──── Coordinate Conversion ────

/// Convert hex tile coordinates to world (cartesian) coordinates.
/// Hex q,r axes are 60° apart; this produces isotropic x,y.
pub fn hex_to_world(q: i32, r: i32) -> (f64, f64) {
    let qf = q as f64;
    let rf = r as f64;
    (qf + rf * 0.5, rf * SQRT_3 / 2.0)
}

/// Convert fractional hex coordinates to cartesian.
/// Internal use for positions that are already in continuous hex space
/// (e.g. grid cell nominal positions).
pub(crate) fn hex_to_cart(q: f64, r: f64) -> (f64, f64) {
    (q + r * 0.5, r * SQRT_3 / 2.0)
}

pub(crate) fn cart_to_hex(x: f64, y: f64) -> (f64, f64) {
    let r = y * 2.0 / SQRT_3;
    let q = x - r * 0.5;
    (q, r)
}

pub(crate) fn div_floor(a: i64, b: i64) -> i64 {
    let d = a / b;
    let r = a % b;
    if (r != 0) && ((r ^ b) < 0) { d - 1 } else { d }
}

/// Round fractional axial hex coordinates to the nearest integer hex.
/// Uses cube-coordinate rounding: convert to cube (q, r, s=-q-r), round
/// each component, then fix the one with the largest rounding error to
/// restore the q+r+s=0 constraint.
pub(crate) fn hex_round(fq: f64, fr: f64) -> (i32, i32) {
    let fz = -fq - fr;
    let rq = fq.round();
    let rr = fr.round();
    let rz = fz.round();

    let q_diff = (rq - fq).abs();
    let r_diff = (rr - fr).abs();
    let z_diff = (rz - fz).abs();

    if q_diff > r_diff && q_diff > z_diff {
        ((-rr - rz) as i32, rr as i32)
    } else if r_diff > z_diff {
        (rq as i32, (-rq - rz) as i32)
    } else {
        (rq as i32, rr as i32)
    }
}

/// Map a hex tile to its chunk using hexagonal Voronoi regions.
/// Chunk centers sit at tile coordinates (cq * spacing, cr * spacing).
/// Each tile is assigned to the nearest chunk center in hex distance.
/// O(1) — just a division and a cube-round.
pub(crate) fn tile_to_hex_chunk(q: i32, r: i32, spacing: i32) -> (i32, i32) {
    let fq = q as f64 / spacing as f64;
    let fr = r as f64 / spacing as f64;
    hex_round(fq, fr)
}

// ──── Terrain ────

pub struct Terrain {
    seed: u64,
    world_tick: u64,
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self { seed, world_tick: DEFAULT_WORLD_TICK }
    }

    pub fn with_tick(seed: u64, world_tick: u64) -> Self {
        Self { seed, world_tick }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn world_tick(&self) -> u64 {
        self.world_tick
    }

    /// Placeholder — elevation system removed, will be rebuilt from material + hotspot height.
    pub fn get_height(&self, _q: i32, _r: i32) -> i32 {
        0
    }

    pub fn evaluate(&self, q: i32, r: i32) -> TerrainEval {
        TerrainEval {
            height: 0,
            temperature: self.temperature(q, r),
            flow: self.flow(q, r),
        }
    }

    /// Primordial material density at a tile.
    /// Dense regions trap heat (hotspot convection), light regions are quiescent.
    pub fn material_density(&self, q: i32, r: i32) -> f64 {
        let (cx, cy) = hex_to_world(q, r);
        material_density_cart(cx, cy, self.seed)
    }

    /// Additive Gaussian surface temperature at a tile.
    /// Creates a temporary cache — prefer `temperature_cached` for bulk evaluation.
    pub fn temperature(&self, q: i32, r: i32) -> f64 {
        let mut cache = ThermalChunkCache::new(self.seed, self.world_tick);
        cache.temperature_at_tile(q, r)
    }

    /// Additive Gaussian surface temperature at a tile, using a shared cache.
    pub fn temperature_cached(&self, q: i32, r: i32, cache: &mut ThermalChunkCache) -> f64 {
        cache.temperature_at_tile(q, r)
    }

    /// Thermal gradient flow vector at a tile (grid-snapped).
    /// Creates a temporary cache — prefer `flow_cached` for bulk evaluation.
    pub fn flow(&self, q: i32, r: i32) -> (f64, f64) {
        let mut cache = flow::FlowChunkCache::new(self.seed, self.world_tick);
        cache.flow_at_tile(q, r)
    }

    /// Thermal gradient flow vector at a tile, using a shared cache.
    pub fn flow_cached(&self, q: i32, r: i32, cache: &mut flow::FlowChunkCache) -> (f64, f64) {
        cache.flow_at_tile(q, r)
    }

    /// Raw sub-lid convection: every cell beneath the dense lid, all lifecycle phases.
    /// No threshold gating — shows the full cellular structure like boiling under glass.
    /// Diagnostic: isolates hotspot placement/lifecycle from surface expression.
    pub fn hotspot_temperature(&self, q: i32, r: i32) -> f64 {
        let (cx, cy) = hex_to_world(q, r);

        // Only render under the lid — light regions stay black
        if material_density_cart(cx, cy, self.seed) < HOTSPOT_THRESHOLD {
            return 0.0;
        }

        let (_, _, cell_q, cell_r, dist) = match nearest_hotspot(cx, cy, self.seed) {
            Some(v) => v,
            None => return 0.0,
        };

        // Radial falloff: hot center, dim boundary — but never black.
        // Voronoi boundary sits at ~half_spacing between adjacent cells.
        let ratio = dist / (HOTSPOT_GRID_SPACING * 0.5);
        let radial = (1.0 - ratio * 0.7).clamp(0.15, 1.0);

        // Lifecycle gives current vigor — dormant cells dim, peaking cells bright
        let offset = hotspot_phase_offset(cell_q, cell_r, self.seed);
        let tick_phase = if HOTSPOT_CYCLE_TICKS > 0 {
            (self.world_tick % HOTSPOT_CYCLE_TICKS) as f64 / HOTSPOT_CYCLE_TICKS as f64
        } else {
            0.0
        };
        let phase = (tick_phase + offset) % 1.0;
        let hotspot_height = hotspot_lifecycle(phase);

        let base = 0.1 + 0.9 * hotspot_height;

        base * radial
    }

    /// Same as `hotspot_temperature`, but uses a precomputed cache to avoid
    /// redundant noise evaluations for grid cell activity checks.
    pub fn hotspot_temperature_cached(&self, q: i32, r: i32, cache: &mut HotspotChunkCache) -> f64 {
        let (cx, cy) = hex_to_world(q, r);

        // Per-tile density check remains (unique per tile, unavoidable)
        if material_density_cart(cx, cy, self.seed) < HOTSPOT_THRESHOLD {
            return 0.0;
        }

        let (_, _, cell_q, cell_r, dist) = match nearest_hotspot_cached(cx, cy, cache) {
            Some(v) => v,
            None => return 0.0,
        };

        let ratio = dist / (HOTSPOT_GRID_SPACING * 0.5);
        let radial = (1.0 - ratio * 0.7).clamp(0.15, 1.0);

        let offset = hotspot_phase_offset(cell_q, cell_r, self.seed);
        let tick_phase = if HOTSPOT_CYCLE_TICKS > 0 {
            (self.world_tick % HOTSPOT_CYCLE_TICKS) as f64 / HOTSPOT_CYCLE_TICKS as f64
        } else {
            0.0
        };
        let phase = (tick_phase + offset) % 1.0;
        let hotspot_height = hotspot_lifecycle(phase);

        let base = 0.1 + 0.9 * hotspot_height;

        base * radial
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let t1 = Terrain::new(42);
        let t2 = Terrain::new(42);
        let mut cache1 = ThermalChunkCache::new(42, DEFAULT_WORLD_TICK);
        let mut cache2 = ThermalChunkCache::new(42, DEFAULT_WORLD_TICK);
        for q in -50..50 {
            for r in -50..50 {
                let temp1 = t1.temperature_cached(q, r, &mut cache1);
                let temp2 = t2.temperature_cached(q, r, &mut cache2);
                assert_eq!(temp1, temp2);
            }
        }
    }

    #[test]
    fn different_seeds_differ() {
        let t1 = Terrain::new(0);
        let t2 = Terrain::new(99999);
        let mut differ = 0;
        let mut total = 0;
        // Sample a grid to cover diverse material provinces
        for q in (-10000..10000).step_by(2000) {
            for r in (-10000..10000).step_by(2000) {
                total += 1;
                let d1 = t1.material_density(q, r);
                let d2 = t2.material_density(q, r);
                if (d1 - d2).abs() > 0.01 {
                    differ += 1;
                }
            }
        }
        assert!(differ > total / 2,
            "Different seeds should produce mostly different material, got {}/{}", differ, total);
    }

    #[test]
    fn temperature_range() {
        let t = Terrain::new(42);
        let mut cache = ThermalChunkCache::new(42, DEFAULT_WORLD_TICK);
        for q in (-10000..10000).step_by(500) {
            for r in (-10000..10000).step_by(500) {
                let temp = t.temperature_cached(q, r, &mut cache);
                assert!(temp >= 0.0 && temp <= 1.0,
                    "Temperature {} at ({}, {}) out of [0, 1]", temp, q, r);
            }
        }
    }

    #[test]
    fn material_density_range() {
        let t = Terrain::new(42);
        for q in (-15000..15000).step_by(500) {
            for r in (-15000..15000).step_by(500) {
                let d = t.material_density(q, r);
                assert!(d >= 0.0 && d <= 1.0,
                    "Material density {} at ({}, {}) out of [0, 1]", d, q, r);
            }
        }
    }

    #[test]
    fn material_density_varies() {
        let t = Terrain::new(42);
        let mut min_d = 1.0;
        let mut max_d = 0.0;
        for q in (-2_500_000..2_500_000).step_by(25_000) {
            for r in (-2_500_000..2_500_000).step_by(25_000) {
                let d = t.material_density(q, r);
                if d < min_d { min_d = d; }
                if d > max_d { max_d = d; }
            }
        }
        let range = max_d - min_d;
        assert!(range > 0.2,
            "Material density should vary significantly, got range {} (min={}, max={})",
            range, min_d, max_d);
    }

    #[test]
    fn material_density_smooth() {
        let t = Terrain::new(42);
        let mut max_diff = 0.0;
        for q in (-5000..5000).step_by(100) {
            let d0 = t.material_density(q, 0);
            let d1 = t.material_density(q + 1, 0);
            let diff = (d1 - d0).abs();
            if diff > max_diff { max_diff = diff; }
        }
        assert!(max_diff < 0.01,
            "Adjacent tile density should be nearly identical, max diff = {}", max_diff);
    }

    #[test]
    fn material_density_deterministic() {
        let t1 = Terrain::new(42);
        let t2 = Terrain::new(42);
        for q in (-5000..5000).step_by(500) {
            for r in (-5000..5000).step_by(500) {
                assert_eq!(t1.material_density(q, r), t2.material_density(q, r));
            }
        }
    }

    #[test]
    fn gaussian_bleeds_past_province_boundary() {
        // Gaussian tails should carry heat into light regions near dense margins.
        let t = Terrain::new(42);
        let mut cache = ThermalChunkCache::new(42, DEFAULT_WORLD_TICK);
        let mut found = false;
        for q in (-20000..20000).step_by(100) {
            for r in (-20000..20000).step_by(100) {
                let density = t.material_density(q, r);
                if density < HOTSPOT_THRESHOLD {
                    let temp = t.temperature_cached(q, r, &mut cache);
                    if temp > 0.0 {
                        found = true;
                        break;
                    }
                }
            }
            if found { break; }
        }
        assert!(found,
            "Some light-region tiles near dense margins should have non-zero temperature from Gaussian bleed");
    }

    #[test]
    fn hotspot_temperature_cached_matches_uncached() {
        let t = Terrain::new(42);
        let mut cache = HotspotChunkCache::new(t.seed());
        let mut checked = 0;

        for q in (-5000..5000).step_by(200) {
            for r in (-5000..5000).step_by(200) {
                let uncached = t.hotspot_temperature(q, r);
                let cached = t.hotspot_temperature_cached(q, r, &mut cache);
                assert!(
                    (uncached - cached).abs() < f64::EPSILON,
                    "Mismatch at ({}, {}): uncached={}, cached={}",
                    q, r, uncached, cached,
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "Should have checked at least some tiles");
    }
}
