//! The water layer for readers on many threads: every level built on request
//! down to the tiles, separate areas on separate threads at once
//! ([`Landmass`]), a cell read as it was built.

use super::{Bounds, Ground, Landmass, Params};

/// A cell of any level as a reader takes it.
#[derive(Clone, Debug)]
pub struct Cell {
    /// Its height, and its dry height: its ground before any cut.
    pub z: f64,
    pub zd: f64,
    /// Its ground and cut before they are rounded to its height: `z` is
    /// this to the nearest level.
    pub fine: f64,
    /// Water's cut below the dry ground.
    pub cut: f64,
    /// The surface of the water standing over the cell, in levels: a
    /// river's or a lake's, and the sea's, zero, over the sea
    /// ([`super::Level::sea`]).
    pub water: Option<f64>,
    /// How far that water falls a tile along its course, in levels: a
    /// river's graded fall, and nothing over a lake or dry ground.
    pub fall: f64,
    /// Which of its nine children stand under its level's water above the
    /// sea, a bit to each ([`super::Level::wparts`]).
    pub wet_parts: u16,
}

/// The water layer, shared by its readers.
pub struct Terrain<'g, G: Ground> {
    landmass: Landmass<'g, G>,
}

impl<'g, G: Ground> Terrain<'g, G> {
    /// The water layer over `bounds` ([`Landmass::new_box`]), its top built.
    pub fn new_box(ground: &'g G, bounds: &[Bounds], p: Params) -> Self {
        Terrain::from_landmass(Landmass::new_box(ground, bounds, p))
    }

    pub fn from_landmass(landmass: Landmass<'g, G>) -> Self {
        Terrain { landmass }
    }

    /// The water layer, for a request or a reading of it.
    pub fn landmass(&self) -> &Landmass<'g, G> {
        &self.landmass
    }

    pub fn params(&self) -> Params {
        self.landmass.p
    }

    /// Cells of the level `w` wide, built where not yet; none where the
    /// water layer has no such level or does not hold the cell.
    pub fn cells(&self, w: i64, at: &[(i64, i64)]) -> Vec<Option<Cell>> {
        let landmass = &self.landmass;
        let Some(k) = landmass.levels.iter().position(|l| l.w == w) else { return vec![None; at.len()] };
        let lv = &landmass.levels[k];
        let held: Vec<(i64, i64)> = at.iter().copied().filter(|&(i, j)| lv.idx(i, j).is_some_and(|c| lv.exists[c])).collect();
        if !held.is_empty() {
            landmass.request(k, &held);
        }
        at.iter().map(|&(i, j)| lv.idx(i, j).filter(|&c| lv.exists[c] && lv.built.done(c)).map(|c| cell(lv, c))).collect()
    }

    /// The rivers leaving each cell of the level `w` wide at `at`
    /// ([`Landmass::courses`]); none where the water layer has no such level.
    pub fn courses(&self, w: i64, at: &[(i64, i64)]) -> Vec<Vec<super::Course>> {
        match self.landmass.levels.iter().position(|l| l.w == w) {
            Some(k) => self.landmass.courses(k, at),
            None => vec![Vec::new(); at.len()],
        }
    }
}

fn cell(lv: &super::Level, c: usize) -> Cell {
    Cell {
        z: lv.z.get(c),
        zd: lv.zd.get(c),
        fine: lv.ground.get(c) + lv.cut.get(c),
        cut: lv.cut.get(c),
        water: Some(lv.water.get(c)).filter(|w| !w.is_nan()).or(lv.sea(c).then_some(0.0)),
        fall: Some(lv.wfall.get(c)).filter(|f| !f.is_nan() && !lv.water.get(c).is_nan()).unwrap_or(0.0),
        wet_parts: lv.wparts.get(c),
    }
}
