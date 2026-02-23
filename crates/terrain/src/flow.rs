use std::collections::{HashMap, HashSet};

use crate::hotspots::{hotspot_center, HOTSPOT_GRID_SPACING};
use crate::thermal::{
    ThermalSource, ThermalChunkCache, tile_to_thermal_chunk,
    THERMAL_SIGMA, GRID_CELLS_PER_CHUNK,
};

/// Compute the thermal flow vector at a world position.
/// Each source pushes material radially outward; the contribution is
/// the gradient of the Gaussian plume: I·(r/σ²)·exp(-r²/2σ²)·r̂.
pub fn flow_at(wx: f64, wy: f64, sources: &[ThermalSource]) -> (f64, f64) {
    let cutoff_sq = (3.0 * THERMAL_SIGMA) * (3.0 * THERMAL_SIGMA);
    let sigma_sq = THERMAL_SIGMA * THERMAL_SIGMA;
    let mut fx = 0.0;
    let mut fy = 0.0;

    for source in sources {
        let dx = wx - source.x;
        let dy = wy - source.y;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq < 0.001 || dist_sq > cutoff_sq {
            continue;
        }

        let dist = dist_sq.sqrt();
        let grad = source.intensity * (dist / sigma_sq)
            * (-dist_sq / (2.0 * sigma_sq)).exp();
        fx += grad * (dx / dist);
        fy += grad * (dy / dist);
    }

    (fx, fy)
}

