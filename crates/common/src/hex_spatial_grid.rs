//! Cells over the world crate's plane, each holding what reaches it: an
//! item is inserted into every cell within its reach of its place, and a
//! point reads the one cell it falls in, exactly. The cells are
//! pointy-top hexes in odd-r offset coordinates on the `f64` (x, y) plane
//! the world crate lays its features on, not the game's flat-top tile
//! grid; `cell_size` is the step between cell centres along a row.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Multiply-shift hasher for hex cell keys. A grid lookup is a handful of
/// arithmetic ops around the hash, so a general-purpose hash is most of its
/// cost — a pair of small integers is already well spread by one multiply
/// against a 64-bit odd constant, with the fold putting entropy in the low
/// bits the table indexes on.
#[derive(Default)]
pub struct HexHasher(u64);

const HEX_HASH_K: u64 = 0x9E37_79B9_7F4A_7C15;

impl Hasher for HexHasher {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(HEX_HASH_K);
        }
    }
    fn write_i32(&mut self, v: i32) {
        self.0 = (self.0 ^ v as u32 as u64).wrapping_mul(HEX_HASH_K);
    }
}

/// Cell buckets. Keys are small integer pairs, so the hasher above beats the
/// default on every lookup, and the grid is looked up a great deal.
type CellMap<T> = HashMap<(i32, i32), Vec<T>, BuildHasherDefault<HexHasher>>;

/// √3/2 — the row spacing of the cell lattice as a share of `cell_size`.
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

/// Cells over the plane, each holding a copy of every item inserted within
/// reach of it.
pub struct HexSpatialGrid<T> {
    cells: CellMap<T>,
    cell_size: f64,
}

impl<T> HexSpatialGrid<T> {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cells: CellMap::default(),
            cell_size,
        }
    }

    /// The cell a point falls in (odd-r offset).
    pub fn cell_at(&self, wx: f64, wy: f64) -> (i32, i32) {
        let row_height = self.cell_size * HEX_ROW_HEIGHT;
        let cr = (wy / row_height).round() as i32;
        let odd_shift = if cr & 1 != 0 { self.cell_size * 0.5 } else { 0.0 };
        let cq = ((wx - odd_shift) / self.cell_size).round() as i32;
        (cq, cr)
    }

    fn cell_center(&self, cq: i32, cr: i32) -> (f64, f64) {
        let odd_shift = if cr & 1 != 0 { self.cell_size * 0.5 } else { 0.0 };
        (
            cq as f64 * self.cell_size + odd_shift,
            cr as f64 * self.cell_size * HEX_ROW_HEIGHT,
        )
    }

    /// Insert an item into every cell whose centre lies within `radius` of
    /// `(wx, wy)`, a cell's width to spare, so a point in any cell the
    /// item's reach touches reads it from its own cell.
    pub fn insert_radius(&mut self, wx: f64, wy: f64, radius: f64, item: T)
    where
        T: Clone,
    {
        let min_cq = ((wx - radius) / self.cell_size).floor() as i32 - 1;
        let max_cq = ((wx + radius) / self.cell_size).ceil() as i32 + 1;
        let row_height = self.cell_size * HEX_ROW_HEIGHT;
        let min_cr = ((wy - radius) / row_height).floor() as i32 - 1;
        let max_cr = ((wy + radius) / row_height).ceil() as i32 + 1;

        for cr in min_cr..=max_cr {
            for cq in min_cq..=max_cq {
                let (ccx, ccy) = self.cell_center(cq, cr);
                let dx = wx - ccx;
                let dy = wy - ccy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= radius + self.cell_size {
                    self.cells.entry((cq, cr)).or_default().push(item.clone());
                }
            }
        }
    }

    /// Everything inserted within reach of a cell, or None where nothing is.
    pub fn cell_contents(&self, cell: (i32, i32)) -> Option<&Vec<T>> {
        self.cells.get(&cell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_query_same_cell() {
        let mut grid = HexSpatialGrid::new(100.0);
        grid.insert_radius(50.0, 50.0, 0.0, 42);
        let items = grid.cell_contents(grid.cell_at(50.0, 50.0));
        assert!(items.is_some_and(|items| items.contains(&42)));
    }

    #[test]
    fn cell_at_matches_odd_r_convention() {
        let grid: HexSpatialGrid<()> = HexSpatialGrid::new(1800.0);
        // Origin maps to (0, 0)
        assert_eq!(grid.cell_at(0.0, 0.0), (0, 0));
        // One cell to the right
        assert_eq!(grid.cell_at(1800.0, 0.0), (1, 0));
        // One row down, odd row has half-cell shift
        let row_h = 1800.0 * HEX_ROW_HEIGHT;
        let (_, cr) = grid.cell_at(0.0, row_h);
        assert_eq!(cr, 1);
    }

    #[test]
    fn insert_radius_covers_nearby_cells() {
        let mut grid = HexSpatialGrid::new(100.0);
        grid.insert_radius(50.0, 50.0, 150.0, 7);
        let items = grid.cell_contents(grid.cell_at(0.0, 0.0));
        assert!(items.is_some_and(|items| items.contains(&7)));
    }
}
