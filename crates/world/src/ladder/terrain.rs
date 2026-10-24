//! The ladder for readers on many threads: one lock round it, every level
//! built on request down to the tiles, a cell read as it was built. A cell
//! once finished never changes, so the cells read lately are kept beside
//! the lock, and a read that finds all it asks for there never waits on a
//! build.

use std::sync::{Mutex, MutexGuard};

use dashmap::DashMap;

use super::{Bounds, Ground, Ladder, Params, NB};

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

/// How many cells read lately are kept beside the lock before they are
/// all forgotten: every cell a few score summary regions and chunks in
/// flight read at once, each with its ring.
pub const RECENT_CELLS: usize = 1 << 19;

/// The ladder behind a lock.
pub struct Terrain<'g, G: Ground> {
    ladder: Mutex<Ladder<'g, G>>,
    /// Cells read lately by level width and index, each finished, or none
    /// where the ladder does not hold it.
    recent: DashMap<(i64, i64, i64), Option<Cell>>,
}

impl<'g, G: Ground> Terrain<'g, G> {
    /// The ladder over `bounds` ([`Ladder::new_box`]), its top built.
    pub fn new_box(ground: &'g G, bounds: &[Bounds], p: Params) -> Self {
        Terrain::from_ladder(Ladder::new_box(ground, bounds, p))
    }

    pub fn from_ladder(ladder: Ladder<'g, G>) -> Self {
        Terrain { ladder: Mutex::new(ladder), recent: DashMap::new() }
    }

    /// The ladder, for a request or a reading of it.
    pub fn ladder(&self) -> MutexGuard<'_, Ladder<'g, G>> {
        self.ladder.lock().expect("the ladder's lock")
    }

    pub fn params(&self) -> Params {
        self.ladder().p
    }

    /// Cells of the level `w` wide, built where not yet; none where the
    /// ladder has no such level or does not hold the cell. Each cell asked
    /// for is kept, with each of its ring finished by then.
    pub fn cells(&self, w: i64, at: &[(i64, i64)]) -> Vec<Option<Cell>> {
        let kept: Option<Vec<Option<Cell>>> = at.iter().map(|&(i, j)| self.recent.get(&(w, i, j)).map(|c| c.clone())).collect();
        if let Some(kept) = kept {
            return kept;
        }
        let mut ladder = self.ladder();
        let Some(k) = ladder.levels.iter().position(|l| l.w == w) else { return vec![None; at.len()] };
        let lv = &ladder.levels[k];
        let held: Vec<(i64, i64)> = at.iter().copied().filter(|&(i, j)| lv.idx(i, j).is_some_and(|c| lv.exists[c])).collect();
        if !held.is_empty() {
            ladder.request(k, &held);
        }
        let lv = &ladder.levels[k];
        let read: Vec<Option<Cell>> = at.iter().map(|&(i, j)| lv.idx(i, j).filter(|&c| lv.exists[c] && lv.built.get(c)).map(|c| cell(lv, c))).collect();
        if self.recent.len() + 7 * at.len() > RECENT_CELLS {
            self.recent.clear();
        }
        for &(i, j) in at {
            for (di, dj) in std::iter::once((0, 0)).chain(NB) {
                let (i, j) = (i + di, j + dj);
                match lv.idx(i, j).filter(|&c| lv.exists[c]) {
                    Some(c) if lv.finished.get(c) => self.recent.insert((w, i, j), Some(cell(lv, c))),
                    Some(_) => None,
                    None => self.recent.insert((w, i, j), None),
                };
            }
        }
        read
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
