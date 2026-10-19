//! The stack read as the ladder's ground ([`crate::ladder::Ground`]): one
//! layer, the base layers, plates through the scarps, as they compose. A
//! cell reads the stack at nine points, the centres of its nine children,
//! the points a summary reads its parts at (`common::summary::part_offsets`),
//! and holds their mean: its height, its rock's erodibility, its plate's age
//! and the sky's moisture over it (forest's `sky_moisture`) alike. A tile
//! reads itself. Coarse is coarse: no cell reads finer than its children.
//! The ladder is built on this and never sees a layer's index or output
//! (INV-009).
//!
//! Cell (i, j) of a level `w` tiles wide is centred on tile `(i, j)·w`, so
//! the world's origin is a centre at every level, as a summary's is
//! (`common::summary::center_tile`), and a summary of a level is that
//! level's cell.

use dashmap::DashMap;

use super::plates::Coasts;
use super::thrusting::Outlines;
use super::{forest, lithology, motion, plates, scarp, thickening, thrusting, tilt, Composite, TileOutput, TileView, WorldEvent};
use crate::continents::{self, SiteId};
use crate::ladder::geom::{cube_round, descendants};
use crate::ladder::terrain::{Cell, Terrain};
use crate::ladder::{Bounds, Ground, Ladder, Level, Params};
use crate::hex_to_world;
use crate::lattice::DIRECTIONS;

/// The points a cell of a level `w` wide reads, as tile offsets from its
/// centre: its nine children's centres; a tile's own.
fn samples(w: i64) -> Vec<(i64, i64)> {
    descendants(w, u32::from(w >= 3))
}

/// What a cell read once: the ground's height, the rock's erodibility, the
/// plate's age and the sky's moisture.
type Read = (f64, f64, f64, f64);

/// The base layers as the ladder's one.
pub struct StackGround {
    base: Composite,
    outlines: Outlines,
    coasts: Coasts,
    wind: (f64, f64),
    memo: DashMap<(i64, i64, i64), Read>,
}

impl Composite {
    /// The layers the ladder is built on: plates through the scarps, every
    /// layer that shapes the ground before water cuts it.
    pub fn base(seed: u64) -> Self {
        let mut composite = Self::new(seed);
        composite.add_event(Box::new(plates::PlateEvent::new()));
        composite.add_event(Box::new(tilt::TiltEvent::new()));
        composite.add_event(Box::new(motion::MotionEvent::new()));
        composite.add_event(Box::new(thrusting::ThrustingEvent::new()));
        composite.add_event(Box::new(thickening::ThickeningEvent::new()));
        composite.add_event(Box::new(lithology::LithologyEvent::new()));
        composite.add_event(Box::new(scarp::ScarpEvent::new()));
        composite
    }
}

impl StackGround {
    /// The base layers' ground, thrust's outlines and the coasts built over
    /// a box `half` tiles across either way of the tile `centre`.
    pub fn new(seed: u64, centre: (i32, i32), half: f64) -> Self {
        let (cx, cy) = hex_to_world(centre.0, centre.1);
        StackGround {
            base: Composite::base(seed),
            outlines: Outlines::in_box(cx, cy, half, seed),
            coasts: Coasts::in_box(cx, cy, half, seed),
            wind: forest::wind(seed),
            memo: DashMap::new(),
        }
    }

    fn cell(&self, w: i64, i: i64, j: i64) -> Read {
        if let Some(hit) = self.memo.get(&(w, i, j)) {
            return hit.clone();
        }
        let (q, r) = (i * w, j * w);
        let offs = samples(w);
        let mut sum = [0.0; 4];
        for &(dq, dr) in &offs {
            let (pq, pr) = ((q + dq) as i32, (r + dr) as i32);
            let view = self.base.compose(1, pq, pr, |_, _| {});
            let (wx, wy) = hex_to_world(pq, pr);
            let at = [
                view.elevation,
                view.rock.map_or(1.0, |rock| rock.erodibility()),
                self.outlines.at(wx, wy).map_or(0.0, |at| at.plate.age),
                forest::sky_moisture(wx, wy, self.wind, &self.coasts, &self.outlines),
            ];
            for (s, v) in sum.iter_mut().zip(at) {
                *s += v;
            }
        }
        let n = offs.len() as f64;
        let read = (sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n);
        self.memo.insert((w, i, j), read.clone());
        read
    }
}

