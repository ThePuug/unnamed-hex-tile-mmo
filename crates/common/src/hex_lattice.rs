//! Hex ball tessellation lattice at arbitrary radius.
//!
//! Hex balls of radius R tile the plane on the lattice with basis:
//!   v1 = (R+1, R),  v2 = (-R, 2R+1)
//! The determinant equals the hex ball tile count (3R²+3R+1), guaranteeing
//! exactly one tile per cell with no gaps or overlaps.
//!
//! This generalizes the game chunk system (radius 9, v1=(10,9), v2=(-9,19))
//! to arbitrary radii for the event framework.

/// Hex ball tessellation lattice for a given radius.
///
/// Provides O(1) cell ID from tile coordinates, tile enumeration within cells,
/// and neighbor/overlap computation.
#[derive(Clone)]
pub struct HexLattice {
    pub radius: u32,
    pub v1: (i32, i32),
    pub v2: (i32, i32),
    det: i32,
    // Inverse matrix numerator coefficients (divide by det)
    inv_a: i32,
    inv_b: i32,
    inv_c: i32,
    inv_d: i32,
}

impl HexLattice {
    pub fn new(radius: u32) -> Self {
        let r = radius as i32;
        let v1 = (r + 1, r);
        let v2 = (-r, 2 * r + 1);
        let det = v1.0 * v2.1 - v1.1 * v2.0;
        debug_assert_eq!(det, 3 * r * r + 3 * r + 1);

        Self {
            radius,
            v1,
            v2,
            det,
            inv_a: 2 * r + 1,
            inv_b: r,
            inv_c: -r,
            inv_d: r + 1,
        }
    }

    /// Number of tiles per cell: 3R² + 3R + 1.
    pub fn tiles_per_cell(&self) -> u32 {
        self.det as u32
    }

    /// Cell ID for a tile at (q, r). O(1) via inverse lattice transform.
    pub fn cell_id(&self, q: i32, r: i32) -> (i32, i32) {
        let det = self.det as f64;
        let nf = (self.inv_a as f64 * q as f64 + self.inv_b as f64 * r as f64) / det;
        let mf = (self.inv_c as f64 * q as f64 + self.inv_d as f64 * r as f64) / det;

        let n0 = nf.floor() as i32;
        let m0 = mf.floor() as i32;
        let mut best = (n0, m0);
        let mut best_dist = i32::MAX;

        for dn in 0..=1 {
            for dm in 0..=1 {
                let n = n0 + dn;
                let m = m0 + dm;
                let cq = n * self.v1.0 + m * self.v2.0;
                let cr = n * self.v1.1 + m * self.v2.1;
                let dq = q - cq;
                let dr = r - cr;
                let dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
                if dist < best_dist || (dist == best_dist && (n, m) < best) {
                    best_dist = dist;
                    best = (n, m);
                }
            }
        }
        best
    }

    /// Center tile of a cell in (q, r) coordinates.
    pub fn cell_center(&self, id: (i32, i32)) -> (i32, i32) {
        (
            id.0 * self.v1.0 + id.1 * self.v2.0,
            id.0 * self.v1.1 + id.1 * self.v2.1,
        )
    }

    /// Iterate all tiles in a cell (hex ball of self.radius centered on cell center).
    pub fn tiles_in_cell(&self, id: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
        let (cq, cr) = self.cell_center(id);
        let r = self.radius as i32;
        (-r..=r).flat_map(move |dq| {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            (dr_min..=dr_max).map(move |dr| (cq + dq, cr + dr))
        })
    }

    /// Iterate all tiles in a hex ball of given radius centered on a cell.
    pub fn tiles_in_radius(&self, id: (i32, i32), radius: u32) -> impl Iterator<Item = (i32, i32)> {
        let (cq, cr) = self.cell_center(id);
        let r = radius as i32;
        (-r..=r).flat_map(move |dq| {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            (dr_min..=dr_max).map(move |dr| (cq + dq, cr + dr))
        })
    }

    /// The 6 neighboring cell IDs in lattice coordinates.
    pub fn neighbor_cells(&self, id: (i32, i32)) -> [(i32, i32); 6] {
        [
            (id.0 + 1, id.1),
            (id.0 - 1, id.1),
            (id.0, id.1 + 1),
            (id.0, id.1 - 1),
            (id.0 + 1, id.1 - 1),
            (id.0 - 1, id.1 + 1),
        ]
    }

    /// All cell IDs within hex distance `dist` in lattice space (hex ball of cells).
    pub fn cells_within_distance(&self, id: (i32, i32), dist: u32) -> Vec<(i32, i32)> {
        let r = dist as i32;
        let mut result = Vec::new();
        for dn in -r..=r {
            let dm_min = (-r).max(-dn - r);
            let dm_max = r.min(-dn + r);
            for dm in dm_min..=dm_max {
                result.push((id.0 + dn, id.1 + dm));
            }
        }
        result
    }

