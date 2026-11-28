//! The water layer: the stack's ground cut by the water that runs on it,
//! a continent's site at a time, at every level a read may stand on.
//!
//! It reads the ground beneath it ([`CellScope::below`]) at its vertices,
//! a cell's centre and its corners, each the centre of one of its
//! children, so no cell reads finer than its children: the tile there as
//! the layers under it compose it, its height, its rock's erodibility, the
//! plate it stands on and what the sky gives it. It sees no layer's index
//! and calls no layer's code: what the layers beneath offer it rides on
//! the tile (INV-009).
//!
//! A site's water is one thing, its routing a whole landmass's, so the
//! layer's cell is wide enough that a cell and its ring hold any site's
//! land whole ([`SITE_CELL`]): the cell a site stands in publishes its
//! water ([`SiteIndex`]) and every tile of its land finds it in reach.
//!
//! Cell (i, j) of a level `w` tiles wide is centred on tile `(i, j)·w`, so
//! the world's origin is a centre at every level, as a summary's is
//! (`common::summary::center_tile`), and a summary of a level is that
//! level's cell.

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use super::index::{CellId, CellIndex, IndexRegistry, Located};
use super::{lithology, motion, plates, scarp, sky, thickening, thrusting, tilt, Below, CellScope, Composite, TileOutput, TileView, WorldEvent, RING_CLEARANCE};
use crate::continents::{self, SiteId};
use crate::water::geom::cube_round;
use crate::water::terrain::{Cell, Terrain};
use crate::water::{Bounds, Course, Ground, Landmass, Level, Params, Point};
use crate::hex_to_world;
use crate::lattice::DIRECTIONS;

/// The ground beneath the water layer, as the water reads it
/// ([`crate::water::Ground`]).
pub struct StackGround {
    below: Below,
    /// A probe's own stack, kept while its ground is read.
    _own: Option<Composite>,
    /// Every tile read, kept: a coarse level's corner is a finer level's
    /// centre.
    memo: DashMap<(i64, i64), Point>,
}

impl Composite {
    /// The layers the water layer stands on: plates through the scarps,
    /// every layer that shapes the ground before water cuts it.
    pub fn base(seed: u64) -> Self {
        let mut composite = Self::new(seed);
        composite.add_event(Box::new(plates::PlateEvent::new()));
        composite.add_event(Box::new(tilt::TiltEvent::new()));
        composite.add_event(Box::new(motion::MotionEvent::new()));
        composite.add_event(Box::new(thrusting::ThrustingEvent::new()));
        composite.add_event(Box::new(sky::SkyEvent::new()));
        composite.add_event(Box::new(thickening::ThickeningEvent::new()));
        composite.add_event(Box::new(lithology::LithologyEvent::new()));
        composite.add_event(Box::new(scarp::ScarpEvent::new()));
        composite.core_mut().slopes = false;
        composite
    }
}

impl StackGround {
    /// The ground of a stack of the base layers of its own
    /// ([`Composite::base`]): for a probe that builds water's levels
    /// itself.
    pub fn new(seed: u64) -> Self {
        let own = Composite::base(seed);
        StackGround { below: own.ground(), _own: Some(own), memo: DashMap::new() }
    }

    /// The ground beneath the layer whose scope this is.
    fn under(scope: &CellScope) -> Self {
        StackGround { below: scope.below(), _own: None, memo: DashMap::new() }
    }
}

impl Ground for StackGround {
    fn at(&self, q: i64, r: i64) -> Point {
        if let Some(hit) = self.memo.get(&(q, r)) {
            return *hit;
        }
        let view = self.below.tile(q as i32, r as i32);
        // Past every outline lies open ocean.
        let (age, ocean) = view.crust.map_or((0.0, true), |crust| (crust.age, !crust.continental));
        let point = Point {
            height: view.elevation,
            erodibility: view.rock.map_or(1.0, |rock| rock.erodibility()),
            age,
            ocean,
            moisture: view.moisture.expect("the sky stands under the water"),
        };
        self.memo.insert((q, r), point);
        point
    }

