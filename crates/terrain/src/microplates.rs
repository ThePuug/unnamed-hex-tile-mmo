use std::collections::{HashMap, HashSet};

use crate::noise::{hash_u64, hash_f64, simplex_2d};
use crate::plates::{PlateCenter, PlateCache};
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

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `PlateCache::warped_plate_at(micro.wx, micro.wy)` with a shared cache.
pub fn macro_plate_for(micro: &MicroplateCenter, seed: u64) -> PlateCenter {
    PlateCache::new(seed).warped_plate_at(micro.wx, micro.wy)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `MicroplateCache::plate_info_at` for repeated lookups.
pub fn plate_info_at(wx: f64, wy: f64, seed: u64) -> (PlateCenter, MicroplateCenter) {
    let mut cache = PlateCache::new(seed);
    let mut micro = micro_cell_at(wx, wy, seed);
    let macro_plate = cache.warped_plate_at(micro.wx, micro.wy);
    micro.parent_id = macro_plate.id;
    (macro_plate, micro)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `generate_micro_cells_for_macro` with a shared `&mut PlateCache`.
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

/// UNCACHED — creates throwaway PlateCache per call.
/// All macro lookups within this function share a single PlateCache.
pub fn microplate_neighbors(wx: f64, wy: f64, seed: u64) -> Vec<MicroplateCenter> {
    let mut plate_cache = PlateCache::new(seed);

    let mut owner_micro = micro_cell_at(wx, wy, seed);
    let owner_macro = plate_cache.warped_plate_at(owner_micro.wx, owner_micro.wy);
    owner_micro.parent_id = owner_macro.id;

    let children = generate_micro_cells_for_macro(&owner_macro, seed, &mut plate_cache);

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
    let macro_neighbors = plate_cache.plate_neighbors(owner_macro.wx, owner_macro.wy);
    for neighbor_macro in &macro_neighbors {
        let neighbor_children = generate_micro_cells_for_macro(neighbor_macro, seed, &mut plate_cache);
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

    /// Look up the cached macro plate assignment for a micro cell by its ID.
    pub fn macro_assignment(&self, micro_id: u64) -> Option<&PlateCenter> {
        self.macro_assignments.get(&micro_id)
    }

    /// Extract all corrected micro→macro assignments as micro_id → macro_plate_id.
    /// Call after `fix_orphans` to get the corrected mapping for read-only sharing.
    pub fn all_macro_ids(&self) -> HashMap<u64, u64> {
        self.macro_assignments.iter()
            .map(|(&mid, plate)| (mid, plate.id))
            .collect()
    }

    /// Fix orphaned macro plate assignments in the cache.
    ///
    /// Connected component analysis: for each macro plate, flood-fill its
    /// micro cells. Keep only the largest component (the main body); reassign
    /// all smaller fragments to the surrounding majority plate. Repeat until
    /// stable — reassignment can cascade when a fragment joins a plate that
    /// splits another plate's connectivity. Typically converges in 2-3 rounds.
    ///
    /// Final sweep: catches isolated cells of plates whose main body is
    /// entirely outside the cache (single-component, no larger body to compare).
    ///
    /// Call after batch-populating a region (e.g. after all `plate_info_at`
    /// calls for a viewport).
    ///
    /// Returns the number of cells corrected.
    pub fn fix_orphans(&mut self) -> usize {
        // Build reverse map: micro_id → (cq, cr, wx, wy)
        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = self.micro_cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        // Ensure all cached micro cells have macro assignments
        let unassigned: Vec<(u64, f64, f64)> = id_to_pos.iter()
            .filter(|(id, _)| !self.macro_assignments.contains_key(id))
            .map(|(&id, &(_, _, wx, wy))| (id, wx, wy))
            .collect();
        for (id, wx, wy) in unassigned {
            let plate = self.plate_cache.warped_plate_at(wx, wy);
            self.macro_assignments.insert(id, plate);
        }

        // Build neighbor map once (topology doesn't change, only assignments)
        let all_neighbors: HashMap<u64, Vec<u64>> = id_to_pos.iter()
            .filter_map(|(&id, &(cq, cr, _, _))| {
                if self.macro_assignments.contains_key(&id) {
                    Some((id, micro_neighbor_ids(cq, cr, id, &self.micro_cells)))
                } else {
                    None
                }
            })
            .collect();

        // ── CC loop: repeat until no fragments found ──

        let mut total_corrected = 0;
        const MAX_ROUNDS: usize = 10;

        for _ in 0..MAX_ROUNDS {
            let round_count = self.cc_round(&all_neighbors);
            if round_count == 0 { break; }
            total_corrected += round_count;
        }

        // ── Final sweep: single-cell orphans of plates with no visible body ──

        let mut sweep_corrections: Vec<(u64, PlateCenter)> = Vec::new();
        for (&id, nbrs) in &all_neighbors {
            if nbrs.is_empty() { continue; }
            let my_plate = match self.macro_assignments.get(&id) {
                Some(p) => p,
                None => continue,
            };
            let has_same = nbrs.iter().any(|nid| {
                self.macro_assignments.get(nid)
                    .map_or(false, |p| p.id == my_plate.id)
            });
            if has_same { continue; }

            let mut counts: HashMap<u64, (usize, &PlateCenter)> = HashMap::new();
            for nid in nbrs {
                if let Some(plate) = self.macro_assignments.get(nid) {
                    let entry = counts.entry(plate.id).or_insert((0, plate));
                    entry.0 += 1;
                }
            }
            if let Some((_, plate)) = counts.into_values()
                .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.id.cmp(&a.1.id)))
            {
                sweep_corrections.push((id, plate.clone()));
            }
        }

        let sweep_count = sweep_corrections.len();
        for (id, plate) in sweep_corrections {
            self.macro_assignments.insert(id, plate);
        }

        total_corrected + sweep_count
    }

    /// One round of connected component analysis + fragment reassignment.
    /// Returns the number of cells corrected (0 = stable).
    fn cc_round(&mut self, all_neighbors: &HashMap<u64, Vec<u64>>) -> usize {
        // Build same-plate adjacency from current assignments
        let mut same_plate_adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&id, nbrs) in all_neighbors {
            if let Some(my_plate) = self.macro_assignments.get(&id) {
                let my_plate_id = my_plate.id;
                let same: Vec<u64> = nbrs.iter()
                    .filter(|nid| {
                        self.macro_assignments.get(nid)
                            .map_or(false, |p| p.id == my_plate_id)
                    })
                    .copied()
                    .collect();
                same_plate_adj.insert(id, same);
            }
        }

        // BFS for connected components (deterministic via sorted IDs)
        let mut visited: HashSet<u64> = HashSet::new();
        let mut all_components: Vec<Vec<u64>> = Vec::new();
        let mut sorted_ids: Vec<u64> = same_plate_adj.keys().copied().collect();
        sorted_ids.sort_unstable();

        for id in sorted_ids {
            if visited.contains(&id) { continue; }
            let mut component = Vec::new();
            let mut queue = vec![id];
            while let Some(current) = queue.pop() {
                if !visited.insert(current) { continue; }
                component.push(current);
                if let Some(nbrs) = same_plate_adj.get(&current) {
                    for &nid in nbrs {
                        if !visited.contains(&nid) {
                            queue.push(nid);
                        }
                    }
                }
            }
            all_components.push(component);
        }

        // Group by plate, identify fragments
        let mut plate_components: HashMap<u64, Vec<Vec<u64>>> = HashMap::new();
        for component in all_components {
            if let Some(plate) = self.macro_assignments.get(&component[0]) {
                plate_components.entry(plate.id).or_default().push(component);
            }
        }

        let mut fragments: Vec<Vec<u64>> = Vec::new();
        for (_, mut components) in plate_components {
            if components.len() <= 1 { continue; }
            components.sort_by(|a, b| {
                b.len().cmp(&a.len())
                    .then_with(|| a.iter().min().cmp(&b.iter().min()))
            });
            fragments.extend(components.into_iter().skip(1));
        }

        if fragments.is_empty() { return 0; }

        // Compute corrections from current state, then batch-apply
        let mut corrections: Vec<(u64, PlateCenter)> = Vec::new();
        for fragment in &fragments {
            let frag_set: HashSet<u64> = fragment.iter().copied().collect();
            let mut surrounding: HashMap<u64, (usize, PlateCenter)> = HashMap::new();
            for &cid in fragment {
                if let Some(nbrs) = all_neighbors.get(&cid) {
                    for &nid in nbrs {
                        if frag_set.contains(&nid) { continue; }
                        if let Some(plate) = self.macro_assignments.get(&nid) {
                            let entry = surrounding.entry(plate.id)
                                .or_insert((0, plate.clone()));
                            entry.0 += 1;
                        }
                    }
                }
            }
            if let Some((_, new_plate)) = surrounding.into_values()
                .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.id.cmp(&a.1.id)))
            {
                for &cid in fragment {
                    corrections.push((cid, new_plate.clone()));
                }
            }
        }

        let count = corrections.len();
        for (id, plate) in corrections {
            self.macro_assignments.insert(id, plate);
        }
        count
    }

}