impl Ground for StackGround {
    fn layers(&self) -> usize {
        1
    }

    fn read(&self, w: i64, i: i64, j: i64, out: &mut [f64]) {
        out[0] = self.cell(w, i, j).0;
    }

    fn erodibility(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).1
    }

    fn age(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).2
    }

    fn moisture(&self, w: i64, i: i64, j: i64) -> f64 {
        self.cell(w, i, j).3
    }
}

/// How many of the top's cells of sea a site's top holds past every tile
/// its land may reach, the outlet its routing drains to.
pub const SEA_MARGIN: i64 = 2;

/// How far past an area a level below the top reaches, in its own cells:
/// room for the deform cells and rings its passes read, and the reads of
/// the level below it.
pub const AREA_PAD: i64 = 240;

/// The ladder of a continent's site: the top over its land and a margin of
/// sea ([`top_bounds`]), built whole, and every level below over the box of
/// the top's land, each built where it is read.
pub struct Laddered {
    terrain: Terrain<'static, StackGround>,
}

impl Laddered {
    pub fn new(seed: u64, id: SiteId) -> Self {
        Laddered::with(seed, id, Params::default())
    }

    /// The ladder of a site with land under parameters `p`.
    pub fn with(seed: u64, id: SiteId, p: Params) -> Self {
        let top_bounds = top_bounds(seed, id).expect("a site with land");
        let (centre, half) = outlines_box(&top_bounds);
        // The ground lives as long as the process: the ladder reads it.
        let ground: &'static StackGround = Box::leak(Box::new(StackGround::new(seed, centre, half)));
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
    /// one's height, the surface of the water standing over it, and which
    /// of its children stand under water ([`Cell::wet_parts`]).
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, Option<f64>, u16)>> {
        self.terrain.cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.water, c.wet_parts))).collect()
    }
}

/// Every continent's ladder, each built the first time one of its tiles or
/// cells is read: a tile's is the site whose land it may be
/// (`continents::land_site`), a cell's that of the tile at its centre. A
/// tile no site's land reaches is open sea, and no ladder's.
pub struct Ladders {
    seed: u64,
    p: Params,
    sites: DashMap<SiteId, std::sync::Arc<std::sync::OnceLock<Laddered>>>,
    /// Land tiles read that no site's ladder holds.
    landless: std::sync::atomic::AtomicU64,
}

impl Ladders {
    pub fn new(seed: u64, p: Params) -> Self {
        Ladders { seed, p, sites: DashMap::new(), landless: std::sync::atomic::AtomicU64::new(0) }
    }

    pub fn params(&self) -> Params {
        self.p
    }

    /// A land tile no site's ladder holds, counted and logged: a seam, an
    /// error. The log thins to the counts that are powers of two.
    pub fn landless(&self, q: i32, r: i32) {
        let n = self.landless.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n.is_power_of_two() {
            tracing::error!("land tile ({q}, {r}) lies in no site's top: {n} such tiles read");
        }
    }