    fn height(&self, q: i64, r: i64) -> f64 {
        self.at(q, r).height
    }

    fn ocean(&self, q: i64, r: i64) -> bool {
        self.at(q, r).ocean
    }
}

/// How many of the top's cells of sea a site's top holds past every tile
/// its land may reach, the outlet its routing drains to.
pub const SEA_MARGIN: i64 = 2;

/// How far past an area a level below the top reaches, in its own cells:
/// room for the deform cells and rings its passes read, and the reads of
/// the level below it.
pub const AREA_PAD: i64 = 240;

/// The water of a continent's site: the top over its land and a margin of
/// sea ([`top_bounds`]), built whole the first time it is read, and every
/// level below over the box of the top's land, each built where it is
/// read.
pub struct SiteWater {
    id: SiteId,
    seed: u64,
    p: Params,
    ground: &'static StackGround,
    /// The box its top lies in, in world units.
    bounds: [f64; 4],
    terrain: OnceLock<Terrain<'static, StackGround>>,
}

impl SiteWater {
    /// A site's water on a stack of the base layers of its own: for a
    /// probe.
    pub fn new(seed: u64, id: SiteId) -> Self {
        SiteWater::with(seed, id, Params::default())
    }

    /// A site's water under parameters `p`, on a stack of the base layers
    /// of its own: for a probe.
    pub fn with(seed: u64, id: SiteId, p: Params) -> Self {
        // The ground lives as long as the process: the water reads it.
        SiteWater::on(Box::leak(Box::new(StackGround::new(seed))), seed, id, p).expect("a site with land")
    }

    /// The water of a site on `ground`, none for a site with no land.
    fn on(ground: &'static StackGround, seed: u64, id: SiteId, p: Params) -> Option<Self> {
        let (centre, half) = outlines_box(&top_bounds(seed, id)?);
        let (cx, cy) = hex_to_world(centre.0, centre.1);
        Some(SiteWater { id, seed, p, ground, bounds: [cx - half, cy - half, cx + half, cy + half], terrain: OnceLock::new() })
    }

    /// The site's levels, its top built first where it is not yet.
    pub fn terrain(&self) -> &Terrain<'static, StackGround> {
        self.terrain.get_or_init(|| {
            let top_bounds = top_bounds(self.seed, self.id).expect("a site with land");
            let top = Landmass::top(self.ground, top_bounds, self.p);
            let bounds = landmass_bounds(&top, self.seed, self.id);
            Terrain::from_landmass(Landmass::with_top(self.ground, &bounds, self.p, Some(top)))
        })
    }

    /// The water layer's tile, where the water layer reaches it.
    pub fn tile(&self, q: i32, r: i32) -> Option<Cell> {
        self.terrain().cells(1, &[(q as i64, r as i64)]).pop().flatten()
    }

    /// The tiles of `coords` computed together.
    pub fn warm(&self, coords: &[(i32, i32)]) {
        let cells: Vec<(i64, i64)> = coords.iter().map(|&(q, r)| (q as i64, r as i64)).collect();
        self.terrain().cells(1, &cells);
    }

    /// The cells of the level `w` wide, where the water layer holds them: each
    /// one's height, the surface of the water standing over it, and which
    /// of its children stand under water ([`Cell::wet_parts`]).
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, f64, Option<f64>, u16)>> {
        self.terrain().cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.fine, c.water, c.wet_parts))).collect()
    }
}

impl Located for Arc<SiteWater> {
    fn bounds(&self) -> [f64; 4] {
        self.bounds
    }
}

/// The water of every site standing in each cell of the layer.
#[derive(Default)]
pub struct SiteIndex {
    cells: HashMap<CellId, Vec<Arc<SiteWater>>>,
}

impl CellIndex for SiteIndex {
    type Cell = Vec<Arc<SiteWater>>;
    type Feature = Arc<SiteWater>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
    }

    fn get(&self, cell: CellId) -> Option<&Self::Cell> {
        self.cells.get(&cell)
    }

    fn features(entry: &Self::Cell) -> impl Iterator<Item = &Arc<SiteWater>> {
        entry.iter()
    }
}

