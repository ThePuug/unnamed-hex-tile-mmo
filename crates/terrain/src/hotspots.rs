use std::collections::{HashMap, HashSet};

use crate::material::material_density_cart;
use crate::{hex_to_cart, cart_to_hex, div_floor};

/// Material density above which sub-lid convection cells exist.
/// Below this → quiescent mantle, no hotspots.
pub(crate) const HOTSPOT_THRESHOLD: f64 = 0.55;

/// Fixed spacing for the hotspot cell grid. All candidates placed on this grid.
/// Cell size variation emerges from which grid points are active, not from variable spacing.
/// Half the previous 1500 — finer granularity, 4× more grid points per area.
pub(crate) const HOTSPOT_GRID_SPACING: f64 = 750.0;

/// Number of ticks for one full hotspot lifecycle.
pub(crate) const HOTSPOT_CYCLE_TICKS: u64 = 1000;

/// Default world time in ticks. Overridden by Terrain::with_tick().
pub(crate) const DEFAULT_WORLD_TICK: u64 = 0;

/// Grid position for a hotspot cell on the fixed HOTSPOT_GRID_SPACING grid.
pub(crate) fn hotspot_center(cell_q: i32, cell_r: i32) -> (f64, f64) {
    let q = cell_q as f64 * HOTSPOT_GRID_SPACING + HOTSPOT_GRID_SPACING * 0.5;
    let r = cell_r as f64 * HOTSPOT_GRID_SPACING + HOTSPOT_GRID_SPACING * 0.5;
    hex_to_cart(q, r)
}

/// Find nearest active hotspot cell center (density >= HOTSPOT_THRESHOLD on the fixed grid).
/// Returns Some((x, y, cell_q, cell_r, dist)) or None if no active cell nearby.
pub(crate) fn nearest_hotspot(wx: f64, wy: f64, seed: u64) -> Option<(f64, f64, i32, i32, f64)> {
    let (approx_q, approx_r) = cart_to_hex(wx, wy);
    let gq = div_floor(approx_q as i64, HOTSPOT_GRID_SPACING as i64) as i32;
    let gr = div_floor(approx_r as i64, HOTSPOT_GRID_SPACING as i64) as i32;

    let mut best_dist_sq = f64::MAX;
    let mut best: Option<(f64, f64, i32, i32)> = None;

    for dq in -2..=2 {
        for dr in -2..=2 {
            let cq = gq + dq;
            let cr = gr + dr;
            let (cx, cy) = hotspot_center(cq, cr);

            // Only active if dense enough
            let density = material_density_cart(cx, cy, seed);
            if density < HOTSPOT_THRESHOLD {
                continue;
            }

            let dx = wx - cx;
            let dy = wy - cy;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best = Some((cx, cy, cq, cr));
            }
        }
    }

    best.map(|(cx, cy, cq, cr)| (cx, cy, cq, cr, best_dist_sq.sqrt()))
}

/// Asymmetric hotspot lifecycle: 60% slow quadratic rise, 10% peak plateau,
/// 30% fast quadratic collapse. Returns [0, 1].
pub(crate) fn hotspot_lifecycle(phase: f64) -> f64 {
    if phase < 0.6 {
        let t = phase / 0.6;
        t * t
    } else if phase < 0.7 {
        1.0
    } else {
        let t = (phase - 0.7) / 0.3;
        (1.0 - t) * (1.0 - t)
    }
}

/// Deterministic phase offset for a hotspot cell. Returns [0, 1).
pub(crate) fn hotspot_phase_offset(cell_q: i32, cell_r: i32, seed: u64) -> f64 {
    let h = seed
        .wrapping_mul(0x517cc1b727220a95)
        .wrapping_add(cell_q as u64)
        .wrapping_mul(0xff51afd7ed558ccd)
        .wrapping_add(cell_r as u64);
    let h = h ^ (h >> 33);
    let h = h.wrapping_mul(0x62a9d9ed799705f5);
    let h = h ^ (h >> 28);
    (h & 0xFFFF_FFFF) as f64 / 0x1_0000_0000_u64 as f64
}

