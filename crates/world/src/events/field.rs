//! A layer's value sampled over the ground, published for the structural
//! passes of the layers above.
//!
//! A tile carries what a layer knows at that tile, and a query reads it
//! there. A structural pass has no tile: it places features at points of
//! its own, a stand's origin, and reading a tile at each would compose the
//! whole stack beneath it a point at a time. So a layer whose value such a
//! pass needs samples it on a lattice of tiles and publishes the samples,
//! each lying where it was taken, and the pass above reads between them
//! through its own scope ([`Field`]). No layer calls another's code for a
//! value, and none builds for itself what the layer beneath it knows.
//!
//! Samples are published in blocks, [`BLOCK`] a side, by the cell a
//! block's middle lies in, and a block is filled the first time one of its
//! samples is read: a cell no pass reads samples nothing.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};

use super::index::{CellId, CellIndex, Located};
use super::CellScope;
use crate::hex_to_world;

/// The samples along a block's side.
pub const BLOCK: i32 = 16;

/// What a layer samples its value by: the value at a world position.
pub type Source = Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>;

/// A block of a layer's samples: sample (a, b) is the value at the tile
/// `(a, b) · spacing`, and the block holds those whose a and b fall in its
/// own [`BLOCK`] of each.
pub struct Block {
    id: (i32, i32),
    spacing: i32,
    bounds: [f64; 4],
    source: Source,
    values: OnceLock<Vec<f64>>,
}

impl Block {
    fn value(&self, a: i32, b: i32) -> f64 {
        let values = self.values.get_or_init(|| {
            (0..BLOCK)
                .flat_map(|j| (0..BLOCK).map(move |i| (i, j)))
                .map(|(i, j)| {
                    let (wx, wy) = hex_to_world((self.id.0 * BLOCK + i) * self.spacing, (self.id.1 * BLOCK + j) * self.spacing);
                    (self.source)(wx, wy)
                })
                .collect()
        });
        values[((a - self.id.0 * BLOCK) + (b - self.id.1 * BLOCK) * BLOCK) as usize]
    }
}

impl Located for Arc<Block> {
    fn bounds(&self) -> [f64; 4] {
        self.bounds
    }
}

/// The samples of the value `M` names, a cell's blocks to each cell of
/// the layer that takes them.
pub struct Sampled<M> {
    cells: HashMap<CellId, Vec<Arc<Block>>>,
    of: PhantomData<fn() -> M>,
}

impl<M> Default for Sampled<M> {
    fn default() -> Self {
        Sampled { cells: HashMap::new(), of: PhantomData }
    }
}

impl<M: 'static> CellIndex for Sampled<M> {
    type Cell = Vec<Arc<Block>>;
    type Feature = Arc<Block>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }

    fn features(entry: &Self::Cell) -> impl Iterator<Item = &Arc<Block>> {
        entry.iter()
    }
}

/// The blocks of samples `spacing` tiles apart whose middles lie in the
/// scope's cell, each to be filled from `source`: what a layer's
/// structural pass publishes of its value.
pub fn blocks_of(scope: &CellScope, spacing: i32, source: Source) -> Vec<Arc<Block>> {
    let (lattice, cell) = (scope.lattice(), scope.cell());
    let (cq, cr) = lattice.cell_center(cell);
    let (side, reach) = (BLOCK * spacing, lattice.radius as i32);
    let over = |c: i32| (c - reach).div_euclid(side) - 1..=(c + reach).div_euclid(side) + 1;
    let mut out = Vec::new();
    for j in over(cr) {
        for i in over(cq) {
            if lattice.cell_id(i * side + side / 2, j * side + side / 2) != cell {
                continue;
            }
            let corners = [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)].map(|(a, b)| hex_to_world(a * side, b * side));
            let (x0, x1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |at, c| (at.0.min(c.0), at.1.max(c.0)));
            let (y0, y1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |at, c| (at.0.min(c.1), at.1.max(c.1)));
            out.push(Arc::new(Block { id: (i, j), spacing, bounds: [x0, y0, x1, y1], source: source.clone(), values: OnceLock::new() }));
        }
    }
    out
}

/// A layer's sampled value as one cell of a layer above reads it: the
/// blocks lying in that cell and its ring, read between their samples.
pub struct Field {
    spacing: i32,
    blocks: HashMap<(i32, i32), Arc<Block>>,
}

impl Field {
    /// The value `M` names over the scope's neighbourhood, its samples
    /// `spacing` tiles apart.
    pub fn of<M: 'static>(scope: &CellScope, spacing: i32) -> Self {
        let blocks = scope.read::<Sampled<M>>().map(|read| read.features().map(|block| (block.id, block.clone())).collect()).unwrap_or_default();
        Field { spacing, blocks }
    }

    fn sample(&self, a: i32, b: i32) -> f64 {
        let id = (a.div_euclid(BLOCK), b.div_euclid(BLOCK));
        let block = self.blocks.get(&id).unwrap_or_else(|| panic!("sample ({a}, {b}) is in no block the reader was handed: no layer beneath publishes the value, or the reader's cell is too small for what it reads"));
        block.value(a, b)
    }

    /// The value at a world position inside the neighbourhood: the four
    /// samples about it, each by its nearness along the lattice's two
    /// ways.
    pub fn at(&self, wx: f64, wy: f64) -> f64 {
        let r = wy * 2.0 / crate::SQRT_3;
        let (a, b) = ((wx - r / 2.0) / self.spacing as f64, r / self.spacing as f64);
        let (a0, b0) = (a.floor(), b.floor());
        let (fa, fb) = (a - a0, b - b0);
        let (a0, b0) = (a0 as i32, b0 as i32);
        let along = |b: i32| self.sample(a0, b) + (self.sample(a0 + 1, b) - self.sample(a0, b)) * fa;
        along(b0) + (along(b0 + 1) - along(b0)) * fb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field read at a sample's own tile gives the sample, and between
    /// two samples a value between theirs.
    #[test]
    fn a_field_reads_its_samples_and_between_them() {
        let source: Source = Arc::new(|wx, wy| wx + 2.0 * wy);
        let spacing = 8;
        let blocks: HashMap<(i32, i32), Arc<Block>> = [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (0, -1), (-1, -1), (-1, 1), (1, -1)]
            .into_iter()
            .map(|id| (id, Arc::new(Block { id, spacing, bounds: [0.0; 4], source: source.clone(), values: OnceLock::new() })))
            .collect();
        let field = Field { spacing, blocks };
        for (q, r) in [(0, 0), (8, 16), (-24, 40), (120, -8)] {
            let (wx, wy) = hex_to_world(q, r);
            assert!((field.at(wx, wy) - source(wx, wy)).abs() < 1e-9, "at the sample ({q}, {r})");
        }
        // The source is a plane, which reading between samples keeps.
        let (wx, wy) = hex_to_world(13, 21);
        assert!((field.at(wx + 0.3, wy - 0.2) - source(wx + 0.3, wy - 0.2)).abs() < 1e-9);
    }
}