/// The sites whose water a cell's tiles may stand on: those the cell and
/// its ring hold. A tile's is the site whose land it may be
/// (`continents::land_site`), a cell's that of the tile at its centre. A
/// tile no site's land reaches is open sea, and no site's.
pub struct Sites {
    seed: u64,
    held: Vec<Arc<SiteWater>>,
}

impl Sites {
    /// The site whose water layer holds the tile at (q, r); none over open sea.
    pub fn site_of(&self, q: i32, r: i32) -> Option<SiteId> {
        let (wx, wy) = hex_to_world(q, r);
        continents::land_site(wx, wy, self.seed)
    }

    /// `f` over a site's water. A site whose land a tile of the cell may
    /// be is held by the cell and its ring, or the layer's cell is too
    /// small for its sites.
    pub fn with<R>(&self, id: SiteId, f: impl FnOnce(&SiteWater) -> R) -> R {
        let site = self.held.iter().find(|site| site.id == id);
        f(site.unwrap_or_else(|| panic!("site {id:?} stands past the ring of a cell holding its land: the water layer's cell is too small")))
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
    /// the water layer of the site of its centre tile.
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
            let got = self.with(id, |l| l.terrain().cells(w, &mine));
            for (x, v) in at.into_iter().zip(got) {
                out[x] = v;
            }
        }
        out
    }

    /// The cell of the level `w` wide centred on tile (q, r), where a site's
    /// water layer holds it, and how steeply that level's ground climbs there, in
    /// z-levels per world unit along x and y: the plane through its six
    /// neighbouring cells' heights, a neighbour the site's water layer does not
    /// hold read as level with it. (q, r) is a centre of the level: a tile
    /// at `w` = 1.
    pub fn ground(&self, w: i64, q: i32, r: i32) -> Option<(Cell, (f64, f64))> {
        let site = self.site_of(q, r)?;
        let (i, j) = (q as i64 / w, r as i64 / w);
        debug_assert!(i * w == q as i64 && j * w == r as i64, "({q}, {r}) is no centre of the level {w} wide");
        let at: Vec<(i64, i64)> = std::iter::once((0, 0)).chain(DIRECTIONS).map(|(di, dj)| (i + di as i64, j + dj as i64)).collect();
        let mut got = self.with(site, |l| l.terrain().cells(w, &at)).into_iter();
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

    /// The rivers leaving each cell of the level `w` wide
    /// ([`Landmass::courses`]), each read from the water layer of the site of its
    /// centre tile; none over open sea.
    pub fn courses(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Vec<Course>> {
        let mut by: std::collections::HashMap<SiteId, Vec<usize>> = std::collections::HashMap::new();
        for (x, &(i, j)) in cells.iter().enumerate() {
            if let Some(id) = self.site_of((i * w) as i32, (j * w) as i32) {
                by.entry(id).or_default().push(x);
            }
        }
        let mut out = vec![Vec::new(); cells.len()];
        for (id, at) in by {
            let mine: Vec<(i64, i64)> = at.iter().map(|&x| cells[x]).collect();
            let got = self.with(id, |l| l.terrain().courses(w, &mine));
            for (x, v) in at.into_iter().zip(got) {
                out[x] = v;
            }
        }
        out
    }

    /// Each cell's height, the surface of the water standing over it, and
    /// which of its children stand under water ([`Waters::cells`]).
    pub fn level(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<(f64, f64, Option<f64>, u16)>> {
        self.cells(w, cells).into_iter().map(|c| c.map(|c| (c.z, c.fine, c.water, c.wet_parts))).collect()
    }
}

/// The width of the layer's cell, in tiles: one ring clears two sites'
/// spacing, further than the box of any site's top reaches from where the
/// site stands, so the cell a site stands in is in reach of every tile of
/// its land. EMPIRICAL: a site claims plates nearer it than any other
/// site, which bounds its land near one spacing, and its top's box is
/// wider than its land; each [`SiteWater`]'s box is checked against the
/// ring as it is made.
pub const SITE_CELL: u32 = (2.0 * continents::SITE_SPACING / RING_CLEARANCE) as u32 + 1;

/// The water layer of the stack: a read on the level `w` wide
/// ([`TileView::w`]) stands at that level's cell centred on the tile, its
/// height, the water standing over it and the slope of the level's
/// ground; a tile's read is the tile's. Every layer above stands on the
/// ground the game draws at the level it is read at. A tile no site's
/// water holds keeps the ground beneath, open sea, with the sea standing
/// over it at zero; a land tile that is so is a seam, counted.
pub struct WaterEvent {
    p: Params,
    /// Land tiles read that no site's water holds.
    landless: AtomicU64,
}

impl WaterEvent {
    pub fn new(p: Params) -> Self {
        WaterEvent { p, landless: AtomicU64::new(0) }
    }

    /// A land tile no site's water holds, counted and logged: a seam, an
    /// error. The log thins to the counts that are powers of two.
    fn landless(&self, q: i32, r: i32) {
        let n = self.landless.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_power_of_two() {
            tracing::error!("land tile ({q}, {r}) lies in no site's top: {n} such tiles read");
        }
    }
}

impl WorldEvent for WaterEvent {
    fn name(&self) -> &str { "water" }

    fn scale(&self) -> u32 { SITE_CELL }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<SiteIndex>();
    }

    /// The water of every site standing in this cell, each on the ground
    /// beneath the layer. Nothing is built of it until a tile is read.
    fn deform(&self, scope: &CellScope) {
        let (lattice, cell, seed) = (scope.lattice(), scope.cell(), scope.seed());
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cy) = hex_to_world(cq, cr);
        let around = (2.0 * lattice.radius as f64 / continents::SITE_SPACING).ceil() as i32 + 1;
        let standing: Vec<SiteId> = crate::tectonic::cells_around(continents::site_cell_for(cx, cy), around)
            .into_iter()
            .filter(|&id| {
                let site = continents::site(id, seed);
                let (q, r) = crate::world_to_hex(site.wx, site.wy);
                lattice.cell_id(q, r) == cell
            })
            .collect();
        let clear = RING_CLEARANCE * lattice.radius as f64;
        let mut ground: Option<&'static StackGround> = None;
        let mut sites = Vec::new();
        for id in standing {
            // The ground lives as long as the process: the water reads it.
            let under = *ground.get_or_insert_with(|| Box::leak(Box::new(StackGround::under(scope))));
            let Some(water) = SiteWater::on(under, seed, id, self.p) else { continue };
            let site = continents::site(id, seed);
            let [x0, y0, x1, y1] = water.bounds;
            let far = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].into_iter().map(|(x, y)| (x - site.wx).hypot(y - site.wy)).fold(0.0, f64::max);
            assert!(far <= clear, "site {id:?} reaches {far:.0} from where it stands and one ring clears {clear:.0}: the water layer's cell is too small");
            sites.push(Arc::new(water));
        }
        scope.publish::<SiteIndex>(sites);
    }

    fn prepare(&self, scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        let held = scope.read::<SiteIndex>().map(|sites| sites.features().cloned().collect()).unwrap_or_default();
        Box::new(Sites { seed: scope.seed(), held })
    }

    fn warm(&self, w: i64, tiles: &[(i32, i32)], cell: &(dyn Any + Send + Sync)) {
        let Some(sites) = cell.downcast_ref::<Sites>() else { return };
        let cells: Vec<(i64, i64)> = tiles.iter().map(|&(q, r)| (q as i64 / w, r as i64 / w)).collect();
        sites.cells(w, &cells);
    }

    fn query(&self, q: i32, r: i32, below: &TileView, cell: &(dyn Any + Send + Sync), _seed: u64) -> Option<TileOutput> {
        let sites = cell.downcast_ref::<Sites>()?;
        let Some((cell, gradient)) = sites.ground(below.w, q, r) else {
            if below.elevation >= 0.0 {
                self.landless(q, r);
                return None;
            }
            return Some(TileOutput { water: Some(0.0), ..TileOutput::default() });
        };
        Some(TileOutput {
            elevation_delta: cell.z - below.elevation,
            fine: Some(cell.fine),
            gradient: (gradient.0 - below.gradient.0, gradient.1 - below.gradient.1),
            water: cell.water,
            fall: cell.fall,
            wet: Some(cell.wet_parts),
            ..TileOutput::default()
        })
    }
}

