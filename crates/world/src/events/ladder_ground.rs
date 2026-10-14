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
use crate::continents::{self, SiteId};
use crate::ladder::geom::{cube_round, descendants};
use crate::ladder::terrain::{Cell, Terrain};
use crate::ladder::{Bounds, Ground, Ladder, Level, Params};
use crate::{hex_to_world, world_to_hex};

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

/// How far past an area a level below the top reaches, in its own cells:
/// room for the deform cells and rings its passes read, and the reads of
/// the level below it.
pub const AREA_PAD: i64 = 240;

/// The ladder of a continent's site: the top over [`TOP_REACH`] either way
/// of the site, built whole, and every level below over the box of the land
/// the site claims, each built where it is read.
pub struct Laddered {
    terrain: Terrain<'static, StackGround>,
}

impl Laddered {
    pub fn new(seed: u64, id: SiteId) -> Self {
        Laddered::with(seed, id, Params::default())
    }

    /// The ladder of a site under parameters `p`.
    pub fn with(seed: u64, id: SiteId, p: Params) -> Self {
        let site = continents::site(id, seed);
        let centre = world_to_hex(site.wx, site.wy);
        let half = (TOP_REACH + 3 * 729) as f64;
        // The ground lives as long as the process: the ladder reads it.
        let ground: &'static StackGround = Box::leak(Box::new(StackGround::new(seed, centre, half, 2)));
        let top_bounds = top_bounds(centre);
        let top = Ladder::top(ground, top_bounds, p);
        let bounds = landmass_bounds(&top, seed, id);
        Laddered { terrain: Terrain::from_ladder(Ladder::with_top(ground, &bounds, p, Some(top))) }
    }

    /// The ladder behind its lock, for a reader that walks its levels.
    pub fn terrain(&self) -> &Terrain<'static, StackGround> {
        &self.terrain
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
    /// one's height, and the surface of the water standing over it.
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, Option<f64>)>> {
        self.terrain.cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.water))).collect()
    }
}

/// Every continent's ladder, each built the first time one of its tiles or
/// cells is read: a tile's is its nearest site's
/// (`continents::nearest_site`), a cell's that of the tile at its centre.
pub struct Ladders {
    seed: u64,
    p: Params,
    sites: DashMap<SiteId, std::sync::Arc<std::sync::OnceLock<Laddered>>>,
}

impl Ladders {
    pub fn new(seed: u64, p: Params) -> Self {
        Ladders { seed, p, sites: DashMap::new() }
    }

    pub fn params(&self) -> Params {
        self.p
    }

    /// The site whose ladder holds the tile at (q, r).
    pub fn site_of(&self, q: i32, r: i32) -> SiteId {
        let (wx, wy) = hex_to_world(q, r);
        continents::nearest_site(wx, wy, self.seed)
    }

    /// `f` over a site's ladder, built first where it is not yet.
    pub fn with<R>(&self, id: SiteId, f: impl FnOnce(&Laddered) -> R) -> R {
        let slot = self.sites.entry(id).or_default().clone();
        f(slot.get_or_init(|| Laddered::with(self.seed, id, self.p)))
    }

    /// The ladder's tile at (q, r), where its site's ladder reaches it.
    pub fn tile(&self, q: i32, r: i32) -> Option<Cell> {
        self.with(self.site_of(q, r), |l| l.tile(q, r))
    }

    /// The tiles of `coords` computed together, a site at a time.
    pub fn warm(&self, coords: &[(i32, i32)]) {
        let mut by: std::collections::HashMap<SiteId, Vec<(i32, i32)>> = std::collections::HashMap::new();
        for &(q, r) in coords {
            by.entry(self.site_of(q, r)).or_default().push((q, r));
        }
        for (id, tiles) in by {
            self.with(id, |l| l.warm(&tiles));
        }
    }

    /// The cells of the level `w` wide, built where not yet, each read from
    /// the ladder of the site of its centre tile.
    pub fn cells(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<Cell>> {
        let mut by: std::collections::HashMap<SiteId, Vec<usize>> = std::collections::HashMap::new();
        for (x, &(i, j)) in cells.iter().enumerate() {
            by.entry(self.site_of((i * w) as i32, (j * w) as i32)).or_default().push(x);
        }
        let mut out = vec![None; cells.len()];
        for (id, at) in by {
            let mine: Vec<(i64, i64)> = at.iter().map(|&x| cells[x]).collect();
            let got = self.with(id, |l| l.terrain.cells(w, &mine));
            for (x, v) in at.into_iter().zip(got) {
                out[x] = v;
            }
        }
        out
    }

    /// Each cell's height and the surface of the water standing over it
    /// ([`Ladders::cells`]).
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, Option<f64>)>> {
        self.cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.water))).collect()
    }
}

/// The top's bounds: [`TOP_REACH`] either way of a site.
pub fn top_bounds(centre: (i32, i32)) -> Bounds {
    let top = 729i64;
    let (ci, cj) = cube_round(centre.0 as f64 / top as f64, centre.1 as f64 / top as f64);
    let h = TOP_REACH / top + 2;
    (top, (ci - h, cj - h), (ci + h + 1, cj + h + 1))
}

/// The bounds of levels `widths` wide over an area `radius` tiles either
/// way of `centre`, each [`AREA_PAD`] of its cells past it.
pub fn area_bounds(centre: (i32, i32), radius: i64, widths: &[i64]) -> Vec<Bounds> {
    widths
        .iter()
        .map(|&w| {
            let (ci, cj) = cube_round(centre.0 as f64 / w as f64, centre.1 as f64 / w as f64);
            let h = radius / w + AREA_PAD;
            (w, (ci - h, cj - h), (ci + h + 1, cj + h + 1))
        })
        .collect()
}

/// Each level's bounds: the top's, over [`TOP_REACH`] either way of the
/// site, and every level below over the box of the top's land the site
/// claims, nearest it of every site, and a margin for what its passes read
/// past it.
pub fn landmass_bounds(top: &Level, seed: u64, id: SiteId) -> Vec<Bounds> {
    let w = top.w;
    let land: Vec<(i64, i64)> = (0..top.cap())
        .filter(|&c| top.exists[c] && top.z[c] >= 0.0)
        .map(|c| top.ij(c))
        .filter(|&(i, j)| {
            let (wx, wy) = hex_to_world((i * w) as i32, (j * w) as i32);
            continents::nearest_site(wx, wy, seed) == id
        })
        .collect();
    let (ci, cj) = (top.lo + top.n as i64 / 2, top.lo_j + top.n_j as i64 / 2);
    let (ilo, ihi) = (land.iter().map(|c| c.0).min().unwrap_or(ci), land.iter().map(|c| c.0).max().unwrap_or(ci));
    let (jlo, jhi) = (land.iter().map(|c| c.1).min().unwrap_or(cj), land.iter().map(|c| c.1).max().unwrap_or(cj));
    let mut bounds = vec![(w, (top.lo, top.lo_j), (top.lo + top.n as i64, top.lo_j + top.n_j as i64))];
    for lw in [243i64, 81, 27, 9, 3, 1] {
        let m = w / lw;
        let pad = 40;
        bounds.push((lw, (ilo * m - pad, jlo * m - pad), ((ihi + 1) * m + pad, (jhi + 1) * m + pad)));
    }
    bounds
}
