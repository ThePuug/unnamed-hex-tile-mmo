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

use common::HexSpatialGrid;

use crate::faces::FaceIndex;
use crate::glacial::Cirque;
use super::index::{CellId, EventIndex};
use super::spines::SPINE_CELL_SCALE;

// ── Faces ───────────────────────────────────────────────────────────────────

/// Faces the erosional layers leave standing, by the cell that cut them.
#[derive(Default)]
pub struct ErosionalFaceIndex {
    pub cells: HashMap<CellId, FaceIndex>,
}

impl ErosionalFaceIndex {
    /// Debris standing at (wx, wy), over the faces in the given cells.
    pub fn apron_in(&self, cells: &[CellId], wx: f64, wy: f64, repose: f64, cap: f64) -> f64 {
        let mut apron = 0.0f64;
        for id in cells {
            if let Some(faces) = self.cells.get(id) {
                let a = faces.apron_at(wx, wy, repose, cap);
                if a > apron { apron = a; }
            }
        }
        apron
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
    pub cells: HashMap<CellId, HexSpatialGrid<Cirque>>,
}

impl BasinIndex {
    /// Record a cell's bowls, replacing whatever it held.
    pub fn set_cell(&mut self, cell_id: CellId, cirques: &[Cirque]) {
        let mut grid = HexSpatialGrid::new(BASIN_CELL_SIZE);
        for c in cirques {
            grid.insert_radius(c.cx, c.cy, c.radius, c.clone());
        }
        self.cells.insert(cell_id, grid);
    }

    /// The level nothing may cut below at (wx, wy), or `None` where no basin
    /// claims the point.
    ///
    /// Bowls within one cell resolve to the lowest claim — stacked cirques
    /// really do drain each other, and the layer that cut them already applied
    /// that. Across cells the highest claim binds: a cut below one basin's
    /// spill altitude opens that basin whatever a neighbour has under the same
    /// ground.
    pub fn impound_in(&self, cells: &[CellId], wx: f64, wy: f64) -> Option<f64> {
        let mut bound: Option<f64> = None;
        for id in cells {
            let Some(grid) = self.cells.get(id) else { continue };
            let mut lowest: Option<f64> = None;
            for c in grid.query(wx, wy) {
                if let Some(l) = c.base_level(wx, wy) {
                    lowest = Some(lowest.map_or(l, |a: f64| a.min(l)));
                }
            }
            if let Some(l) = lowest {
                bound = Some(bound.map_or(l, |a: f64| a.max(l)));
            }
        }
        bound
    }
}

impl EventIndex for BasinIndex {
    fn source_scale(&self) -> u32 { SPINE_CELL_SCALE }
    fn tiles(&self, _cell_ids: &[CellId]) -> Vec<(i32, i32)> { Vec::new() }
    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { Vec::new() }
    fn remove_cell(&mut self, cell_id: CellId) { self.cells.remove(&cell_id); }
}