/// The water of a stack's sites, for the world viewer and the probes,
/// never the game (INV-009): a site's is found where it stands, its cell
/// made ready by a read of the tile there.
#[cfg(feature = "inspect")]
pub struct Waters<'a> {
    stack: &'a Composite,
}

#[cfg(feature = "inspect")]
impl Composite {
    /// The water of this stack's sites, where it has a water layer.
    pub fn waters(&self) -> Option<Waters<'_>> {
        self.with_indexes(|indexes| indexes.layer_of::<SiteIndex>()).map(|_| Waters { stack: self })
    }

    /// The rivers leaving each cell of the level `w` wide
    /// ([`Landmass::courses`]), each read from the water of the site of
    /// its centre tile; none over open sea or in a stack with no water.
    pub fn courses(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Vec<Course>> {
        let Some(waters) = self.waters() else { return vec![Vec::new(); cells.len()] };
        let mut by: HashMap<SiteId, Vec<usize>> = HashMap::new();
        for (x, &(i, j)) in cells.iter().enumerate() {
            if let Some(id) = waters.site_of((i * w) as i32, (j * w) as i32) {
                by.entry(id).or_default().push(x);
            }
        }
        let mut out = vec![Vec::new(); cells.len()];
        for (id, at) in by {
            let mine: Vec<(i64, i64)> = at.iter().map(|&x| cells[x]).collect();
            for (x, v) in at.into_iter().zip(waters.with(id, |site| site.terrain().courses(w, &mine))) {
                out[x] = v;
            }
        }
        out
    }
}