    /// Find all cells of another lattice whose hex balls overlap a hex ball
    /// of `query_radius` tiles centered on `center_q, center_r`.
    pub fn cells_overlapping_ball(
        &self,
        center_q: i32, center_r: i32,
        query_radius: u32,
    ) -> Vec<(i32, i32)> {
        // A cell overlaps if its center is within (self.radius + query_radius) hex distance
        let reach = self.radius + query_radius;
        let r = reach as i32;
        let mut result = Vec::new();
        for dq in -r..=r {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            for dr in dr_min..=dr_max {
                let candidate = self.cell_id(center_q + dq, center_r + dr);
                if !result.contains(&candidate) {
                    result.push(candidate);
                }
            }
        }
        // Deduplicate: the cell_id calls can produce duplicates since many tiles
        // map to the same cell. Use a simpler approach: search in lattice space.
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn determinant_equals_tile_count() {
        for radius in [1, 2, 5, 9, 64, 128] {
            let lat = HexLattice::new(radius);
            let expected = 3 * radius * radius + 3 * radius + 1;
            assert_eq!(lat.tiles_per_cell(), expected,
                "radius {radius}: det={} expected={expected}", lat.tiles_per_cell());
        }
    }

    #[test]
    fn radius_9_matches_game_chunks() {
        let lat = HexLattice::new(9);
        assert_eq!(lat.v1, (10, 9));
        assert_eq!(lat.v2, (-9, 19));
        assert_eq!(lat.tiles_per_cell(), 271);
    }

    #[test]
    fn tile_count_matches_iteration() {
        for radius in [1, 2, 5, 9, 20] {
            let lat = HexLattice::new(radius);
            let count = lat.tiles_in_cell((0, 0)).count();
            assert_eq!(count, lat.tiles_per_cell() as usize,
                "radius {radius}: iterated {count} tiles, expected {}", lat.tiles_per_cell());
        }
    }

    #[test]
    fn roundtrip_all_tiles() {
        for radius in [1, 5, 9, 20] {
            let lat = HexLattice::new(radius);
            for cell in [(0, 0), (1, 0), (0, 1), (-1, 2), (3, -2)] {
                for (q, r) in lat.tiles_in_cell(cell) {
                    let recovered = lat.cell_id(q, r);
                    assert_eq!(recovered, cell,
                        "radius {radius}: tile ({q},{r}) in cell {cell:?} mapped to {recovered:?}");
                }
            }
        }
    }

    #[test]
    fn no_gaps_no_overlaps() {
        // For a region, verify every tile belongs to exactly one cell
        let lat = HexLattice::new(9);
        let mut tile_owners: HashMap<(i32, i32), (i32, i32)> = HashMap::new();

        // Enumerate tiles from several cells
        for n in -2..=2 {
            for m in -2..=2 {
                let cell = (n, m);
                for (q, r) in lat.tiles_in_cell(cell) {
                    if let Some(prev) = tile_owners.insert((q, r), cell) {
                        panic!("tile ({q},{r}) claimed by both {prev:?} and {cell:?}");
                    }
                }
            }
        }

        // Verify all tiles in the covered area are accounted for
        let center = lat.cell_center((0, 0));
        for dq in -15..=15 {
            for dr in -15..=15 {
                let q = center.0 + dq;
                let r = center.1 + dr;
                let cell = lat.cell_id(q, r);
                if cell.0.abs() <= 2 && cell.1.abs() <= 2 {
                    assert!(tile_owners.contains_key(&(q, r)),
                        "tile ({q},{r}) in cell {cell:?} not enumerated");
                }
            }
        }
    }

    #[test]
    fn no_gaps_no_overlaps_large_radius() {
        let lat = HexLattice::new(64);
        let mut seen: HashSet<(i32, i32)> = HashSet::new();

        for n in -1..=1 {
            for m in -1..=1 {
                let cell = (n, m);
                for (q, r) in lat.tiles_in_cell(cell) {
                    assert!(seen.insert((q, r)),
                        "tile ({q},{r}) claimed by multiple cells");
                }
            }
        }
    }

    #[test]
    fn adjacent_centers_at_correct_distance() {
        for radius in [1, 5, 9, 64] {
            let lat = HexLattice::new(radius);
            let (cq, cr) = lat.cell_center((0, 0));
            let expected_dist = 2 * radius as i32 + 1;

            for &nbr in &lat.neighbor_cells((0, 0)) {
                let (nq, nr) = lat.cell_center(nbr);
                let dq = cq - nq;
                let dr = cr - nr;
                let dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
                assert_eq!(dist, expected_dist,
                    "radius {radius}: neighbor {nbr:?} center at distance {dist}, expected {expected_dist}");
            }
        }
    }
}
