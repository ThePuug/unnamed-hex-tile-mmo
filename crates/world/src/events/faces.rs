//! Indexes the erosional layers publish for the layers above them.

//! Both answer questions about geometry a carve deliberately created, so a
//! consumer never has to read the surface around a tile to infer it back:
//! - [`ErosionalFaceIndex`] — where the walls are, and what lies at their feet.
//! - [`BasinIndex`] — the level a closed basin impounds, which nothing above
//!   the water layer may cut below.

//! Both are keyed by the cell of the layer that fills them, so the framework
//! evicts them with it and a reader resolves a tile from its own cell plus the
//! ring, the same envelope every other layer is scoped by.

use std::collections::HashMap;
use std::sync::Arc;

use common::HexSpatialGrid;

use crate::faces::FaceIndex;
use crate::glacial::Cirque;
use super::index::{CellId, CellIndex, EventIndex};
use super::spines::SPINE_CELL_SCALE;

// ── Faces ───────────────────────────────────────────────────────────────────

/// Faces the erosional layers leave standing, by the cell that cut them.
#[derive(Default)]
pub struct ErosionalFaceIndex {
    pub cells: HashMap<CellId, Arc<FaceIndex>>,
}

impl CellIndex for ErosionalFaceIndex {
    type Cell = Arc<FaceIndex>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl EventIndex for ErosionalFaceIndex {
    fn source_scale(&self) -> u32 { SPINE_CELL_SCALE }
    fn tiles(&self, _cell_ids: &[CellId]) -> Vec<(i32, i32)> { Vec::new() }
    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }
    fn remove_cell(&mut self, cell_id: CellId) { self.cells.remove(&cell_id); }
}

// ── Basins ──────────────────────────────────────────────────────────────────

/// Cell size of the basin lookup, in world units. Held near the largest bowl a
/// spine sites so a footprint spans a couple of cells rather than tens.
const BASIN_CELL_SIZE: f64 = 512.0;

/// Closed basins, by the cell that cut them.
///
/// A bowl claims every point inside its footprint, so it is inserted across
/// that footprint and one lookup returns whichever bowls stand over a tile.
#[derive(Default)]
pub struct BasinIndex {
    pub cells: HashMap<CellId, Arc<HexSpatialGrid<Cirque>>>,
}

impl CellIndex for BasinIndex {
    type Cell = Arc<HexSpatialGrid<Cirque>>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }
}

impl BasinIndex {
    /// Index a cell's bowls for lookup by point.
    pub fn grid_of(cirques: impl IntoIterator<Item = Cirque>) -> Arc<HexSpatialGrid<Cirque>> {
        let mut grid = HexSpatialGrid::new(BASIN_CELL_SIZE);
        for c in cirques {
            grid.insert_radius(c.cx, c.cy, c.radius, c.clone());
        }
        Arc::new(grid)
    }
}

impl EventIndex for BasinIndex {
    fn source_scale(&self) -> u32 { SPINE_CELL_SCALE }
    fn tiles(&self, _cell_ids: &[CellId]) -> Vec<(i32, i32)> { Vec::new() }
    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }
    fn remove_cell(&mut self, cell_id: CellId) { self.cells.remove(&cell_id); }
}