#[cfg(feature = "inspect")]
impl Waters<'_> {
    /// The site whose water holds the tile at (q, r); none over open sea.
    pub fn site_of(&self, q: i32, r: i32) -> Option<SiteId> {
        let (wx, wy) = hex_to_world(q, r);
        continents::land_site(wx, wy, self.stack.seed)
    }

    /// `f` over a site's water.
    pub fn with<R>(&self, id: SiteId, f: impl FnOnce(&SiteWater) -> R) -> R {
        let site = continents::site(id, self.stack.seed);
        let (q, r) = crate::world_to_hex(site.wx, site.wy);
        self.stack.tile_at(q, r);
        let found = self.stack.with_indexes(|indexes| {
            let index = indexes.get::<SiteIndex>().expect("a stack with water");
            index.cells.values().flatten().find(|held| held.id == id).cloned()
        });
        f(&found.expect("a site with land"))
    }

    /// The water's tile at (q, r), where a site's water holds it.
    pub fn tile(&self, q: i32, r: i32) -> Option<Cell> {
        self.with(self.site_of(q, r)?, |site| site.tile(q, r))
    }

    /// The cells of the level `w` wide, each read from the water of the
    /// site of its centre tile.
    pub fn cells(&self, w: i64, cells: &[(i64, i64)]) -> Vec<Option<Cell>> {
        let mut by: HashMap<SiteId, Vec<usize>> = HashMap::new();
        for (x, &(i, j)) in cells.iter().enumerate() {
            if let Some(id) = self.site_of((i * w) as i32, (j * w) as i32) {
                by.entry(id).or_default().push(x);
            }
        }
        let mut out = vec![None; cells.len()];
        for (id, at) in by {
            let mine: Vec<(i64, i64)> = at.iter().map(|&x| cells[x]).collect();
            for (x, v) in at.into_iter().zip(self.with(id, |site| site.terrain().cells(w, &mine))) {
                out[x] = v;
            }
        }
        out
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
        .filter(|&c| top.exists[c] && top.z.get(c) >= 0.0)
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