    /// How many land tiles read lay in no site's top.
    pub fn landless_count(&self) -> u64 {
        self.landless.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The site whose ladder holds the tile at (q, r); none over open sea.
    pub fn site_of(&self, q: i32, r: i32) -> Option<SiteId> {
        let (wx, wy) = hex_to_world(q, r);
        continents::land_site(wx, wy, self.seed)
    }

    /// `f` over a site's ladder, built first where it is not yet.
    pub fn with<R>(&self, id: SiteId, f: impl FnOnce(&Laddered) -> R) -> R {
        let slot = self.sites.entry(id).or_default().clone();
        f(slot.get_or_init(|| Laddered::with(self.seed, id, self.p)))
    }

    /// The ladder's tile at (q, r), where a site's ladder holds it.
    pub fn tile(&self, q: i32, r: i32) -> Option<Cell> {
        self.with(self.site_of(q, r)?, |l| l.tile(q, r))
    }

    /// The tiles of `coords` computed together, a site at a time.
    pub fn warm(&self, coords: &[(i32, i32)]) {
        let mut by: std::collections::HashMap<SiteId, Vec<(i32, i32)>> = std::collections::HashMap::new();
        for &(q, r) in coords {
            if let Some(id) = self.site_of(q, r) {
                by.entry(id).or_default().push((q, r));
            }
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
            if let Some(id) = self.site_of((i * w) as i32, (j * w) as i32) {
                by.entry(id).or_default().push(x);
            }
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

    /// The cell of the level `w` wide centred on tile (q, r), where a site's
    /// ladder holds it, and how steeply that level's ground climbs there, in
    /// z-levels per world unit along x and y: the plane through its six
    /// neighbouring cells' heights, a neighbour the site's ladder does not
    /// hold read as level with it. (q, r) is a centre of the level: a tile
    /// at `w` = 1.
    pub fn ground(&self, w: i64, q: i32, r: i32) -> Option<(Cell, (f64, f64))> {
        let site = self.site_of(q, r)?;
        let (i, j) = (q as i64 / w, r as i64 / w);
        debug_assert!(i * w == q as i64 && j * w == r as i64, "({q}, {r}) is no centre of the level {w} wide");
        let at: Vec<(i64, i64)> = std::iter::once((0, 0)).chain(DIRECTIONS).map(|(di, dj)| (i + di as i64, j + dj as i64)).collect();
        let mut got = self.with(site, |l| l.terrain.cells(w, &at)).into_iter();
        let cell = got.next().flatten()?;
        // Over a hex ring of neighbours `w` apart Σdx² = Σdy² = 3w² and
        // Σdx·dy = 0, so the least squares plane's slope is each sum over
        // 3w².
        let (mut gx, mut gy) = (0.0, 0.0);
        for (&(di, dj), n) in DIRECTIONS.iter().zip(got) {
            let dz = n.map_or(0.0, |n| n.z - cell.z);
            let (dx, dy) = hex_to_world(di * w as i32, dj * w as i32);
            gx += dz * dx;
            gy += dz * dy;
        }
        let spread = 3.0 * (w * w) as f64;
        Some((cell, (gx / spread, gy / spread)))
    }

    /// Each cell's height, the surface of the water standing over it, and
    /// which of its children stand under water ([`Ladders::cells`]).
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, Option<f64>, u16)>> {
        self.cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.water, c.wet_parts))).collect()
    }
}

/// The ladder as a layer of the stack: a read on the level `w` wide
/// ([`TileView::w`]) stands at that level's cell centred on the tile, its
/// height, the water the ladder stands over it and the slope of the
/// level's ground; a tile's read is the tile's. Every layer above stands
/// on the ground the game draws at the level it is read at. A tile no
/// site's ladder holds keeps the ground beneath, open sea; a land tile
/// that is so is a seam, counted ([`Ladders::landless`]).
pub struct LadderEvent {
    ladders: std::sync::Arc<Ladders>,
}

impl LadderEvent {
    pub fn new(ladders: std::sync::Arc<Ladders>) -> Self {
        LadderEvent { ladders }
    }
}

impl WorldEvent for LadderEvent {
    fn name(&self) -> &str { "ladder" }

    /// Nothing is resolved per cell: a read is the ladder's.
    fn scale(&self) -> u32 { 81 }

    fn deform(&self, _scope: &super::CellScope) {}

