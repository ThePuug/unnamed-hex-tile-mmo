use std::collections::HashMap;

use crate::noise::{hash_u64, hash_f64, simplex_2d};
use crate::plates::{PlateCenter, PlateCache, macro_plate_neighbors, warped_plate_at};
use crate::{MACRO_CELL_SIZE, MICRO_CELL_SIZE, MICRO_SUPPRESSION_RATE,
            MICRO_JITTER_WAVELENGTH, MICRO_JITTER_MIN, MICRO_JITTER_MAX,
            WARP_STRENGTH_MAX, MAX_ELONGATION};

/// Row height factor for hex grid: sqrt(3)/2.
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

/// A microplate center with position, identity, sub-grid cell, and macro assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct MicroplateCenter {
    pub wx: f64,
    pub wy: f64,
    pub id: u64,
    /// Macro plate this micro cell is assigned to (via warped distance).
    /// Zero if not yet assigned (raw micro_cell_at result).
    pub parent_id: u64,
    pub sub_cell_q: i32,
    pub sub_cell_r: i32,
}

// ──── Micro-grid seeds (distinct from macro seeds) ────

const MICRO_SUPPRESS_SEED: u64 = 0xBBBB_CAFE_0001;
const MICRO_JITTER_SEED: u64 = 0xBBBB_CAFE_0002;
const MICRO_OFFSET_X_SEED: u64 = 0xBBBB_CAFE_0003;
const MICRO_OFFSET_Y_SEED: u64 = 0xBBBB_CAFE_0004;
const MICRO_ID_SEED: u64 = 0xBBBB_CAFE_0005;

// ──── Micro-grid cell → microplate center ────

/// Jitter factor at a world position for the micro grid.
fn micro_jitter_at(wx: f64, wy: f64, seed: u64) -> f64 {
    let n = simplex_2d(
        wx / MICRO_JITTER_WAVELENGTH,
        wy / MICRO_JITTER_WAVELENGTH,
        seed ^ MICRO_JITTER_SEED,
    );
    let t = (n + 1.0) * 0.5;
    MICRO_JITTER_MIN + t * (MICRO_JITTER_MAX - MICRO_JITTER_MIN)
}

/// Whether a micro cell is suppressed (produces no microplate center).
fn micro_cell_is_suppressed(cq: i32, cr: i32, seed: u64) -> bool {
    hash_f64(cq as i64, cr as i64, seed ^ MICRO_SUPPRESS_SEED) < MICRO_SUPPRESSION_RATE
}

/// Compute the microplate center for a specific micro-grid cell (odd-r offset hex).
/// Returns None if the cell is suppressed.
fn micro_center_for_cell(cq: i32, cr: i32, seed: u64) -> Option<(f64, f64, u64)> {
    if micro_cell_is_suppressed(cq, cr, seed) {
        return None;
    }

    let odd_shift = if cr & 1 != 0 { MICRO_CELL_SIZE * 0.5 } else { 0.0 };
    let nominal_wx = cq as f64 * MICRO_CELL_SIZE + odd_shift;
    let nominal_wy = cr as f64 * MICRO_CELL_SIZE * HEX_ROW_HEIGHT;

    let jitter = micro_jitter_at(nominal_wx, nominal_wy, seed);

    let offset_x = hash_f64(cq as i64, cr as i64, seed ^ MICRO_OFFSET_X_SEED) - 0.5;
    let offset_y = hash_f64(cq as i64, cr as i64, seed ^ MICRO_OFFSET_Y_SEED) - 0.5;

    let wx = nominal_wx + offset_x * jitter * MICRO_CELL_SIZE;
    let wy = nominal_wy + offset_y * jitter * MICRO_CELL_SIZE;

    let id = hash_u64(cq as i64, cr as i64, seed ^ MICRO_ID_SEED);

    Some((wx, wy, id))
}

/// Which micro-grid cell contains a world position.
fn micro_world_to_cell(wx: f64, wy: f64) -> (i32, i32) {
    let row_height = MICRO_CELL_SIZE * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { MICRO_CELL_SIZE * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / MICRO_CELL_SIZE).round() as i32;
    (cq, cr)
}