// ──── Chunk Cache ────

/// A grid cell that has been confirmed active (density >= threshold).
#[derive(Clone, Copy, Debug)]
pub struct HotspotCell {
    pub cell_q: i32,
    pub cell_r: i32,
    pub center_x: f64,
    pub center_y: f64,
}

/// Convert world cartesian coordinates to the hotspot grid cell that contains them.
pub fn cart_to_grid_cell(wx: f64, wy: f64) -> (i32, i32) {
    let (approx_q, approx_r) = cart_to_hex(wx, wy);
    let gq = div_floor(approx_q as i64, HOTSPOT_GRID_SPACING as i64) as i32;
    let gr = div_floor(approx_r as i64, HOTSPOT_GRID_SPACING as i64) as i32;
    (gq, gr)
}

/// Check if a grid cell is active (material density >= threshold at its center).
fn precompute_cell(cell_q: i32, cell_r: i32, seed: u64) -> Option<HotspotCell> {
    let (cx, cy) = hotspot_center(cell_q, cell_r);
    if material_density_cart(cx, cy, seed) >= HOTSPOT_THRESHOLD {
        Some(HotspotCell { cell_q, cell_r, center_x: cx, center_y: cy })
    } else {
        None
    }
}

/// Approximate hex radius of a chunk in tiles. Must be large enough
/// that a hex 1-ring of chunks always contains the nearest active grid cell
/// for any tile in the center chunk.
/// Matches HOTSPOT_GRID_SPACING so one grid cell fits per chunk radius.
pub const CHUNK_RADIUS: i32 = 750;

/// Spacing between chunk centers in tile coordinates.
const CHUNK_SPACING: i32 = 2 * CHUNK_RADIUS;

/// Grid cells from a chunk center's grid cell to evaluate in each direction.
/// Derived: ceil(CHUNK_RADIUS / HOTSPOT_GRID_SPACING).
const GRID_CELL_REACH: i32 = {
    let s = HOTSPOT_GRID_SPACING as i32;
    (CHUNK_RADIUS + s - 1) / s
};

/// Hex neighbor offsets for a 1-ring of chunks.
const HEX_NEIGHBORS: [(i32, i32); 6] = [
    (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1),
];

/// Map a tile (q, r) to its chunk coordinate using hexagonal Voronoi regions.
/// Each tile is assigned to the nearest chunk center in hex distance.
pub fn tile_to_chunk(q: i32, r: i32) -> (i32, i32) {
    crate::tile_to_hex_chunk(q, r, CHUNK_SPACING)
}

/// Grid cell nearest to the center of a chunk.
/// Chunk center is at tile (chunk_q * CHUNK_SPACING, chunk_r * CHUNK_SPACING).
fn chunk_center_grid_cell(chunk_q: i32, chunk_r: i32) -> (i32, i32) {
    let center_q = chunk_q as i64 * CHUNK_SPACING as i64;
    let center_r = chunk_r as i64 * CHUNK_SPACING as i64;
    (
        div_floor(center_q, HOTSPOT_GRID_SPACING as i64) as i32,
        div_floor(center_r, HOTSPOT_GRID_SPACING as i64) as i32,
    )
}

/// Cache of precomputed active hotspot grid cells.
/// Eliminates redundant `material_density_cart` noise evaluations
/// by storing which grid cells are active across tile lookups.
pub struct HotspotChunkCache {
    active_cells: HashMap<(i32, i32), HotspotCell>,
    evaluated: HashSet<(i32, i32)>,
    seed: u64,
}

impl HotspotChunkCache {
    pub fn new(seed: u64) -> Self {
        Self {
            active_cells: HashMap::new(),
            evaluated: HashSet::new(),
            seed,
        }
    }