/// Flow vector at a hex tile, using a shared thermal chunk cache.
/// Computes flow at the exact tile position (not grid-snapped).
pub fn flow_at_tile(q: i32, r: i32, cache: &mut ThermalChunkCache) -> (f64, f64) {
    let (wx, wy) = crate::hex_to_world(q, r);
    let (cq, cr) = tile_to_thermal_chunk(q, r);
    let sources = cache.gather_sources(cq, cr);
    flow_at(wx, wy, &sources)
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

/// Cache of precomputed flow vectors at hotspot grid point centers.
/// Grid points are spaced at HOTSPOT_GRID_SPACING intervals; flow is computed
/// once per grid point from thermal sources, then interpolated by world position.
pub struct FlowChunkCache {
    vectors: HashMap<(i32, i32), (f64, f64)>,
    computed: HashSet<(i32, i32)>,
    thermal_cache: ThermalChunkCache,
}

impl FlowChunkCache {
    pub fn new(seed: u64, world_tick: u64) -> Self {
        Self {
            vectors: HashMap::new(),
            computed: HashSet::new(),
            thermal_cache: ThermalChunkCache::new(seed, world_tick),
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
                let flow = flow_at(wx, wy, &sources);
                self.vectors.insert((gq, gr), flow);
            }
        }
    }

    /// Get or compute the raw flow vector at a single grid point.
    fn get_or_compute(&mut self, gq: i32, gr: i32) -> (f64, f64) {
        if let Some(&flow) = self.vectors.get(&(gq, gr)) {
            return flow;
        }
        let (tcq, tcr) = grid_cell_thermal_chunk(gq, gr);
        self.ensure_thermal_chunk(tcq, tcr);
        self.vectors.get(&(gq, gr)).copied().unwrap_or((0.0, 0.0))
    }

    /// Interpolated flow at an arbitrary world position.
    /// IDW average of cached flow vectors at the enclosing grid cell
    /// plus its 6 hex neighbors (7 equidistant points). Using the hex
    /// neighborhood instead of a 3×3 QR box avoids pulling interpolation
    /// toward the two far-diagonal points at distance 750√3.
    pub fn flow_at(&mut self, wx: f64, wy: f64) -> (f64, f64) {
        let (gq, gr) = crate::cart_to_grid_cell(wx, wy);

        // Self + 6 hex neighbors (all at equal grid spacing)
        const HEX_OFFSETS: [(i32, i32); 7] = [
            (0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1),
        ];

        let mut fx_sum = 0.0;
        let mut fy_sum = 0.0;
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

            let (fx, fy) = self.get_or_compute(ngq, ngr);
            let w = 1.0 / dist_sq;
            fx_sum += w * fx;
            fy_sum += w * fy;
            w_sum += w;
        }

        if w_sum > 0.0 {
            (fx_sum / w_sum, fy_sum / w_sum)
        } else {
            (0.0, 0.0)
        }
    }

    /// Interpolated flow vector at a hex tile.
    pub fn flow_at_tile(&mut self, q: i32, r: i32) -> (f64, f64) {
        let (wx, wy) = crate::hex_to_world(q, r);
        self.flow_at(wx, wy)
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotspots::DEFAULT_WORLD_TICK;

    #[test]
    fn empty_sources_zero_flow() {
        let (fx, fy) = flow_at(0.0, 0.0, &[]);
        assert_eq!(fx, 0.0);
        assert_eq!(fy, 0.0);
    }

    #[test]
    fn flow_points_away_from_source() {
        let sources = vec![ThermalSource { x: 0.0, y: 0.0, intensity: 0.1 }];
        let (fx, fy) = flow_at(THERMAL_SIGMA, 0.0, &sources);
        assert!(fx > 0.0, "Flow should point away (positive x), got {fx}");
        assert!(fy.abs() < 1e-15, "Flow should have no y component, got {fy}");
    }

    #[test]
    fn flow_magnitude_peaks_at_sigma() {
        let sources = vec![ThermalSource { x: 0.0, y: 0.0, intensity: 0.1 }];
        let at_half_sigma = flow_at(THERMAL_SIGMA * 0.5, 0.0, &sources).0;
        let at_sigma = flow_at(THERMAL_SIGMA, 0.0, &sources).0;
        let at_two_sigma = flow_at(THERMAL_SIGMA * 2.0, 0.0, &sources).0;
        assert!(at_sigma > at_half_sigma,
            "Flow at σ ({at_sigma}) should exceed flow at σ/2 ({at_half_sigma})");
        assert!(at_sigma > at_two_sigma,
            "Flow at σ ({at_sigma}) should exceed flow at 2σ ({at_two_sigma})");
    }

    #[test]
    fn opposing_sources_cancel_at_midpoint() {
        let sources = vec![
            ThermalSource { x: -1000.0, y: 0.0, intensity: 0.1 },
            ThermalSource { x: 1000.0, y: 0.0, intensity: 0.1 },
        ];
        let (fx, fy) = flow_at(0.0, 0.0, &sources);
        assert!(fx.abs() < 1e-15, "Symmetric sources should cancel x flow, got {fx}");
        assert!(fy.abs() < 1e-15, "Symmetric sources should cancel y flow, got {fy}");
    }

    #[test]
    fn beyond_cutoff_is_zero() {
        let sources = vec![ThermalSource { x: 0.0, y: 0.0, intensity: 0.1 }];
        let far = 3.0 * THERMAL_SIGMA + 1.0;
        let (fx, fy) = flow_at(far, 0.0, &sources);
        assert_eq!(fx, 0.0);
        assert_eq!(fy, 0.0);
    }

    #[test]
    fn flow_scales_with_intensity() {
        let weak = vec![ThermalSource { x: 0.0, y: 0.0, intensity: 0.01 }];
        let strong = vec![ThermalSource { x: 0.0, y: 0.0, intensity: 0.1 }];
        let (fx_w, _) = flow_at(THERMAL_SIGMA, 0.0, &weak);
        let (fx_s, _) = flow_at(THERMAL_SIGMA, 0.0, &strong);
        let ratio = fx_s / fx_w;
        assert!((ratio - 10.0).abs() < 1e-10,
            "10× intensity should give 10× flow, ratio = {ratio}");
    }

    /// At an exact grid center, the interpolated lookup should return
    /// the raw cached value (early-return path), which must match
    /// direct computation from thermal sources.
    #[test]
    fn cache_exact_at_grid_centers() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut flow_cache = FlowChunkCache::new(seed, tick);
        let mut thermal_cache = ThermalChunkCache::new(seed, tick);

        for gq in -10..10 {
            for gr in -10..10 {
                let (wx, wy) = hotspot_center(gq, gr);
                let cached = flow_cache.flow_at(wx, wy);

                let (fq, fr) = crate::cart_to_hex(wx, wy);
                let (hq, hr) = crate::hex_round(fq, fr);
                let (tcq, tcr) = tile_to_thermal_chunk(hq, hr);
                let sources = thermal_cache.gather_sources(tcq, tcr);
                let direct = flow_at(wx, wy, &sources);

                assert_eq!(cached, direct,
                    "Cache mismatch at grid cell ({gq}, {gr}): cached={cached:?}, direct={direct:?}");
            }
        }
    }

    #[test]
    fn cache_is_deterministic() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut cache1 = FlowChunkCache::new(seed, tick);
        let mut cache2 = FlowChunkCache::new(seed, tick);

        for q in (-5000..5000).step_by(500) {
            for r in (-5000..5000).step_by(500) {
                let (fx1, fy1) = cache1.flow_at_tile(q, r);
                let (fx2, fy2) = cache2.flow_at_tile(q, r);
                assert_eq!(fx1, fx2, "Cache not deterministic at ({q}, {r})");
                assert_eq!(fy1, fy2, "Cache not deterministic at ({q}, {r})");
            }
        }
    }

    #[test]
    fn cache_nonzero_near_sources() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut flow_cache = FlowChunkCache::new(seed, tick);
        let mut thermal_cache = ThermalChunkCache::new(seed, tick);
        let mut found = false;

        for q in (-10000..10000).step_by(500) {
            for r in (-10000..10000).step_by(500) {
                let temp = thermal_cache.temperature_at_tile(q, r);
                if temp > 0.05 {
                    let (fx, fy) = flow_cache.flow_at_tile(q, r);
                    let mag = (fx * fx + fy * fy).sqrt();
                    if mag > 0.0 {
                        found = true;
                        break;
                    }
                }
            }
            if found { break; }
        }
        assert!(found, "Should find non-zero cached flow near thermal sources");
    }

    /// Interpolation must produce smooth transitions — adjacent sample
    /// points at sub-grid resolution should have similar flow vectors,
    /// with no discontinuities at grid cell boundaries.
    #[test]
    fn interpolation_is_smooth() {
        let seed = 42u64;
        let tick = DEFAULT_WORLD_TICK;
        let mut cache = FlowChunkCache::new(seed, tick);

        // Find a grid cell with non-zero flow
        let mut base_gq = 0;
        let mut base_gr = 0;
        let mut found = false;
        'outer: for gq in -20..20 {
            for gr in -20..20 {
                let (fx, fy) = cache.get_or_compute(gq, gr);
                if fx * fx + fy * fy > 1e-20 {
                    base_gq = gq;
                    base_gr = gr;
                    found = true;
                    break 'outer;
                }
            }
        }
        assert!(found, "Need a non-zero flow grid cell for smoothness test");

        // Sample flow along a line crossing this grid cell at 50-tile intervals
        // (much finer than the 750-tile grid spacing)
        let (cx, cy) = hotspot_center(base_gq, base_gr);
        let step = 50.0;
        let mut prev: Option<(f64, f64)> = None;
        let mut max_change = 0.0;
        for i in -10..=10 {
            let wx = cx + i as f64 * step;
            let wy = cy;
            let (fx, fy) = cache.flow_at(wx, wy);
            if let Some((pfx, pfy)) = prev {
                let dfx = fx - pfx;
                let dfy = fy - pfy;
                let change = (dfx * dfx + dfy * dfy).sqrt();
                if change > max_change {
                    max_change = change;
                }
            }
            prev = Some((fx, fy));
        }

        // With IDW interpolation, 50-tile steps (1/15 of grid spacing) should
        // produce gradual changes. Without interpolation, grid-snapping would
        // cause jumps equal to the full flow difference between adjacent cells.
        assert!(max_change < 5e-5,
            "Flow should change smoothly between 50-tile samples, max change = {max_change}");
    }

    /// grid_cell_thermal_chunk must agree with thermal.rs's grid_cell_to_chunk.
    /// Both must produce the same chunk assignment, proving the shared
    /// tile_to_hex_chunk path is equivalent to the direct hex_round formula.
    #[test]
    fn grid_cell_chunk_agrees_with_thermal() {
        use crate::thermal::grid_cell_to_chunk;
        for gq in -20..20 {
            for gr in -20..20 {
                let ours = grid_cell_thermal_chunk(gq, gr);
                let theirs = grid_cell_to_chunk(gq, gr);
                assert_eq!(ours, theirs,
                    "grid cell ({gq}, {gr}): tile_to_hex_chunk={ours:?}, grid_cell_to_chunk={theirs:?}");
            }
        }
    }
}