/// Hex neighbor offsets for odd-r offset grid (even rows).
const HEX_NEIGHBORS_EVEN: [(i32, i32); 6] = [(-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)];
/// Hex neighbor offsets for odd-r offset grid (odd rows).
const HEX_NEIGHBORS_ODD: [(i32, i32); 6] = [(-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)];

/// Find micro neighbor IDs via direct sub-grid hex coordinate offsets.
/// 6 lookups per cell — no spatial scanning, no distance math.
fn micro_neighbor_ids(
    cq: i32, cr: i32, id: u64,
    micro_cells: &HashMap<(i32, i32), Option<(f64, f64, u64)>>,
) -> Vec<u64> {
    let offsets = if cr & 1 == 0 { &HEX_NEIGHBORS_EVEN } else { &HEX_NEIGHBORS_ODD };
    let mut neighbors = Vec::new();
    for &(dq, dr) in offsets {
        if let Some(Some((_, _, nid))) = micro_cells.get(&(cq + dq, cr + dr)) {
            if *nid != id {
                neighbors.push(*nid);
            }
        }
    }
    neighbors
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plates::{macro_plate_at, macro_plate_neighbors};
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

    // ──── Orphan correction tests ────

    /// Populate a cache region by calling plate_info_at on a grid of sample points.
    fn populate_region(cache: &mut MicroplateCache, cx: f64, cy: f64, radius: f64, step: f64) {
        let mut x = cx - radius;
        while x <= cx + radius {
            let mut y = cy - radius;
            while y <= cy + radius {
                cache.plate_info_at(x, y);
                y += step;
            }
            x += step;
        }
    }

    #[test]
    fn fix_orphans_is_deterministic() {
        let seed = 42u64;
        let mut cache_a = MicroplateCache::new(seed);
        let mut cache_b = MicroplateCache::new(seed);

        populate_region(&mut cache_a, 0.0, 0.0, 5000.0, 100.0);
        populate_region(&mut cache_b, 0.0, 0.0, 5000.0, 100.0);

        let count_a = cache_a.fix_orphans();
        let count_b = cache_b.fix_orphans();

        assert_eq!(count_a, count_b, "fix_orphans correction count should be deterministic");

        // Verify identical assignments after correction
        for (&id, plate_a) in &cache_a.macro_assignments {
            let plate_b = cache_b.macro_assignments.get(&id)
                .expect("same micro IDs should exist in both caches");
            assert_eq!(plate_a.id, plate_b.id,
                "Macro assignment for micro {} differs after fix_orphans", id);
        }
    }

    #[test]
    fn fix_orphans_only_modifies_disconnected_cells() {
        // Every changed cell must have been either:
        // (a) in a non-largest component of its plate, OR
        // (b) a single-cell orphan (zero same-plate neighbors)
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        populate_region(&mut cache, 0.0, 0.0, 5000.0, 100.0);

        let before: HashMap<u64, u64> = cache.macro_assignments.iter()
            .map(|(&mid, plate)| (mid, plate.id))
            .collect();

        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = cache.micro_cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        // Build same-plate adjacency from BEFORE snapshot
        let mut same_plate_adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&id, &(cq, cr, _, _)) in &id_to_pos {
            if let Some(&my_pid) = before.get(&id) {
                let nbrs = micro_neighbor_ids(cq, cr, id, &cache.micro_cells);
                let same: Vec<u64> = nbrs.into_iter()
                    .filter(|nid| before.get(nid).map_or(false, |&pid| pid == my_pid))
                    .collect();
                same_plate_adj.insert(id, same);
            }
        }

        // BFS components, track largest per plate (matching code's tie-break)
        let mut cell_is_fragment: HashSet<u64> = HashSet::new();
        let mut plate_components: HashMap<u64, Vec<Vec<u64>>> = HashMap::new();
        let mut visited: HashSet<u64> = HashSet::new();
        let mut sorted: Vec<u64> = same_plate_adj.keys().copied().collect();
        sorted.sort_unstable();
        for id in sorted {
            if visited.contains(&id) { continue; }
            let mut comp = Vec::new();
            let mut queue = vec![id];
            while let Some(cur) = queue.pop() {
                if !visited.insert(cur) { continue; }
                comp.push(cur);
                if let Some(nbrs) = same_plate_adj.get(&cur) {
                    for &nid in nbrs {
                        if !visited.contains(&nid) { queue.push(nid); }
                    }
                }
            }
            let pid = before[&comp[0]];
            plate_components.entry(pid).or_default().push(comp);
        }
        for (_, mut components) in plate_components {
            if components.len() <= 1 { continue; }
            components.sort_by(|a, b| {
                b.len().cmp(&a.len())
                    .then_with(|| a.iter().min().cmp(&b.iter().min()))
            });
            for comp in components.into_iter().skip(1) {
                for cid in comp { cell_is_fragment.insert(cid); }
            }
        }
        // Also mark single-cell orphans
        for (&id, same) in &same_plate_adj {
            if same.is_empty() {
                cell_is_fragment.insert(id);
            }
        }

        cache.fix_orphans();

        let mut changed = 0;
        for (&mid, plate) in &cache.macro_assignments {
            if let Some(&old_pid) = before.get(&mid) {
                if plate.id != old_pid {
                    changed += 1;
                    assert!(cell_is_fragment.contains(&mid),
                        "Cell {} was changed but was not a fragment or single-cell orphan", mid);
                }
            }
        }
        assert!(changed > 0, "Expected at least some corrections in a 10k×10k region");
    }

    #[test]
    fn no_interior_fragments_remain_after_fix() {
        // After fix_orphans, check for orphan components that are fully
        // contained in the interior (not touching the boundary where they
        // might connect to a main body outside the cache).
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        populate_region(&mut cache, 0.0, 0.0, 5000.0, 100.0);
        cache.fix_orphans();

        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = cache.micro_cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        let inner_radius = 4000.0;
        let border_radius = 4500.0; // cells between inner and border may connect outside

        let assigned_ids: HashSet<u64> = id_to_pos.keys()
            .filter(|id| cache.macro_assignments.contains_key(id))
            .copied()
            .collect();

        // Build same-plate adjacency over all assigned cells
        let mut same_plate_adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for &id in &assigned_ids {
            let &(cq, cr, _, _) = &id_to_pos[&id];
            let my_pid = cache.macro_assignments[&id].id;
            let nbrs = micro_neighbor_ids(cq, cr, id, &cache.micro_cells);
            let same: Vec<u64> = nbrs.into_iter()
                .filter(|nid| {
                    cache.macro_assignments.get(nid)
                        .map_or(false, |p| p.id == my_pid)
                })
                .collect();
            same_plate_adj.insert(id, same);
        }

        // BFS to find components, check if any are fully interior
        let mut visited: HashSet<u64> = HashSet::new();
        let mut interior_orphans = 0;
        let mut sorted: Vec<u64> = assigned_ids.iter().copied().collect();
        sorted.sort_unstable();

        for id in sorted {
            if visited.contains(&id) { continue; }
            let pid = cache.macro_assignments[&id].id;
            let mut comp = Vec::new();
            let mut queue = vec![id];
            while let Some(cur) = queue.pop() {
                if !visited.insert(cur) { continue; }
                comp.push(cur);
                if let Some(nbrs) = same_plate_adj.get(&cur) {
                    for &nid in nbrs {
                        if !visited.contains(&nid) { queue.push(nid); }
                    }
                }
            }

            // Skip the largest component per plate (only check fragments)
            // A component touching the border might connect to the main body outside
            let touches_border = comp.iter().any(|&cid| {
                let &(_, _, wx, wy) = &id_to_pos[&cid];
                wx.abs() > border_radius || wy.abs() > border_radius
            });
            let fully_interior = comp.iter().all(|&cid| {
                let &(_, _, wx, wy) = &id_to_pos[&cid];
                wx.abs() <= inner_radius && wy.abs() <= inner_radius
            });

            // Count how many same-plate components exist
            let same_plate_count = assigned_ids.iter()
                .filter(|&&aid| cache.macro_assignments[&aid].id == pid)
                .count();

            // A fully-interior component that isn't the whole plate is suspect
            if fully_interior && !touches_border && comp.len() < same_plate_count {
                interior_orphans += 1;
            }
        }

        assert_eq!(interior_orphans, 0,
            "Found {interior_orphans} fully-interior orphan components after fix_orphans");
    }

    #[test]
    fn single_cell_orphan_reassigned_to_a_neighbor() {
        // Phase 2 (single-cell orphan sweep): a cell with a bogus plate ID
        // that no neighbor shares should be reassigned to some neighbor's plate.
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        populate_region(&mut cache, 0.0, 0.0, 2000.0, 100.0);

        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = cache.micro_cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        // Find a cell with assigned neighbors
        let mut target = None;
        for (&id, &(cq, cr, wx, wy)) in &id_to_pos {
            if wx.abs() > 1500.0 || wy.abs() > 1500.0 { continue; }
            if cache.macro_assignments.get(&id).is_none() { continue; }
            let nids = micro_neighbor_ids(cq, cr, id, &cache.micro_cells);
            if nids.len() >= 3 && nids.iter().all(|n| cache.macro_assignments.contains_key(n)) {
                target = Some((id, nids));
                break;
            }
        }

        let (target_id, neighbor_ids) = target.expect("Should find a suitable cell");

        // Force to a bogus plate — creates a single-cell orphan (no main body)
        let bogus = PlateCenter { wx: 0.0, wy: 0.0, cell_q: 0, cell_r: 0, id: 0xDEAD };
        cache.macro_assignments.insert(target_id, bogus);

        cache.fix_orphans();

        let corrected = &cache.macro_assignments[&target_id];
        assert_ne!(corrected.id, 0xDEAD,
            "Single-cell orphan should have been reassigned away from bogus plate");
        // The corrected plate should be one of the (possibly updated) neighbor plates
        let neighbor_plates: HashSet<u64> = neighbor_ids.iter()
            .filter_map(|nid| cache.macro_assignments.get(nid).map(|p| p.id))
            .collect();
        assert!(neighbor_plates.contains(&corrected.id),
            "Corrected plate {} should be one of the neighbor plates {:?}",
            corrected.id, neighbor_plates);
    }

    #[test]
    fn multi_cell_fragment_reassigned_to_surrounding() {
        // Phase 1 (CC analysis): force two adjacent cells to a plate that has
        // a large main body elsewhere. The 2-cell splinter should be reassigned.
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        populate_region(&mut cache, 0.0, 0.0, 3000.0, 100.0);

        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = cache.micro_cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        // Find two adjacent cells that share the same plate
        let mut pair = None;
        for (&id_a, &(cq_a, cr_a, wx_a, wy_a)) in &id_to_pos {
            if wx_a.abs() > 2000.0 || wy_a.abs() > 2000.0 { continue; }
            let plate_a = match cache.macro_assignments.get(&id_a) {
                Some(p) => p.id,
                None => continue,
            };
            let nbrs = micro_neighbor_ids(cq_a, cr_a, id_a, &cache.micro_cells);
            for &nid in &nbrs {
                if cache.macro_assignments.get(&nid).map_or(true, |p| p.id != plate_a) {
                    continue;
                }
                pair = Some((id_a, nid, plate_a));
                break;
            }
            if pair.is_some() { break; }
        }

        let (cell_a, cell_b, original_plate) = pair.expect("Should find adjacent same-plate pair");

        // Find a different plate that has a large body in the cache
        let donor_plate = cache.macro_assignments.values()
            .find(|p| p.id != original_plate)
            .cloned()
            .expect("Should find a different plate");

        // Force both cells to the donor plate → creates a 2-cell fragment
        cache.macro_assignments.insert(cell_a, donor_plate.clone());
        cache.macro_assignments.insert(cell_b, donor_plate.clone());

        cache.fix_orphans();

        // Both should have been reassigned away from the donor
        let after_a = cache.macro_assignments[&cell_a].id;
        let after_b = cache.macro_assignments[&cell_b].id;
        assert_ne!(after_a, donor_plate.id,
            "Cell A should have been reassigned from fragment of plate {}", donor_plate.id);
        assert_ne!(after_b, donor_plate.id,
            "Cell B should have been reassigned from fragment of plate {}", donor_plate.id);
    }

    #[test]
    fn orphan_rate_is_small() {
        // The total number of orphans should be a small fraction of total micro cells.
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        populate_region(&mut cache, 0.0, 0.0, 8000.0, 100.0);

        let total_assigned = cache.macro_assignments.len();
        let corrected = cache.fix_orphans();

        let rate = corrected as f64 / total_assigned as f64;
        assert!(rate < 0.05,
            "Orphan rate {rate:.4} ({corrected}/{total_assigned}) should be < 5%");
    }

}