    /// Evaluate all grid cells covered by this chunk + 6 hex-neighbor chunks.
    /// Skips already-evaluated cells.
    pub fn ensure_neighborhood(&mut self, chunk_q: i32, chunk_r: i32) {
        self.ensure_chunk_cells(chunk_q, chunk_r);
        for (dq, dr) in HEX_NEIGHBORS {
            self.ensure_chunk_cells(chunk_q + dq, chunk_r + dr);
        }
    }

    fn ensure_chunk_cells(&mut self, chunk_q: i32, chunk_r: i32) {
        let (gq, gr) = chunk_center_grid_cell(chunk_q, chunk_r);
        for dq in -GRID_CELL_REACH..=GRID_CELL_REACH {
            for dr in -GRID_CELL_REACH..=GRID_CELL_REACH {
                self.ensure_cell(gq + dq, gr + dr);
            }
        }
    }

    fn ensure_cell(&mut self, cq: i32, cr: i32) {
        if self.evaluated.contains(&(cq, cr)) {
            return;
        }
        self.evaluated.insert((cq, cr));
        if let Some(cell) = precompute_cell(cq, cr, self.seed) {
            self.active_cells.insert((cq, cr), cell);
        }
    }

    /// Collect active cells from this chunk + 6 hex-neighbor chunks.
    pub fn gather_neighborhood(&self, chunk_q: i32, chunk_r: i32) -> Vec<HotspotCell> {
        let mut result = Vec::new();
        self.gather_chunk_cells(chunk_q, chunk_r, &mut result);
        for (dq, dr) in HEX_NEIGHBORS {
            self.gather_chunk_cells(chunk_q + dq, chunk_r + dr, &mut result);
        }
        result
    }

    fn gather_chunk_cells(&self, chunk_q: i32, chunk_r: i32, out: &mut Vec<HotspotCell>) {
        let (gq, gr) = chunk_center_grid_cell(chunk_q, chunk_r);
        for dq in -GRID_CELL_REACH..=GRID_CELL_REACH {
            for dr in -GRID_CELL_REACH..=GRID_CELL_REACH {
                if let Some(&c) = self.active_cells.get(&(gq + dq, gr + dr)) {
                    out.push(c);
                }
            }
        }
    }
}

