use std::collections::HashMap;

/// √3/2 — row height factor for hex grids with flat-top orientation.
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

/// Hex-indexed spatial grid. Items are stored in hex cells (odd-r offset)
/// sized to a query radius. Queries return items from the target cell + 6
/// hex neighbors, guaranteeing coverage within one cell_size of any point.
pub struct HexSpatialGrid<T> {
    cells: HashMap<(i32, i32), Vec<T>>,
    cell_size: f64,
}

impl<T> HexSpatialGrid<T> {
    /// Create a new grid with the given cell size.
    pub fn new(cell_size: f64) -> Self {
        Self {
            cells: HashMap::new(),
            cell_size,
        }
    }

    /// Cell size used for this grid.
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }

    /// Get the cell coordinate for a world position (odd-r offset).
    pub fn cell_at(&self, wx: f64, wy: f64) -> (i32, i32) {
        let row_height = self.cell_size * HEX_ROW_HEIGHT;
        let cr = (wy / row_height).round() as i32;
        let odd_shift = if cr & 1 != 0 { self.cell_size * 0.5 } else { 0.0 };
        let cq = ((wx - odd_shift) / self.cell_size).round() as i32;
        (cq, cr)
    }

    /// Center world position of a cell.
    pub fn cell_center(&self, cq: i32, cr: i32) -> (f64, f64) {
        let odd_shift = if cr & 1 != 0 { self.cell_size * 0.5 } else { 0.0 };
        (
            cq as f64 * self.cell_size + odd_shift,
            cr as f64 * self.cell_size * HEX_ROW_HEIGHT,
        )
    }

    /// The 7 cell offsets (self + 6 neighbors) for odd-r offset at row `cr`.
    pub fn neighborhood_offsets(cr: i32) -> [(i32, i32); 7] {
        if cr & 1 == 0 {
            [(0, 0), (-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
        } else {
            [(0, 0), (-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
        }
    }

    /// Insert an item at a world position (single cell).
    pub fn insert(&mut self, wx: f64, wy: f64, item: T) {
        let cell = self.cell_at(wx, wy);
        self.cells.entry(cell).or_default().push(item);
    }

    /// Insert an item into all cells its influence radius overlaps.
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
                // Conservative check: cell center within radius + cell diagonal
                let dx = wx - ccx;
                let dy = wy - ccy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= radius + self.cell_size {
                    self.cells.entry((cq, cr)).or_default().push(item.clone());
                }
            }
        }
    }

    /// Query all items in the 7-cell neighborhood of (wx, wy).
    pub fn query(&self, wx: f64, wy: f64) -> impl Iterator<Item = &T> {
        let (cq, cr) = self.cell_at(wx, wy);
        let offsets = Self::neighborhood_offsets(cr);
        // Collect into a vec to avoid lifetime issues with the closure
        let mut items: Vec<&T> = Vec::new();
        for (dq, dr) in offsets {
            if let Some(cell) = self.cells.get(&(cq + dq, cr + dr)) {
                items.extend(cell.iter());
            }
        }
        items.into_iter()
    }

    /// Query into a provided buffer, avoiding allocation on repeated calls.
    pub fn query_into<'a>(&'a self, wx: f64, wy: f64, buf: &mut Vec<&'a T>) {
        buf.clear();
        let (cq, cr) = self.cell_at(wx, wy);
        let offsets = Self::neighborhood_offsets(cr);
        for (dq, dr) in offsets {
            if let Some(cell) = self.cells.get(&(cq + dq, cr + dr)) {
                buf.extend(cell.iter());
            }
        }
    }

    /// Get all items in a specific cell.
    pub fn cell_contents(&self, cell: (i32, i32)) -> Option<&Vec<T>> {
        self.cells.get(&cell)
    }

    /// Mutable access to all items in a specific cell.
    pub fn cell_contents_mut(&mut self, cell: (i32, i32)) -> Option<&mut Vec<T>> {
        self.cells.get_mut(&cell)
    }

    /// Get or create the contents of a specific cell.
    pub fn cell_entry(&mut self, cell: (i32, i32)) -> &mut Vec<T> {
        self.cells.entry(cell).or_default()
    }

    /// Number of cells with items.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Iterate over all cells and their contents.
    pub fn cells(&self) -> impl Iterator<Item = (&(i32, i32), &Vec<T>)> {
        self.cells.iter()
    }

    /// Mutable iteration over all cells.
    pub fn cells_mut(&mut self) -> impl Iterator<Item = (&(i32, i32), &mut Vec<T>)> {
        self.cells.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_query_same_cell() {
        let mut grid = HexSpatialGrid::new(100.0);
        grid.insert(50.0, 50.0, 42);
        let items: Vec<&i32> = grid.query(50.0, 50.0).collect();
        assert!(items.contains(&&42));
    }

    #[test]
    fn query_finds_neighbor_cell() {
        let mut grid = HexSpatialGrid::new(100.0);
        // Insert at origin cell
        grid.insert(0.0, 0.0, 1);
        // Query from adjacent cell — should still find item in 1-ring
        let items: Vec<&i32> = grid.query(100.0, 0.0).collect();
        assert!(items.contains(&&1));
    }

    #[test]
    fn query_misses_distant_cell() {
        let mut grid = HexSpatialGrid::new(100.0);
        grid.insert(0.0, 0.0, 1);
        // Two cells away — outside 1-ring
        let items: Vec<&i32> = grid.query(300.0, 300.0).collect();
        assert!(items.is_empty());
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
    fn query_into_clears_and_fills() {
        let mut grid = HexSpatialGrid::new(100.0);
        grid.insert(0.0, 0.0, 10);
        grid.insert(0.0, 0.0, 20);

        let mut buf: Vec<&i32> = vec![&99]; // pre-existing junk
        grid.query_into(0.0, 0.0, &mut buf);
        assert_eq!(buf.len(), 2);
        assert!(buf.contains(&&10));
        assert!(buf.contains(&&20));
    }

    #[test]
    fn insert_radius_covers_nearby_cells() {
        let mut grid = HexSpatialGrid::new(100.0);
        grid.insert_radius(50.0, 50.0, 150.0, 7);
        // Should be findable from the origin cell
        let items: Vec<&i32> = grid.query(0.0, 0.0).collect();
        assert!(items.contains(&&7));
    }

    #[test]
    fn neighborhood_offsets_even_row() {
        let offsets = HexSpatialGrid::<()>::neighborhood_offsets(0);
        assert_eq!(offsets.len(), 7);
        assert!(offsets.contains(&(0, 0)));
    }

    #[test]
    fn neighborhood_offsets_odd_row() {
        let offsets = HexSpatialGrid::<()>::neighborhood_offsets(1);
        assert_eq!(offsets.len(), 7);
        assert!(offsets.contains(&(0, 0)));
        // Odd row: diagonal neighbors shift right
        assert!(offsets.contains(&(1, -1)));
        assert!(offsets.contains(&(1, 1)));
    }
}