    fn query(&self, q: i32, r: i32, below: &TileView, _cell: &(dyn std::any::Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        let Some((cell, gradient)) = self.ladders.ground(below.w, q, r) else {
            if below.elevation >= 0.0 {
                self.ladders.landless(q, r);
            }
            return None;
        };
        Some(TileOutput {
            elevation_delta: cell.z - below.elevation,
            gradient: (gradient.0 - below.gradient.0, gradient.1 - below.gradient.1),
            water: cell.water,
            ..TileOutput::default()
        })
    }
}

/// The top's bounds over a site's land: every cell holding a tile
/// `continents::land_site` can give the site, one whose position as the
/// plates are drawn lies within [`continents::LAND_REACH`] of a plate it
/// claimed, so within the warp's reach of that as the tiles stand
/// ([`plates::WARP_REACH`]), and [`SEA_MARGIN`] cells of sea past those,
/// the outlet. None for a site that claimed nothing.
pub fn top_bounds(seed: u64, id: SiteId) -> Option<Bounds> {
    let land = continents::land(id, seed);
    if land.polygons.is_empty() {
        return None;
    }
    let pad = continents::LAND_REACH + plates::WARP_REACH;
    let (x0, y0, x1, y1) = (land.lo.0 - pad, land.lo.1 - pad, land.hi.0 + pad, land.hi.1 + pad);
    // A tile's axial position is linear in the world's, so the box's
    // corners hold its least and greatest.
    let axial = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| (x - y / crate::SQRT_3, 2.0 * y / crate::SQRT_3));
    let top = 729.0;
    let (lo_q, hi_q) = axial.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.0), a.1.max(p.0)));
    let (lo_r, hi_r) = axial.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    // A tile rounds to a cell at most one either way of its quotient's.
    let m = SEA_MARGIN + 1;
    let lo = ((lo_q / top).floor() as i64 - m, (lo_r / top).floor() as i64 - m);
    let hi = ((hi_q / top).ceil() as i64 + m + 1, (hi_r / top).ceil() as i64 + m + 1);
    Some((729, lo, hi))
}

/// The tile at the middle of a top's bounds and how far either way of it,
/// in tiles, its thrust outlines are built: past every cell's tiles.
fn outlines_box(&(w, lo, hi): &Bounds) -> ((i32, i32), f64) {
    let corners = [(lo.0, lo.1), (hi.0, lo.1), (lo.0, hi.1), (hi.0, hi.1)].map(|(i, j)| hex_to_world((i * w) as i32, (j * w) as i32));
    let (x0, x1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.0), a.1.max(c.0)));
    let (y0, y1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.1), a.1.max(c.1)));
    let centre = crate::world_to_hex((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    (centre, (x1 - x0).max(y1 - y0) / 2.0 + w as f64)
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

/// Each level's bounds: the top's, and every level below over the box of
/// the top's land that is the site's (`continents::land_site`), and a
/// margin for what its passes read past it, never past the level above,
/// which holds every cell's parent.
pub fn landmass_bounds(top: &Level, seed: u64, id: SiteId) -> Vec<Bounds> {
    let w = top.w;
    let land: Vec<(i64, i64)> = (0..top.cap())
        .filter(|&c| top.exists[c] && top.z[c] >= 0.0)
        .map(|c| top.ij(c))
        .filter(|&(i, j)| {
            let (wx, wy) = hex_to_world((i * w) as i32, (j * w) as i32);
            continents::land_site(wx, wy, seed) == Some(id)
        })
        .collect();
    let (ci, cj) = (top.lo + top.n as i64 / 2, top.lo_j + top.n_j as i64 / 2);
    let (ilo, ihi) = (land.iter().map(|c| c.0).min().unwrap_or(ci), land.iter().map(|c| c.0).max().unwrap_or(ci));
    let (jlo, jhi) = (land.iter().map(|c| c.1).min().unwrap_or(cj), land.iter().map(|c| c.1).max().unwrap_or(cj));
    let mut bounds = vec![(w, (top.lo, top.lo_j), (top.lo + top.n as i64, top.lo_j + top.n_j as i64))];
    for lw in [243i64, 81, 27, 9, 3, 1] {
        let m = w / lw;
        let pad = 40;
        // A child lies at most one cell back and two on of three times its
        // parent's index, so these are the cells whose parents are all held.
        let (_, plo, phi) = *bounds.last().expect("the top");
        let lo = ((ilo * m - pad).max(3 * plo.0 + 2), (jlo * m - pad).max(3 * plo.1 + 2));
        let hi = (((ihi + 1) * m + pad).min(3 * phi.0 - 3), ((jhi + 1) * m + pad).min(3 * phi.1 - 3));
        bounds.push((lw, lo, hi));
    }
    bounds
}
