use std::collections::{HashMap, HashSet};

use crate::hotspots::{hotspot_center, HOTSPOT_GRID_SPACING};
use crate::material::material_density_cart;
use crate::thermal::{
    ThermalChunkCache, temperature_at,
    tile_to_thermal_chunk, GRID_CELLS_PER_CHUNK,
};

/// Maximum crust thickness (normalized to [0, 1]).
const CRUST_MAX_THICKNESS: f64 = 1.0;

/// How aggressively thermal energy suppresses crust formation.
/// Higher = thinner crust near heat sources.
const THERMAL_SUPPRESSION_RATE: f64 = 6.0;

/// Fraction of cooling retained at a given thermal intensity.
/// 1.0 at zero thermal, decays exponentially with heat.
fn cooling_factor(thermal: f64) -> f64 {
    (-thermal * THERMAL_SUPPRESSION_RATE).exp()
}

/// Crust thickness from material supply and thermal inhibition.
/// Dense + cold = thick crust. Hot or sparse = thin/absent.
pub fn crust_thickness(material_density: f64, thermal: f64) -> f64 {
    material_density * cooling_factor(thermal) * CRUST_MAX_THICKNESS
}

// ──── Chunk Cache ────

/// Map a hotspot grid cell to its thermal chunk via the shared hex Voronoi
/// utility. Computes the grid cell's center tile, then delegates to
/// `tile_to_thermal_chunk` (which calls `tile_to_hex_chunk`).
fn grid_cell_thermal_chunk(gq: i32, gr: i32) -> (i32, i32) {
    let spacing = HOTSPOT_GRID_SPACING as i32;
    let tq = gq * spacing + spacing / 2;
    let tr = gr * spacing + spacing / 2;
    tile_to_thermal_chunk(tq, tr)
}

/// Cache of precomputed crust thickness values at hotspot grid point centers.
/// Grid points are spaced at HOTSPOT_GRID_SPACING intervals; crust is computed
/// once per grid point from material density and thermal sources, then
/// interpolated by world position.
pub struct CrustChunkCache {
    values: HashMap<(i32, i32), f64>,
    computed: HashSet<(i32, i32)>,
    thermal_cache: ThermalChunkCache,
    seed: u64,
}

impl CrustChunkCache {
    pub fn new(seed: u64, world_tick: u64) -> Self {
        Self {
            values: HashMap::new(),
            computed: HashSet::new(),
            thermal_cache: ThermalChunkCache::new(seed, world_tick),
            seed,
        }
    }

    fn ensure_thermal_chunk(&mut self, tcq: i32, tcr: i32) {
        if self.computed.contains(&(tcq, tcr)) {
            return;
        }
        self.computed.insert((tcq, tcr));

        let sources = self.thermal_cache.gather_sources(tcq, tcr);

        let gcpc = GRID_CELLS_PER_CHUNK;
        let center_gq = tcq * gcpc;
        let center_gr = tcr * gcpc;
        let reach = gcpc / 2 + 1;

        for dgq in -reach..=reach {
            for dgr in -reach..=reach {
                let gq = center_gq + dgq;
                let gr = center_gr + dgr;
                if grid_cell_thermal_chunk(gq, gr) != (tcq, tcr) {
                    continue;
                }
                let (wx, wy) = hotspot_center(gq, gr);
                let density = material_density_cart(wx, wy, self.seed);
                let thermal = temperature_at(wx, wy, &sources);
                let thickness = crust_thickness(density, thermal);
                self.values.insert((gq, gr), thickness);
            }
        }
    }

    /// Get or compute the raw crust thickness at a single grid point.
    fn get_or_compute(&mut self, gq: i32, gr: i32) -> f64 {
        if let Some(&v) = self.values.get(&(gq, gr)) {
            return v;
        }
        let (tcq, tcr) = grid_cell_thermal_chunk(gq, gr);
        self.ensure_thermal_chunk(tcq, tcr);
        self.values.get(&(gq, gr)).copied().unwrap_or(0.0)
    }

    /// Interpolated crust thickness at an arbitrary world position.
    /// IDW average of cached values at the enclosing grid cell
    /// plus its 6 hex neighbors (7 equidistant points).
    pub fn crust_at(&mut self, wx: f64, wy: f64) -> f64 {
        let (gq, gr) = crate::cart_to_grid_cell(wx, wy);

        const HEX_OFFSETS: [(i32, i32); 7] = [
            (0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1),
        ];

        let mut val_sum = 0.0;
        let mut w_sum = 0.0;

        for &(dq, dr) in &HEX_OFFSETS {
            let ngq = gq + dq;
            let ngr = gr + dr;
            let (cx, cy) = hotspot_center(ngq, ngr);
            let dx = wx - cx;
            let dy = wy - cy;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq < 1.0 {
                return self.get_or_compute(ngq, ngr);
            }

            let v = self.get_or_compute(ngq, ngr);
            let w = 1.0 / dist_sq;
            val_sum += w * v;
            w_sum += w;
        }

        if w_sum > 0.0 {
            val_sum / w_sum
        } else {
            0.0
        }
    }

