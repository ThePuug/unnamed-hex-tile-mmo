//! The ladder for readers on many threads: every level built on request
//! down to the tiles, separate areas on separate threads at once
//! ([`Ladder`]), a cell read as it was built.

use super::{Bounds, Ground, Ladder, Params};

/// A cell of any level as a reader takes it.
#[derive(Clone, Debug)]
pub struct Cell {
    /// Its height, and its dry height: its ground before any cut.
    pub z: f64,
    pub zd: f64,
    /// Water's cut below the dry ground.
    pub cut: f64,
    /// The surface of the water standing over the cell, in levels.
    pub water: Option<f64>,
    /// Which of its nine children stand under its level's water above the
    /// sea, a bit to each ([`super::Level::wparts`]).
    pub wet_parts: u16,
}

/// The ladder, shared by its readers.
pub struct Terrain<'g, G: Ground> {
    ladder: Ladder<'g, G>,
}

impl<'g, G: Ground> Terrain<'g, G> {
    /// The ladder over `bounds` ([`Ladder::new_box`]), its top built.
    pub fn new_box(ground: &'g G, bounds: &[Bounds], p: Params) -> Self {
        Terrain::from_ladder(Ladder::new_box(ground, bounds, p))
    }

    pub fn from_ladder(ladder: Ladder<'g, G>) -> Self {
        Terrain { ladder }
    }

    /// The ladder, for a request or a reading of it.
    pub fn ladder(&self) -> &Ladder<'g, G> {
        &self.ladder
    }

    pub fn params(&self) -> Params {
        self.ladder.p
    }

    /// Cells of the level `w` wide, built where not yet; none where the
    /// ladder has no such level or does not hold the cell.
    pub fn cells(&self, w: i64, at: &[(i64, i64)]) -> Vec<Option<Cell>> {
        let ladder = &self.ladder;
        let Some(k) = ladder.levels.iter().position(|l| l.w == w) else { return vec![None; at.len()] };
        let lv = &ladder.levels[k];
        let held: Vec<(i64, i64)> = at.iter().copied().filter(|&(i, j)| lv.idx(i, j).is_some_and(|c| lv.exists[c])).collect();
        if !held.is_empty() {
            ladder.request(k, &held);
        }
        at.iter().map(|&(i, j)| lv.idx(i, j).filter(|&c| lv.exists[c] && lv.built.done(c)).map(|c| cell(lv, c))).collect()
    }
}

fn cell(lv: &super::Level, c: usize) -> Cell {
    Cell {
        z: lv.z.get(c),
        zd: lv.zd.get(c),
        cut: lv.cut.get(c),
        water: Some(lv.water.get(c)).filter(|w| !w.is_nan()),
        wet_parts: lv.wparts.get(c),
    }
}
