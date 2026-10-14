//! The stack read as the ladder's ground ([`crate::ladder::Ground`]): two
//! layers, thrusting's relief averaged over a cell, and the rest of the base
//! layers, plates through lithology, as they compose, the mean over the
//! cell's centre and the midpoints of its edges. The ladder is built on
//! these and never sees a layer's index or output (INV-009).
//!
//! Cell (i, j) of a level `w` tiles wide is centred on tile `(i, j)·w`, so
//! the world's origin is a centre at every level, as a summary's is
//! (`common::summary::center_tile`), and a summary of a level is that
//! level's cell. A cell's tiles are its descendants, the nine children of
//! each cell down to the tiles; its rock and its plate are those at its
//! centre.

use dashmap::DashMap;

use super::thrusting::Outlines;
use super::{lithology, motion, plates, thickening, thrusting, tilt, Composite};
use crate::hex_to_world;
use crate::ladder::fine::{Cell, Terrain, FINE};
use crate::ladder::geom::{cube_round, descendants, NB};
use crate::ladder::{Bounds, Ground, Ladder, Params};

/// A cell's centre and the midpoints of its six edges, as tile offsets.
fn samples(w: i64) -> Vec<(i64, i64)> {
    if w == 1 {
        return vec![(0, 0)];
    }
    let h = w / 2;
    vec![(0, 0), (h, 0), (-h, 0), (0, h), (0, -h), (h, -h), (-h, h)]
}

/// What a cell read once: each layer's value, the rock's erodibility and
/// the plate's age.
type Read = (Vec<f64>, f64, f64);

/// The base layers as the ladder's two.
pub struct StackGround {
    base: Composite,
    outlines: Outlines,
    /// How many levels down thrust's relief is averaged over: at most
    /// `9^depth` reads a cell, the tiles themselves where the cell has
    /// fewer.
    depth: u32,
    memo: DashMap<(i64, i64, i64), Read>,
}

impl Composite {
    /// The layers the ladder is built on: plates through lithology.
    pub fn base(seed: u64) -> Self {
        let mut composite = Self::new(seed);
        composite.add_event(Box::new(plates::PlateEvent::new()));
        composite.add_event(Box::new(tilt::TiltEvent::new()));
        composite.add_event(Box::new(motion::MotionEvent::new()));
        composite.add_event(Box::new(thrusting::ThrustingEvent::new()));
        composite.add_event(Box::new(thickening::ThickeningEvent::new()));
        composite.add_event(Box::new(lithology::LithologyEvent::new()));
        composite
    }
}

impl StackGround {
    /// The base layers' ground, thrust's outlines built over a box `half`
    /// tiles across either way of the tile `centre`.
    pub fn new(seed: u64, centre: (i32, i32), half: f64, depth: u32) -> Self {
        let (cx, cy) = hex_to_world(centre.0, centre.1);
        StackGround { base: Composite::base(seed), outlines: Outlines::in_box(cx, cy, half, seed), depth, memo: DashMap::new() }
    }

    /// Thrust's relief at a tile.
    fn relief(&self, q: i64, r: i64) -> f64 {
        let (wx, wy) = hex_to_world(q as i32, r as i32);
        self.outlines.at(wx, wy).map_or(0.0, |at| self.outlines.relief_of(&at))
    }

    /// Thrust's relief averaged over cell (i, j) of the level `w` wide, read
    /// at its descendants `depth` levels down, or at its tiles where those
    /// are fewer.
    pub fn thrust_mean(&self, w: i64, i: i64, j: i64, depth: u32) -> f64 {
        let depth = depth.min(w.ilog(3));
        let offs = descendants(w, depth);
        offs.iter().map(|&(dq, dr)| self.relief(i * w + dq, j * w + dr)).sum::<f64>() / offs.len() as f64
    }

    fn cell(&self, w: i64, i: i64, j: i64) -> Read {
        if let Some(hit) = self.memo.get(&(w, i, j)) {
            return hit.clone();
        }
        let (q, r) = (i * w, j * w);
        let offs = samples(w);
        let thrusting = self.base.events.iter().position(|e| e.name() == "thrusting").expect("the base holds thrusting");
        let (mut fields, mut erod) = (0.0, 1.0);
        for &(dq, dr) in &offs {
            let mut own = 0.0;
            let view = self.base.compose((q + dq) as i32, (r + dr) as i32, |layer, out| {
                if layer == thrusting {
                    own = out.map_or(0.0, |o| o.elevation_delta);
                }
            });
            fields += view.elevation - own;
            if (dq, dr) == (0, 0) {
                erod = view.rock.map_or(1.0, |rock| rock.erodibility());
            }
        }
        let (wx, wy) = hex_to_world(q as i32, r as i32);
        let age = self.outlines.at(wx, wy).map_or(0.0, |at| at.plate.age);
        let read = (vec![fields / offs.len() as f64, self.thrust_mean(w, i, j, self.depth)], erod, age);
        self.memo.insert((w, i, j), read.clone());
        read
    }
}