    /// Interpolated crust thickness at a hex tile.
    pub fn crust_at_tile(&mut self, q: i32, r: i32) -> f64 {
        let (wx, wy) = crate::hex_to_world(q, r);
        self.crust_at(wx, wy)
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotspots::DEFAULT_WORLD_TICK;

    #[test]
    fn crust_thin_at_low_density() {
        // Low material density → thin crust even with zero thermal
        let thickness = crust_thickness(0.1, 0.0);
        assert!(thickness < 0.15,
            "Low density should produce thin crust, got {thickness}");
        assert!(thickness > 0.0,
            "Non-zero density should produce some crust");
    }

    #[test]
    fn crust_zero_at_high_thermal() {
        // Very high thermal intensity → cooling factor ≈ 0 → no crust
        let thickness = crust_thickness(1.0, 5.0);
        assert!(thickness < 0.01,
            "Extreme thermal should suppress crust to near zero, got {thickness}");
    }

    #[test]
    fn crust_maximum_at_cold_dense() {
        // Maximum density + zero thermal → maximum crust
        let thickness = crust_thickness(1.0, 0.0);
        assert!((thickness - CRUST_MAX_THICKNESS).abs() < f64::EPSILON,
            "Dense + cold should produce max crust, got {thickness}");
    }

    #[test]
    fn crust_monotonic_with_density() {
        // At fixed thermal, crust should increase with density
        let thermal = 0.1;
        let mut prev = 0.0;
        for i in 0..=10 {
            let density = i as f64 / 10.0;
            let thickness = crust_thickness(density, thermal);
            assert!(thickness >= prev,
                "Crust should increase with density: density={density}, got {thickness} < {prev}");
            prev = thickness;
        }
    }

    #[test]
    fn crust_monotonic_with_cooling() {
        // At fixed density, crust should decrease as thermal increases
        let density = 0.8;
        let mut prev = f64::MAX;
        for i in 0..=10 {
            let thermal = i as f64 / 10.0;
            let thickness = crust_thickness(density, thermal);
            assert!(thickness <= prev,
                "Crust should decrease with thermal: thermal={thermal}, got {thickness} > {prev}");
            prev = thickness;
        }
    }

    #[test]
    fn crust_cache_matches_direct() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut crust_cache = CrustChunkCache::new(seed, tick);
        let mut thermal_cache = ThermalChunkCache::new(seed, tick);

        for gq in -10..10 {
            for gr in -10..10 {
                let (wx, wy) = hotspot_center(gq, gr);
                let cached = crust_cache.get_or_compute(gq, gr);

                let density = material_density_cart(wx, wy, seed);
                let (fq, fr) = crate::cart_to_hex(wx, wy);
                let (hq, hr) = crate::hex_round(fq, fr);
                let (tcq, tcr) = tile_to_thermal_chunk(hq, hr);
                let sources = thermal_cache.gather_sources(tcq, tcr);
                let thermal = temperature_at(wx, wy, &sources);
                let direct = crust_thickness(density, thermal);

                assert_eq!(cached, direct,
                    "Cache mismatch at grid cell ({gq}, {gr}): cached={cached}, direct={direct}");
            }
        }
    }

    #[test]
    fn crust_interpolation_smooth() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut cache = CrustChunkCache::new(seed, tick);

        // Find a grid cell with non-trivial crust
        let mut base_gq = 0;
        let mut base_gr = 0;
        let mut found = false;
        'outer: for gq in -20..20 {
            for gr in -20..20 {
                let v = cache.get_or_compute(gq, gr);
                if v > 0.05 {
                    base_gq = gq;
                    base_gr = gr;
                    found = true;
                    break 'outer;
                }
            }
        }
        assert!(found, "Need a non-trivial crust grid cell for smoothness test");

        // Sample along a line crossing this grid cell at 50-tile intervals
        let (cx, cy) = hotspot_center(base_gq, base_gr);
        let step = 50.0;
        let mut prev: Option<f64> = None;
        let mut max_change = 0.0;
        for i in -10..=10 {
            let wx = cx + i as f64 * step;
            let wy = cy;
            let v = cache.crust_at(wx, wy);
            if let Some(pv) = prev {
                let change = (v - pv).abs();
                if change > max_change {
                    max_change = change;
                }
            }
            prev = Some(v);
        }

        assert!(max_change < 0.05,
            "Crust should change smoothly between 50-tile samples, max change = {max_change}");
    }
}