fn dist_sq(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

// ──── Public API (bottom-up flow) ────

/// Find the nearest micro cell to a world position using euclidean distance.
/// Micro cells are equidimensional everywhere — elongation lives in the
/// macro plate assignment layer (`warped_plate_at`), not here.
pub fn micro_cell_at(wx: f64, wy: f64, seed: u64) -> MicroplateCenter {
    let (cq, cr) = micro_world_to_cell(wx, wy);

    let mut best: Option<MicroplateCenter> = None;
    let mut best_dist = f64::MAX;

    for dr in -2..=2 {
        for dq in -2..=2 {
            let ncq = cq + dq;
            let ncr = cr + dr;
            if let Some((mx, my, mid)) = micro_center_for_cell(ncq, ncr, seed) {
                let d = dist_sq(wx, wy, mx, my);
                if d < best_dist {
                    best = Some(MicroplateCenter {
                        wx: mx,
                        wy: my,
                        id: mid,
                        parent_id: 0,
                        sub_cell_q: ncq,
                        sub_cell_r: ncr,
                    });
                    best_dist = d;
                }
            }
        }
    }

    best.expect("no micro cell found in 2-ring neighborhood — micro suppression rate too high")
}

/// Which macro plate does this micro cell belong to?
/// Uses warped distance from the micro cell's position to nearby macro seeds.
pub fn macro_plate_for(micro: &MicroplateCenter, seed: u64) -> PlateCenter {
    warped_plate_at(micro.wx, micro.wy, seed)
}

/// Returns both the micro cell and its macro plate assignment at a world position.
pub fn plate_info_at(wx: f64, wy: f64, seed: u64) -> (PlateCenter, MicroplateCenter) {
    let mut micro = micro_cell_at(wx, wy, seed);
    let macro_plate = macro_plate_for(&micro, seed);
    micro.parent_id = macro_plate.id;
    (macro_plate, micro)
}

/// Returns all micro cells assigned to the given macro seed.
/// Scans a wide radius because anisotropic warped distance can pull micro cells
/// from far along coastlines (up to MAX_ELONGATION× further than isotropic).
pub fn micro_cells_for_macro(macro_seed: &PlateCenter, seed: u64) -> Vec<MicroplateCenter> {
    let mut plate_cache = PlateCache::new(seed);
    generate_micro_cells_for_macro(macro_seed, seed, &mut plate_cache)
}

/// Internal: generate micro cells for a macro seed using a shared PlateCache.
fn generate_micro_cells_for_macro(
    macro_seed: &PlateCenter,
    seed: u64,
    plate_cache: &mut PlateCache,
) -> Vec<MicroplateCenter> {
    let search_radius = (MACRO_CELL_SIZE + WARP_STRENGTH_MAX) * MAX_ELONGATION;
    let cell_reach = (search_radius / MICRO_CELL_SIZE) as i32 + 3;

    let (center_cq, center_cr) = micro_world_to_cell(macro_seed.wx, macro_seed.wy);

    let mut children = Vec::new();

    for dr in -cell_reach..=cell_reach {
        for dq in -cell_reach..=cell_reach {
            let cq = center_cq + dq;
            let cr = center_cr + dr;

            if let Some((wx, wy, id)) = micro_center_for_cell(cq, cr, seed) {
                let owner = plate_cache.warped_plate_at(wx, wy);
                if owner.id == macro_seed.id {
                    children.push(MicroplateCenter {
                        wx,
                        wy,
                        id,
                        parent_id: macro_seed.id,
                        sub_cell_q: cq,
                        sub_cell_r: cr,
                    });
                }
            }
        }
    }

    children
}

/// Returns microplate neighbors (both intra-plate and cross-boundary).
pub fn microplate_neighbors(wx: f64, wy: f64, seed: u64) -> Vec<MicroplateCenter> {
    let (owner_macro, owner_micro) = plate_info_at(wx, wy, seed);
    let children = micro_cells_for_macro(&owner_macro, seed);

    let mut neighbors = Vec::new();

    // Intra-plate neighbors via midpoint sampling
    for child in &children {
        if child.id == owner_micro.id { continue; }

        let mid_x = (owner_micro.wx + child.wx) * 0.5;
        let mid_y = (owner_micro.wy + child.wy) * 0.5;

        let mid_micro = micro_cell_at(mid_x, mid_y, seed);
        if mid_micro.id == owner_micro.id || mid_micro.id == child.id {
            neighbors.push(child.clone());
        }
    }

    // Cross-boundary neighbors
    let macro_neighbors = macro_plate_neighbors(owner_macro.wx, owner_macro.wy, seed);
    for neighbor_macro in &macro_neighbors {
        let neighbor_children = micro_cells_for_macro(neighbor_macro, seed);
        for neighbor_child in &neighbor_children {
            let mid_x = (owner_micro.wx + neighbor_child.wx) * 0.5;
            let mid_y = (owner_micro.wy + neighbor_child.wy) * 0.5;

            let mid_micro = micro_cell_at(mid_x, mid_y, seed);
            if mid_micro.id == owner_micro.id || mid_micro.id == neighbor_child.id {
                neighbors.push(neighbor_child.clone());
            }
        }
    }

    neighbors
}

// ──── Cached API ────

/// Lazy cache for the bottom-up micro → macro lookup flow.
/// Caches micro sub-grid cells and macro assignments independently.
pub struct MicroplateCache {
    /// Micro sub-grid cell cache: (cq, cr) → Option<(wx, wy, id)>
    micro_cells: HashMap<(i32, i32), Option<(f64, f64, u64)>>,
    /// Macro assignment cache: micro_id → PlateCenter
    macro_assignments: HashMap<u64, PlateCenter>,
    /// Plate cache for warped macro lookups
    pub plate_cache: PlateCache,
    seed: u64,
}

impl MicroplateCache {
    pub fn new(seed: u64) -> Self {
        Self {
            micro_cells: HashMap::new(),
            macro_assignments: HashMap::new(),
            plate_cache: PlateCache::new(seed),
            seed,
        }
    }

    /// Ensure a micro sub-grid cell is cached.
    fn ensure_micro_cell(&mut self, cq: i32, cr: i32) {
        let seed = self.seed;
        self.micro_cells.entry((cq, cr))
            .or_insert_with(|| micro_center_for_cell(cq, cr, seed));
    }

    /// Cached micro cell lookup with euclidean distance.
    pub fn micro_cell_at(&mut self, wx: f64, wy: f64) -> MicroplateCenter {
        let (cq, cr) = micro_world_to_cell(wx, wy);

        for dr in -2..=2 {
            for dq in -2..=2 {
                self.ensure_micro_cell(cq + dq, cr + dr);
            }
        }

        let mut best: Option<MicroplateCenter> = None;
        let mut best_dist = f64::MAX;

        for dr in -2..=2 {
            for dq in -2..=2 {
                if let Some((mx, my, mid)) = self.micro_cells[&(cq + dq, cr + dr)] {
                    let d = dist_sq(wx, wy, mx, my);
                    if d < best_dist {
                        best = Some(MicroplateCenter {
                            wx: mx,
                            wy: my,
                            id: mid,
                            parent_id: 0,
                            sub_cell_q: cq + dq,
                            sub_cell_r: cr + dr,
                        });
                        best_dist = d;
                    }
                }
            }
        }

        best.expect("no micro cell found in 2-ring neighborhood")
    }

    /// Cached plate_info_at: micro cell lookup + macro assignment.
    pub fn plate_info_at(&mut self, wx: f64, wy: f64) -> (PlateCenter, MicroplateCenter) {
        let mut micro = self.micro_cell_at(wx, wy);

        let macro_plate = if let Some(cached) = self.macro_assignments.get(&micro.id) {
            cached.clone()
        } else {
            let assigned = self.plate_cache.warped_plate_at(micro.wx, micro.wy);
            self.macro_assignments.insert(micro.id, assigned.clone());
            assigned
        };

        micro.parent_id = macro_plate.id;
        (macro_plate, micro)
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plates::macro_plate_at;
    use std::collections::HashSet;

    #[test]
    fn micro_cell_at_returns_result_everywhere() {
        let seed = 42u64;
        for x in (-10000..10000).step_by(500) {
            for y in (-10000..10000).step_by(500) {
                // Should never panic — micro grid has no macro dependency
                let _micro = micro_cell_at(x as f64, y as f64, seed);
            }
        }
    }

    #[test]
    fn micro_cell_at_returns_nearest() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(1500) {
            for y in (-5000..5000).step_by(1500) {
                let wx = x as f64;
                let wy = y as f64;
                let result = micro_cell_at(wx, wy, seed);
                let result_dist = dist_sq(wx, wy, result.wx, result.wy);

                // Brute force check over wider area
                let (cq, cr) = micro_world_to_cell(wx, wy);
                for dq in -4..=4 {
                    for dr in -4..=4 {
                        if let Some((mx, my, _)) = micro_center_for_cell(cq + dq, cr + dr, seed) {
                            let d = dist_sq(wx, wy, mx, my);
                            assert!(result_dist <= d + 1e-6,
                                "micro_cell_at({wx}, {wy}) not nearest: result dist²={result_dist}, found dist²={d}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn macro_plate_for_is_deterministic() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(2000) {
            for y in (-5000..5000).step_by(2000) {
                let micro = micro_cell_at(x as f64, y as f64, seed);
                let a = macro_plate_for(&micro, seed);
                let b = macro_plate_for(&micro, seed);
                assert_eq!(a.id, b.id);
            }
        }
    }

    #[test]
    fn plate_info_at_is_consistent() {
        let seed = 42u64;
        for x in (-8000..8000).step_by(500) {
            for y in (-8000..8000).step_by(500) {
                let wx = x as f64;
                let wy = y as f64;
                let (macro_plate, micro) = plate_info_at(wx, wy, seed);
                assert_eq!(micro.parent_id, macro_plate.id,
                    "plate_info_at({wx}, {wy}): micro.parent_id={} != macro.id={}",
                    micro.parent_id, macro_plate.id);

                // macro_plate_for should agree
                let raw_micro = micro_cell_at(wx, wy, seed);
                let assigned = macro_plate_for(&raw_micro, seed);
                assert_eq!(assigned.id, macro_plate.id,
                    "plate_info_at vs macro_plate_for disagree at ({wx}, {wy})");
            }
        }
    }

    #[test]
    fn every_macro_has_micro_cells() {
        let seed = 42u64;
        for x in (-10000..10000).step_by(3000) {
            for y in (-10000..10000).step_by(3000) {
                let plate = macro_plate_at(x as f64, y as f64, seed);
                let children = micro_cells_for_macro(&plate, seed);
                assert!(!children.is_empty(),
                    "Macro plate at ({x}, {y}) id={} has no micro cells", plate.id);
            }
        }
    }

    #[test]
    fn micro_cells_for_macro_matches_individual() {
        let seed = 42u64;
        // Pick a macro plate and verify its micro cells individually report back to it
        let plate = macro_plate_at(0.0, 0.0, seed);
        let children = micro_cells_for_macro(&plate, seed);

        for child in &children {
            let assigned = macro_plate_for(child, seed);
            assert_eq!(assigned.id, plate.id,
                "Micro cell id={} at ({:.0}, {:.0}) claims plate {} but macro_plate_for says {}",
                child.id, child.wx, child.wy, plate.id, assigned.id);
        }
    }

    #[test]
    fn micro_cell_ids_unique_within_macro() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(3000) {
            for y in (-5000..5000).step_by(3000) {
                let plate = macro_plate_at(x as f64, y as f64, seed);
                let children = micro_cells_for_macro(&plate, seed);
                let ids: HashSet<u64> = children.iter().map(|c| c.id).collect();
                assert_eq!(ids.len(), children.len(),
                    "Duplicate micro IDs within plate at ({x}, {y})");
            }
        }
    }

    #[test]
    fn micro_suppression_rate_is_reasonable() {
        let seed = 42u64;
        let mut suppressed = 0;
        let mut total = 0;
        for cq in -40..40 {
            for cr in -40..40 {
                total += 1;
                if micro_cell_is_suppressed(cq, cr, seed) {
                    suppressed += 1;
                }
            }
        }
        let rate = suppressed as f64 / total as f64;
        assert!(rate > 0.10 && rate < 0.35,
            "Micro suppression rate {rate:.3} ({suppressed}/{total}) should be near {MICRO_SUPPRESSION_RATE}");
    }

    #[test]
    fn cached_matches_uncached() {
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        for x in (-5000..5000).step_by(1000) {
            for y in (-5000..5000).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                let (uncached_macro, uncached_micro) = plate_info_at(wx, wy, seed);
                let (cached_macro, cached_micro) = cache.plate_info_at(wx, wy);
                assert_eq!(uncached_micro.id, cached_micro.id,
                    "Micro mismatch at ({wx}, {wy})");
                assert_eq!(uncached_macro.id, cached_macro.id,
                    "Macro mismatch at ({wx}, {wy})");
            }
        }
    }

    #[test]
    fn cross_boundary_neighbors_exist() {
        let seed = 42u64;
        let (plate, _micro) = plate_info_at(0.0, 0.0, seed);
        let macro_nbrs = macro_plate_neighbors(plate.wx, plate.wy, seed);

        if let Some(nbr) = macro_nbrs.first() {
            let boundary_x = (plate.wx + nbr.wx) * 0.5;
            let boundary_y = (plate.wy + nbr.wy) * 0.5;

            let dx = plate.wx - boundary_x;
            let dy = plate.wy - boundary_y;
            let len = (dx * dx + dy * dy).sqrt();
            let near_x = boundary_x + dx / len * 50.0;
            let near_y = boundary_y + dy / len * 50.0;

            let neighbors = microplate_neighbors(near_x, near_y, seed);
            assert!(!neighbors.is_empty(), "Should have at least some neighbors near boundary");
        }
    }

}