impl Ground for StackGround {
    fn layers(&self) -> usize {
        2
    }

    fn read(&self, w: i64, i: i64, j: i64, out: &mut [f64]) {
        out.copy_from_slice(&self.cell(w, i, j).0);
    }

    fn erodibility(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).1
    }

    fn age(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).2
    }
}

/// How far either way of its site the ladder's top reaches, in tiles: a
/// landmass lies within it (`terrain-generation.md`, "Continents").
pub const TOP_REACH: i64 = 142_884;

/// The ladder over the landmass around a site: the top over [`TOP_REACH`]
/// either way, the levels below over the box of the land joined to the
/// site's top cell, and the finer levels wherever they are read.
pub struct Laddered {
    terrain: Terrain<'static, StackGround>,
}

impl Laddered {
    pub fn new(seed: u64, centre: (i32, i32)) -> Self {
        let half = (TOP_REACH + 3 * 729) as f64;
        // The ground lives as long as the process: the ladder reads it.
        let ground: &'static StackGround = Box::leak(Box::new(StackGround::new(seed, centre, half, 2)));
        let bounds = landmass_bounds(ground, centre);
        Laddered { terrain: Terrain::new_box(ground, &bounds, Params::default()) }
    }

    /// The ladder's tile, where the ladder reaches it.
    pub fn tile(&self, q: i32, r: i32) -> Option<Cell> {
        self.terrain.kept(1, q as i64, r as i64).or_else(|| self.terrain.cells(1, &[(q as i64, r as i64)]).pop().flatten())
    }

    /// The tiles of `coords` computed together.
    pub fn warm(&self, coords: &[(i32, i32)]) {
        let cells: Vec<(i64, i64)> = coords.iter().map(|&(q, r)| (q as i64, r as i64)).collect();
        self.terrain.cells(1, &cells);
    }

    /// The cells of the level `w` wide, where the ladder holds them: each
    /// one's height, and its water's surface where the level is fine enough
    /// to hold water above its cells.
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, Option<f64>)>> {
        if FINE.contains(&w) {
            return self.terrain.cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.water))).collect();
        }
        let mut ladder = self.terrain.coarse();
        let Some(k) = ladder.levels.iter().position(|l| l.w == w) else { return vec![None; cells.len()] };
        let lv = &ladder.levels[k];
        let held: Vec<(i64, i64)> = cells.iter().copied().filter(|&(i, j)| lv.idx(i, j).is_some_and(|c| lv.exists[c])).collect();
        if k > 0 && !held.is_empty() {
            ladder.request(k, &held);
        }
        let lv = &ladder.levels[k];
        cells.iter().map(|&(i, j)| lv.idx(i, j).filter(|&c| lv.exists[c] && lv.built[c]).map(|c| (lv.z[c], None))).collect()
    }
}

/// Each level's bounds: the top over [`TOP_REACH`] either way of the site,
/// and the levels below over the box of the land joined to the site's top
/// cell, and a margin for what their passes read past it.
pub fn landmass_bounds(ground: &StackGround, centre: (i32, i32)) -> Vec<Bounds> {
    let top = 729i64;
    let (ci, cj) = cube_round(centre.0 as f64 / top as f64, centre.1 as f64 / top as f64);
    let h = TOP_REACH / top + 2;
    let top_bounds: Bounds = (top, (ci - h, cj - h), (ci + h + 1, cj + h + 1));
    let alone = Ladder::new_box(ground, &[top_bounds], Params::default());
    let lv = &alone.levels[0];
    let mut land = vec![false; lv.cap()];
    let mut todo: Vec<usize> = lv.idx(ci, cj).into_iter().collect();
    while let Some(c) = todo.pop() {
        if land[c] || !lv.exists[c] || lv.z[c] < 0.0 {
            continue;
        }
        land[c] = true;
        let (i, j) = lv.ij(c);
        for (di, dj) in NB {
            if let Some(d) = lv.idx(i + di, j + dj) {
                todo.push(d);
            }
        }
    }
    let cells: Vec<(i64, i64)> = (0..land.len()).filter(|&c| land[c]).map(|c| lv.ij(c)).collect();
    let (ilo, ihi) = (cells.iter().map(|c| c.0).min().unwrap_or(ci), cells.iter().map(|c| c.0).max().unwrap_or(ci));
    let (jlo, jhi) = (cells.iter().map(|c| c.1).min().unwrap_or(cj), cells.iter().map(|c| c.1).max().unwrap_or(cj));
    let mut bounds = vec![top_bounds];
    for w in [243i64, 81, 27] {
        let m = top / w;
        let pad = 40;
        bounds.push((w, (ilo * m - pad, jlo * m - pad), ((ihi + 1) * m + pad, (jhi + 1) * m + pad)));
    }
    bounds
}