/// Same as `nearest_hotspot` but uses the cache instead of brute-force noise evaluation.
pub(crate) fn nearest_hotspot_cached(
    wx: f64,
    wy: f64,
    cache: &mut HotspotChunkCache,
) -> Option<(f64, f64, i32, i32, f64)> {
    let (hq, hr) = cart_to_hex(wx, wy);
    let (cq, cr) = tile_to_chunk(hq as i32, hr as i32);
    cache.ensure_neighborhood(cq, cr);
    let candidates = cache.gather_neighborhood(cq, cr);

    let mut best_dist_sq = f64::MAX;
    let mut best: Option<(f64, f64, i32, i32)> = None;

    for c in candidates {
        let dx = wx - c.center_x;
        let dy = wy - c.center_y;
        let dist_sq = dx * dx + dy * dy;
        // Tie-break by lexicographic (cell_q, cell_r) to match brute-force iteration order
        if dist_sq < best_dist_sq
            || (dist_sq == best_dist_sq
                && best.map_or(true, |(_, _, bq, br)| (c.cell_q, c.cell_r) < (bq, br)))
        {
            best_dist_sq = dist_sq;
            best = Some((c.center_x, c.center_y, c.cell_q, c.cell_r));
        }
    }

    best.map(|(cx, cy, cq, cr)| (cx, cy, cq, cr, best_dist_sq.sqrt()))
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotspot_lifecycle_range() {
        for i in 0..1000 {
            let phase = i as f64 / 1000.0;
            let v = hotspot_lifecycle(phase);
            assert!(v >= 0.0 && v <= 1.0,
                "hotspot_lifecycle({}) = {} out of [0, 1]", phase, v);
        }
    }

    #[test]
    fn hotspot_lifecycle_asymmetric() {
        // Early rise is slow (quadratic)
        let early = hotspot_lifecycle(0.15);
        assert!(early < 0.1, "Early rise should be slow, got {}", early);

        // Late rise approaches 1.0
        let late_rise = hotspot_lifecycle(0.55);
        assert!(late_rise > 0.7, "Late rise should be high, got {}", late_rise);

        // Peak plateau
        assert_eq!(hotspot_lifecycle(0.65), 1.0);

        // Collapse is fast — halfway through collapse should still be fairly high
        let mid_collapse = hotspot_lifecycle(0.85);
        assert!(mid_collapse > 0.15 && mid_collapse < 0.5,
            "Mid-collapse should be moderate, got {}", mid_collapse);

        // End of collapse near zero
        let end = hotspot_lifecycle(0.99);
        assert!(end < 0.02, "End of collapse should be near zero, got {}", end);
    }

    #[test]
    fn hotspot_phase_offset_deterministic() {
        for cq in -10..10 {
            for cr in -10..10 {
                let a = hotspot_phase_offset(cq, cr, 42);
                let b = hotspot_phase_offset(cq, cr, 42);
                assert_eq!(a, b, "Phase offset should be deterministic for ({}, {})", cq, cr);
            }
        }
    }

    #[test]
    fn hotspot_phase_offset_varies() {
        let mut offsets = std::collections::HashSet::new();
        for cq in 0..20 {
            for cr in 0..20 {
                let bits = hotspot_phase_offset(cq, cr, 42).to_bits();
                offsets.insert(bits);
            }
        }
        assert!(offsets.len() > 350,
            "400 cells should produce >350 unique offsets, got {}", offsets.len());
    }

    #[test]
    fn precompute_cell_active_matches_density() {
        let seed = 42u64;
        // Sample grid cells and verify precompute_cell agrees with manual density check.
        for cq in -5..5 {
            for cr in -5..5 {
                let (cx, cy) = hotspot_center(cq, cr);
                let density = material_density_cart(cx, cy, seed);
                let result = precompute_cell(cq, cr, seed);
                if density >= HOTSPOT_THRESHOLD {
                    let cell = result.expect(
                        &format!("Cell ({}, {}) has density {} >= threshold but precompute returned None",
                            cq, cr, density));
                    assert_eq!(cell.cell_q, cq);
                    assert_eq!(cell.cell_r, cr);
                    assert_eq!(cell.center_x, cx);
                    assert_eq!(cell.center_y, cy);
                } else {
                    assert!(result.is_none(),
                        "Cell ({}, {}) has density {} < threshold but precompute returned Some",
                        cq, cr, density);
                }
            }
        }
    }

    #[test]
    fn cache_ensure_neighborhood_populates() {
        let seed = 42u64;
        let mut cache = HotspotChunkCache::new(seed);
        cache.ensure_neighborhood(0, 0);

        // 7 chunks × (2*GRID_CELL_REACH+1)² grid cells per chunk (before dedup)
        let side = (2 * GRID_CELL_REACH + 1) as usize;
        let cells_per_chunk = side * side;
        // With dedup, actual count ≤ 7 * cells_per_chunk (overlapping chunks share cells)
        assert!(cache.evaluated.len() > 0 && cache.evaluated.len() <= 7 * cells_per_chunk);

        // gather should return only active cells
        let gathered = cache.gather_neighborhood(0, 0);
        for c in &gathered {
            let (cx, cy) = hotspot_center(c.cell_q, c.cell_r);
            let density = material_density_cart(cx, cy, seed);
            assert!(density >= HOTSPOT_THRESHOLD,
                "Gathered cell ({}, {}) has density {} below threshold",
                c.cell_q, c.cell_r, density);
        }
    }

    #[test]
    fn cache_idempotent() {
        let seed = 42u64;
        let mut cache = HotspotChunkCache::new(seed);
        cache.ensure_neighborhood(0, 0);
        let count_after_first = cache.evaluated.len();
        let active_after_first = cache.active_cells.len();

        cache.ensure_neighborhood(0, 0);
        assert_eq!(cache.evaluated.len(), count_after_first);
        assert_eq!(cache.active_cells.len(), active_after_first);
    }

    #[test]
    fn cache_overlapping_neighborhoods_share_cells() {
        let seed = 42u64;
        let mut cache = HotspotChunkCache::new(seed);
        cache.ensure_neighborhood(0, 0);
        let count_after_origin = cache.evaluated.len();

        cache.ensure_neighborhood(1, 0);
        assert!(cache.evaluated.len() > count_after_origin,
            "Adjacent neighborhood should add new cells");
        // Two neighborhoods share some chunks, so total < 2× first
        assert!(cache.evaluated.len() < 2 * count_after_origin,
            "Overlapping neighborhoods should share cells, got {} evaluated", cache.evaluated.len());
    }

    /// Invariant: gather_neighborhood must contain the nearest active cell
    /// that the brute-force ±2 search finds. If this fails, the neighborhood
    /// ring is too small for the current grid spacing / density wavelengths.
    /// Checked across multiple seeds to guard against parameter tuning regressions.
    #[test]
    fn neighborhood_contains_nearest_cell() {
        for seed in [0u64, 42, 12345, 99999] {
            let mut cache = HotspotChunkCache::new(seed);
            let mut checked = 0;

            for q in (-15000..15000).step_by(250) {
                for r in (-15000..15000).step_by(250) {
                    let (wx, wy) = crate::hex_to_world(q, r);
                    if material_density_cart(wx, wy, seed) < HOTSPOT_THRESHOLD {
                        continue;
                    }

                    let brute = match nearest_hotspot(wx, wy, seed) {
                        Some(v) => v,
                        None => continue,
                    };
                    let (_, _, brute_cq, brute_cr, _) = brute;

                    let (cq, cr) = tile_to_chunk(q, r);
                    cache.ensure_neighborhood(cq, cr);
                    let neighbors = cache.gather_neighborhood(cq, cr);

                    assert!(
                        neighbors.iter().any(|c| c.cell_q == brute_cq && c.cell_r == brute_cr),
                        "seed={seed}: tile ({q}, {r}), chunk ({cq}, {cr}) — \
                         brute-force nearest cell ({brute_cq}, {brute_cr}) not in \
                         gather_neighborhood (got {} cells). \
                         CHUNK_RADIUS={} is too small.",
                        neighbors.len(), CHUNK_RADIUS,
                    );
                    checked += 1;
                }
            }
            assert!(checked > 100,
                "seed={seed}: only checked {checked} tiles — sampling too sparse");
        }
    }

    #[test]
    fn nearest_hotspot_cached_matches_brute_force() {
        let seed = 42u64;
        let mut cache = HotspotChunkCache::new(seed);
        let mut checked = 0;

        // Sample tiles across a wide area, only check where density >= threshold
        for q in (-10000..10000).step_by(500) {
            for r in (-10000..10000).step_by(500) {
                let (wx, wy) = crate::hex_to_world(q, r);
                let density = material_density_cart(wx, wy, seed);
                if density < HOTSPOT_THRESHOLD {
                    continue;
                }

                let brute = nearest_hotspot(wx, wy, seed);
                let cached = nearest_hotspot_cached(wx, wy, &mut cache);

                match (brute, cached) {
                    (Some((_, _, bq, br, bdist)), Some((_, _, cq, cr, cdist))) => {
                        assert_eq!((bq, br), (cq, cr),
                            "Cell mismatch at tile ({}, {}): brute=({}, {}), cached=({}, {})",
                            q, r, bq, br, cq, cr);
                        assert!((bdist - cdist).abs() < 1e-6,
                            "Distance mismatch at tile ({}, {}): brute={}, cached={}",
                            q, r, bdist, cdist);
                    }
                    (None, None) => {}
                    (b, c) => panic!(
                        "Presence mismatch at tile ({}, {}): brute={:?}, cached={:?}",
                        q, r, b.is_some(), c.is_some()),
                }
                checked += 1;
            }
        }
        assert!(checked > 0, "Should have checked at least some dense tiles");
    }
}
