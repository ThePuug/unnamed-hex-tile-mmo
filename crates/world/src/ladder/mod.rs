//! The terrain ladder (`design/ladder.md`): the ground built on hex levels,
//! each three times finer than the one above, down to the tiles, a level
//! answered on request one cell at a time.
//!
//! A request names a level and its cells. Each pass at a level deforms in
//! cells of its own size, read with one ring: the pass declares how far its
//! work reads, and its deform cell is the smallest ladder width whose ring
//! clears that ([`geom::ring_clearance`]). A deform reads its cell and ring
//! at its level, the passes before it there, and the level above under the
//! same footprint and one ring of the level above's cells. It writes its
//! own cell only. Deform cascades upward to the top, built once over the whole
//! domain, its edge and the sea the outlet, and kept.
//!
//! Below the top a level runs its passes in order:
//!
//! - **ground**: the ground read at each vertex, a cell's centre and its
//!   corners A and B ([`Ground::at`]): the level's dry ground. Met by the
//!   valleys of every coarser level's rivers there: its carved ground.
//! - **route**: every vertex's receiver downhill on the carved ground, its
//!   catchment, and the level's troughs: the level above's rivers run on
//!   from its nodes, and troughs born at the level ([`route`]). A
//!   catchment is the land a vertex drains, in tiles, and everything that
//!   cuts reads it: floods do the cutting, and a dry country's floods run
//!   as high as a wet one's. Its flow is the rain that land gathers, each
//!   tile adding the sky's moisture over it ([`Point::moisture`]), and
//!   only the water drawn reads it, a dry country's river narrower in the
//!   same valley. Every vertex's graded profile is the height its channel
//!   stands at graded down its course: its outlet's level, the sea's or a
//!   lake's, or a node's as the level above graded it, with the channel's
//!   graded slope integrated up from there ([`graded_slope`]). A trough's
//!   floor is the level's own dry ground cut toward that profile as far as
//!   the plate's age has carried erosion ([`drained`]), and never above the
//!   floors draining into it.
//! - **water**: one target surface, the valley drainage's law cuts: a floor
//!   along each trough, walls rising from it at water's slope, steeper in
//!   hard rock, out to where they meet the carved ground.
//!
//! Water comes last and reads the level's troughs, never the other way:
//! routing reads only the ground.
//!
//! Every level below the top runs all three, down to the tiles. A level
//! holds water where its own beds cross a cell: a level over the bed's
//! floor, a level more for each two tiles of its half width past the first
//! ([`water::depth`]). [`terrain::Terrain`] holds the ladder for readers on
//! many threads.
//!
//! **A river is handed down as where it is and what it carries, never its
//! course.** The level above hands down its trough vertices, each with its
//! catchment, its graded profile and the level it was born at; each is one of the level's
//! vertices, since levels nest, and the level's own water runs the river on
//! from there, downhill, wherever its ground leads; a river starts once,
//! where it first comes into reach. Where troughs are born, routing picks
//! between the two neighbours bracketing the downslope by chance weighted
//! by the slope, so on even ground they wander and join as drainage does;
//! below, a river follows its valley. Water never climbs ([`route`]).
//!
//! **A level carves into what the levels above carved.** It reads their
//! rivers' valleys at its own vertices: routing reads the carved ground,
//! so water gathers into the valleys above; floors read the dry ground, so no floor deepens
//! by what the level above already cut. A level's height at each vertex is
//! its own troughs' valleys within its ring met by its carved ground there,
//! rounded to whole steps: a valley is an absolute height, so a river the
//! level above cut to its floor is not cut again. Its water fills that
//! carve. Nothing makes it walkable: a step past walkable is a face.
//!
//! **Nothing is averaged.** Every vertex of a level coarser than the tiles
//! is a child's centre, a whole tile, and reads the ground there; a tile's
//! corners lie between tiles and take the mean of the three around each.
//! **A level hands its rivers down as shapes, never as heights at its
//! vertices:** a finer level reads every coarser level's beds, each out to
//! that level's ring, and draws their valleys at its own vertices and its
//! own width, so a bend or a shelf smaller than a coarse level's spacing
//! reaches every level fine enough to show it.
//! The corner rule, faces as walls, is the drawing's alone: meshes, physics
//! and summaries draw a level's cells by it (`common::corner`) from their
//! centres, and nothing here reads it.
//!
//! Every value is a function of its position: what a level keeps is filled
//! as requests reach it, and never read in place of a recomputation that
//! would differ.

pub mod geom;
pub mod mask;
pub mod paged;
pub mod terrain;
mod route;
mod top;
mod water;

use std::collections::HashSet;
use std::time::Instant;

use geom::*;
use mask::{Exists, Mask};
use paged::{Paged, PAGE};

/// How the water layer cuts.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The flow at which a trough's bed is a trunk's, in tiles of full rain.
    pub catchment_full: f64,
    /// The catchment at which a channel begins to cut, and a trough is born,
    /// in the softest rock on a slope of [`HEAD_SLOPE`], in tiles: where
    /// every channel's graded profile falls from ([`graded_slope`]).
    pub cut_from: f64,
    /// A wall's slope in the softest rock, in steps per tile: walkable.
    pub wall: f64,
    /// How far water reads lines from a cell, in cells of the level.
    pub ring: f64,
    /// Whether water cuts: off, the ground is the layers' alone, the
    /// counterfactual that tells water's faces from the rest.
    pub water: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params { catchment_full: 400_000.0, cut_from: 4_000.0, wall: 1.0, ring: RING, water: true }
    }
}

/// What the ladder reads of the ground at one tile.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    /// The ground's height, in steps.
    pub height: f64,
    /// The erodibility of the rock.
    pub erodibility: f64,
    /// The age of the plate, 0 to 1.
    pub age: f64,
    /// The sky's moisture: the rain each tile adds to the water draining
    /// from it, one the shore's lowland's.
    pub moisture: f64,
}

/// What the ladder is seeded with: the ground at any tile.
pub trait Ground: Sync {
    fn at(&self, q: i64, r: i64) -> Point;
}

/// How far water reads lines from a cell, in cells of the level: what one
/// ring of its deform cell, 27 of the level's cells wide, clears.
pub const RING: f64 = 15.588_457_268_119_896;
/// The least erodible rock, whose walls stand at [`WALL_HARDEST`].
pub const HARDEST: f64 = 0.3;
/// The steepest a wall stands, in the hardest rock, in steps per tile.
pub const WALL_HARDEST: f64 = 3.0;
/// The slope at which a channel begins at [`cut_head`]'s catchment, in
/// steps per tile: a walkable wall in the softest rock.
pub const HEAD_SLOPE: f64 = 1.0;
/// How far upstream a trough born at a level may gather, in its cells.
pub const ROUTE_REACH: f64 = 11.0;

/// The stages a level's work is timed by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// Each layer's value read from the ground.
    Read = 0,
    /// The prediction and the layers' changes.
    Supply = 1,
    Route = 2,
    Water = 3,
    /// Heights rounded.
    Build = 4,
    /// The prediction surface's corners.
    Corners = 5,
}

pub const STAGES: [&str; 6] = ["read", "supply", "route", "water", "build", "corners"];

/// The passes that deform in cells of their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pass {
    Ground = 0,
    Water = 1,
    Route = 2,
}

const PASSES: [Pass; 3] = [Pass::Ground, Pass::Water, Pass::Route];

/// The catchment at which a trough is born at a level, on rock of
/// `erodibility` falling `slope` steps per tile: where a channel begins
/// there, [`cut_head`] at [`HEAD_SLOPE`] and growing as the square of how
/// much flatter the ground is, since a channel begins where catchment ×
/// slope² reaches a threshold, but no flatter than [`FLAT_SLOPE`]; and
/// never less than two of the level's cells, the narrowest trough a level
/// resolves.
pub fn head_of(w: i64, erodibility: f64, slope: f64, p: &Params) -> f64 {
    (cut_head(erodibility, p) * (HEAD_SLOPE / slope.max(FLAT_SLOPE)).powi(2)).max(2.0 * (w * w) as f64)
}

/// The slope below which a channel begins at the catchment it would at
/// this one, in steps per tile, a gradient of about a tenth: on flatter
/// ground water gathers until the ground is saturated, and a channel
/// begins where enough land drains, whatever the slope.
pub const FLAT_SLOPE: f64 = 0.2;

/// The catchment at which a channel begins to cut on rock of
/// `erodibility`: a hard rock needs proportionally more water.
pub fn cut_head(erodibility: f64, p: &Params) -> f64 {
    p.cut_from / erodibility.max(0.05)
}

/// How far from a cell a pass's work there reads, in tiles.
pub fn reach(pass: Pass, w: i64) -> f64 {
    w as f64
        * match pass {
            Pass::Ground => 0.0,
            Pass::Water => RING,
            Pass::Route => ROUTE_REACH,
        }
}

/// The smallest ladder width whose one ring clears what a pass reads at a
/// level `w` tiles wide, water reading as far as `ring` cells.
pub fn cell_size(pass: Pass, w: i64, ring: f64) -> i64 {
    let reach = if pass == Pass::Water { ring * w as f64 } else { reach(pass, w) };
    let mut cell = w;
    while ring_clearance(cell) < reach - 1e-9 {
        cell *= 3;
    }
    cell
}

/// How steeply a channel carrying `catchment` tiles falls at grade on rock
/// of `erodibility`, in steps per tile: the channel head's law, catchment
/// × slope² at [`cut_head`], held along the whole course, so a channel
/// falls at [`HEAD_SLOPE`] where it begins and flattens as the square root
/// of the water it gathers, steeper in harder rock (stream power at steady
/// state, a concavity of one half).
pub fn graded_slope(catchment: f64, erodibility: f64, p: &Params) -> f64 {
    let head = cut_head(erodibility, p);
    HEAD_SLOPE * (head / catchment.max(head)).sqrt()
}

/// Drainage's floor under dry ground at `elevation`: cut toward its
/// river's graded profile `graded` as far as its plate's age has carried
/// erosion, never above the ground, and never deeper than a wall at the
/// rock's slope climbs over `room` tiles, the distance to the divide of the
/// basin it drains. Where the profile stands above the ground the river is
/// at grade and cuts nothing: the lowland's. A profile not known cuts
/// nothing.
pub fn drained(elevation: f64, graded: f64, age: f64, erodibility: f64, room: f64, p: &Params) -> f64 {
    let cut = (elevation - graded).max(0.0) * crate::tectonic::aged(age);
    elevation - cut.min(wall_slope(erodibility, p.wall) * room)
}

/// Water's wall slope in rock of an erodibility: `wall` in the softest,
/// rising to [`WALL_HARDEST`] in the hardest.
pub fn wall_slope(erodibility: f64, wall: f64) -> f64 {
    let hard = ((1.0 - erodibility) / (1.0 - HARDEST)).clamp(0.0, 1.0);
    wall + (WALL_HARDEST - wall) * hard
}

/// One level: its cells over a square of indices, and everything it keeps.
pub struct Level {
    pub w: i64,
    /// The first cell along i and how many, and along j; held in pages of
    /// 64 by 64 (`idx`).
    pub lo: i64,
    pub n: usize,
    pub lo_j: i64,
    pub n_j: usize,
    pub exists: Exists,
    /// The rock's erodibility and the plate's age at each cell's centre.
    pub erod: Paged<f64>,
    pub age: Paged<f64>,
    /// The rain each of a cell's tiles adds to the flow, the sky's moisture
    /// at its centre.
    pub rain: Paged<f64>,
    // Each vertex: a cell's centre, its corner A and its corner B.
    /// The dry ground at each vertex, NaN until read: the ground's height
    /// there ([`Ground::at`]). A tile's corners lie between tiles, and take
    /// the mean of the three tiles sharing each.
    pub ground: Paged<f64>,
    /// The carved ground at each vertex: its ground met by the valleys of
    /// every coarser level's rivers there.
    pub carved: Paged<f64>,
    /// The lowest the carved ground falls within half a step of each
    /// vertex, a step the distance between the level's points: its ground
    /// met by every coarser level's valleys at their nearest there. What
    /// routing reads, so a valley narrower than the level's steps runs on
    /// as a course, never a row of hollows.
    pub sunk: Paged<f64>,
    /// The ground beside the beds at each vertex: its ground met by every
    /// coarser level's valleys without their channels, their plains and
    /// walls alone. What its rivers' floors are cut from, so a river
    /// crossing a plain a coarser river planed cuts into that plain, and a
    /// river in its own coarser channel cuts no deeper than that river.
    pub banks: Paged<f64>,
    /// The level's carve at each vertex: its valleys, each at its lowest
    /// within half a step, met by its sunk ground, less the ground; never
    /// above the sunk ground.
    pub cut: Paged<f64>,
    /// How far each cell lies from the river governing its valley, in its
    /// cells; infinite where none reaches it.
    pub wdist: Paged<f64>,
    /// The width of the level the river governing each cell's valley was
    /// born at, and of the level it was read from; -1 where none does.
    pub wborn: Paged<i64>,
    pub wsrc: Paged<i64>,
    /// The river vertex whose valley governs each cell, as an index of the
    /// level it was read from; -1 where none does. And that river's floor
    /// there.
    pub wgov: Paged<i64>,
    pub wgfloor: Paged<f64>,
    /// The river vertex whose bed the water over each cell stands in, as an
    /// index of the level; -1 where none does, as over a lake.
    pub wwet: Paged<i64>,
    /// The surface of the water standing over each cell, in levels, where
    /// one of the level's beds crosses it and it stands above the cell; NaN
    /// where none. And how deep that bed's water stands over its floor.
    pub water: Paged<f64>,
    pub wdepth: Paged<f64>,
    /// Which of each cell's nine children ([`CHILD_OFFSETS`]) stand under
    /// the level's water above the sea, a bit to each in that order: under
    /// a bed covering the child's centre at least as wide as the child. A
    /// level's own rivers mark them, so the finer level is never built for
    /// them.
    pub wparts: Paged<u16>,
    /// Each vertex's height, its ground and its cut rounded to whole steps,
    /// and its dry height, its ground rounded: what the level below is
    /// handed. A cell's own is its centre's. A tile's corners have none.
    pub z: Paged<f64>,
    pub zd: Paged<f64>,
    /// The width of the level a trough at each vertex was born at; -1 off
    /// the troughs.
    pub born: Paged<i64>,
    pub rec: Paged<i64>,
    pub trough: Paged<bool>,
    /// The land each vertex drains, in tiles: what cuts.
    pub acc: Paged<f64>,
    /// Each vertex's graded profile: the level its water ends at, with its
    /// channel's graded slope ([`graded_slope`]) integrated up its course
    /// from there, read from no ground. What its floor cuts toward, and
    /// what a river hands down at its nodes with its catchment.
    pub graded: Paged<f64>,
    /// How far each vertex's hillslopes run to the nearer divide, in
    /// tiles: what a floor may widen into.
    pub room: Paged<f64>,
    /// The surface of the lake routing keeps over each vertex, its outlet
    /// cut as far as the plate's age has carried erosion, in levels; NaN
    /// where none stands. What the level draws as a lake.
    pub lake: Paged<f64>,
    /// The rain that land gathers, in tiles of full rain: what the water
    /// drawn carries.
    pub flow: Paged<f64>,
    pub floor: Paged<f64>,
    pub supplied: Paged<bool>,
    pub watered: Paged<bool>,
    pub built: Paged<bool>,
    pub routed: Paged<bool>,
    /// Whether each cell's six corners are final: built, its own two and
    /// the four its neighbours own, where those exist.
    pub finished: Paged<bool>,
    /// How many levels up each pass's deform cell lies.
    pub deform: [u32; 3],
    /// Deforms run by each pass, and cells built.
    pub cost: [u64; 5],
}

const NONE: (i64, i64) = (i64::MIN, i64::MIN);

impl Level {
    /// A level of cells `lo.0..hi.0` along i and `lo.1..hi.1` along j.
    fn new(w: i64, lo: (i64, i64), hi: (i64, i64), top: bool, ring: f64) -> Self {
        let (n, n_j) = ((hi.0 - lo.0) as usize, (hi.1 - lo.1) as usize);
        let cells = Level::space(n, n_j);
        let nan = |k: usize| Paged::new(f64::NAN, k);
        let deform = PASSES.map(|pass| if top { 0 } else { (cell_size(pass, w, ring) / w).ilog(3) });
        Level {
            w,
            lo: lo.0,
            n,
            lo_j: lo.1,
            n_j,
            exists: Exists { n, n_j },
            erod: nan(cells),
            age: nan(cells),
            rain: nan(cells),
            ground: nan(3 * cells),
            carved: nan(3 * cells),
            sunk: nan(3 * cells),
            banks: nan(3 * cells),
            cut: nan(3 * cells),
            wdist: nan(cells),
            wborn: Paged::new(-1, cells),
            wsrc: Paged::new(-1, cells),
            wgov: Paged::new(-1, cells),
            wgfloor: nan(cells),
            wwet: Paged::new(-1, cells),
            water: nan(cells),
            wdepth: nan(cells),
            wparts: Paged::new(0, cells),
            z: nan(3 * cells),
            zd: nan(3 * cells),
            born: Paged::new(-1, 3 * cells),
            rec: Paged::new(-1, 3 * cells),
            trough: Paged::new(false, 3 * cells),
            acc: Paged::new(0.0, 3 * cells),
            graded: nan(3 * cells),
            room: nan(3 * cells),
            lake: nan(3 * cells),
            flow: Paged::new(0.0, 3 * cells),
            floor: nan(3 * cells),
            supplied: Paged::new(false, cells),
            watered: Paged::new(false, cells),
            built: Paged::new(false, cells),
            routed: Paged::new(false, cells),
            finished: Paged::new(false, cells),
            deform,
            cost: [0; 5],
        }
    }

    /// The size of the index space of a level `n` by `n_j` cells: whole
    /// pages of 64 by 64 cells.
    pub fn space(n: usize, n_j: usize) -> usize {
        n.div_ceil(64) * n_j.div_ceil(64) * PAGE
    }

    /// The number of cell indices, the stride between vertex kinds.
    pub fn cap(&self) -> usize {
        Level::space(self.n, self.n_j)
    }

    /// A cell's index, where the level holds it: its page of 64 by 64
    /// cells, then its place on the page.
    pub fn idx(&self, i: i64, j: i64) -> Option<usize> {
        let (a, b) = (i - self.lo, j - self.lo_j);
        if a < 0 || b < 0 || a as usize >= self.n || b as usize >= self.n_j {
            return None;
        }
        let (a, b) = (a as usize, b as usize);
        let side = self.n_j.div_ceil(64);
        Some(((a >> 6) * side + (b >> 6)) * PAGE + ((a & 63) << 6) + (b & 63))
    }

    /// A cell's indices from its index; past the level's side for the
    /// padding of its last pages.
    pub fn ij(&self, c: usize) -> (i64, i64) {
        Level::cell_at(self.n_j, (self.lo, self.lo_j), c)
    }

    /// The cell at index `c` of a level `n_j` cells along j from `lo`.
    fn cell_at(n_j: usize, lo: (i64, i64), c: usize) -> (i64, i64) {
        let side = n_j.div_ceil(64);
        let (page, at) = (c / PAGE, c % PAGE);
        let a = (page / side) * 64 + (at >> 6);
        let b = (page % side) * 64 + (at & 63);
        (lo.0 + a as i64, lo.1 + b as i64)
    }

    /// A vertex's index, where the level holds its cell.
    pub fn vid(&self, t: u8, i: i64, j: i64) -> Option<usize> {
        self.idx(i, j).map(|c| t as usize * self.cap() + c)
    }

    /// A vertex's kind and cell from its index.
    pub fn vertex(&self, v: usize) -> (u8, i64, i64) {
        let nn = self.cap();
        let (i, j) = self.ij(v % nn);
        ((v / nn) as u8, i, j)
    }

    /// The cell holding a vertex at a position in thirds of a tile.
    pub fn vid_at(&self, q3: i64, r3: i64) -> Option<usize> {
        let (t, i, j) = vertex_at(q3, r3, self.w);
        self.vid(t, i, j)
    }

    fn height(&self, i: i64, j: i64, of: &Paged<f64>, done: &Paged<bool>) -> f64 {
        match self.idx(i, j) {
            Some(c) if self.exists[c] && done[c] => of[c],
            _ => f64::NAN,
        }
    }

    /// A cell's deform cell of a pass; none for a cell the level does not
    /// hold.
    pub fn block(&self, pass: Pass, c: usize) -> (i64, i64) {
        if !self.exists[c] {
            return NONE;
        }
        let (i, j) = self.ij(c);
        ancestor(i, j, self.deform[pass as usize])
    }

    /// The cells of a deform cell of a pass the level holds: its
    /// descendants down to the level.
    fn block_cells(&self, pass: Pass, (a, b): (i64, i64)) -> Vec<usize> {
        let mut cells = vec![(a, b)];
        for _ in 0..self.deform[pass as usize] {
            cells = cells.iter().flat_map(|&(x, y)| CHILD_OFFSETS.iter().map(move |&(oi, oj)| (3 * x + oi, 3 * y + oj))).collect();
        }
        cells.into_iter().filter_map(|(i, j)| self.idx(i, j)).filter(|&c| self.exists[c]).collect()
    }

    /// The pending deform cells of a pass over a mask.
    fn pending(&self, pass: Pass, mask: &Mask) -> Vec<(i64, i64)> {
        let mut s: Vec<(i64, i64)> = mask.iter().map(|c| self.block(pass, c)).collect();
        s.sort_unstable();
        s.dedup();
        s.retain(|&b| b != NONE);
        s
    }

    /// Cells whose deform cell of a pass is one of `pend` or in its ring.
    fn footprint(&self, pass: Pass, pend: &[(i64, i64)]) -> Mask {
        let mut keys = HashSet::new();
        for &(a, b) in pend {
            keys.insert((a, b));
            for (da, db) in NB {
                keys.insert((a + da, b + db));
            }
        }
        Mask::new(keys.into_iter().flat_map(|key| self.block_cells(pass, key)))
    }

    /// For each pending deform cell of a pass, its own cells and its
    /// footprint's.
    fn block_lists(&self, pass: Pass, pend: &[(i64, i64)]) -> Vec<(Vec<usize>, Vec<usize>)> {
        pend.iter()
            .map(|&(a, b)| {
                let own = self.block_cells(pass, (a, b));
                let mut fp = own.clone();
                for (da, db) in NB {
                    fp.extend(self.block_cells(pass, (a + da, b + db)));
                }
                (own, fp)
            })
            .collect()
    }

    /// Grows a mask by `k` rings of the level's cells.
    fn dilate(&self, mask: &Mask, k: usize) -> Mask {
        let mut all: mask::Cells = mask.iter().collect();
        let mut ring: Vec<usize> = mask.iter().collect();
        for _ in 0..k {
            let mut next = Vec::new();
            for c in ring {
                let (i, j) = self.ij(c);
                for (di, dj) in NB {
                    if let Some(d) = self.idx(i + di, j + dj) {
                        if all.insert(d) {
                            next.push(d);
                        }
                    }
                }
            }
            ring = next;
        }
        Mask::new(all)
    }

    /// The mean of a cell field over the cells sharing a cell's own corners
    /// A and B; each ready where every cell there that exists holds its
    /// value.
    fn corner_means(&self, i: i64, j: i64, of: &Paged<f64>, done: &Paged<bool>) -> (f64, f64, bool, bool) {
        let h = |a: i64, b: i64| self.height(a, b, of, done);
        let present = |a: i64, b: i64| self.idx(a, b).is_some_and(|c| self.exists[c]);
        let ready = |cells: [(i64, i64); 3]| cells.iter().all(|&(x, y)| !present(x, y) || !h(x, y).is_nan());
        let mean = |cells: [(i64, i64); 3]| {
            let v: Vec<f64> = cells.iter().map(|&(x, y)| h(x, y)).filter(|v| !v.is_nan()).collect();
            v.iter().sum::<f64>() / v.len() as f64
        };
        let (ca, cb) = ([(i, j), (i + 1, j), (i, j + 1)], [(i, j), (i + 1, j - 1), (i + 1, j)]);
        (mean(ca), mean(cb), ready(ca), ready(cb))
    }

    /// The vertices of cell `c` the level holds values at: its centre and
    /// its corners A and B, but a tile's corners lie between tiles and are
    /// its neighbours' as much as its own.
    fn own_vertices(&self, c: usize) -> impl Iterator<Item = (u8, usize)> {
        let nn = self.cap();
        (0..if self.w >= 3 { 3u8 } else { 1 }).map(move |t| (t, t as usize * nn + c))
    }
}

/// A level's width, and its cells' first index and one past its last along
/// i and along j.
pub type Bounds = (i64, (i64, i64), (i64, i64));

/// The ladder: the top, built once, and the levels below on request.
pub struct Ladder<'g, G: Ground> {
    pub levels: Vec<Level>,
    ground: &'g G,
    pub p: Params,
    /// Per level: troughs whose upstream reached the read edge, vertices
    /// that did, own vertices left undecided, own vertices in a cycle.
    pub route_stats: Vec<[u64; 4]>,
    /// Per level, the wall time each stage took there, in seconds, nested
    /// work at other levels not counted.
    pub timing: Vec<[f64; 6]>,
}

impl<'g, G: Ground> Ladder<'g, G> {
    /// A ladder over `extents`, each level's width and its cells' index range
    /// `lo..hi` on both axes, coarsest first; the top built over all of its.
    pub fn new(ground: &'g G, extents: &[(i64, i64, i64)], p: Params) -> Self {
        let boxes: Vec<Bounds> = extents.iter().map(|&(w, lo, hi)| (w, (lo, lo), (hi, hi))).collect();
        Ladder::new_box(ground, &boxes, p)
    }

    /// A ladder over `bounds`, each level's width and its cells' index range
    /// along i and along j, coarsest first; the top built over all of its.
    pub fn new_box(ground: &'g G, bounds: &[Bounds], p: Params) -> Self {
        Ladder::with_top(ground, bounds, p, None)
    }

    /// The top over `bounds` of `ground`, built.
    pub fn top(ground: &'g G, bounds: Bounds, p: Params) -> Level {
        Ladder::new_box(ground, &[bounds], p).levels.remove(0)
    }

    /// A ladder over `bounds` whose top is `top`, built over `bounds[0]`
    /// with the same ground and parameters, or built here without one.
    pub fn with_top(ground: &'g G, bounds: &[Bounds], p: Params, top: Option<Level>) -> Self {
        let mut levels: Vec<Level> = Vec::new();
        for (k, &(w, lo, hi)) in bounds.iter().enumerate() {
            // A cell is the level's within its bounds, its parent the level
            // above's: every cell's parent lies inside the level above.
            if k > 0 {
                let par = &levels[k - 1];
                let edge = (lo.0..hi.0)
                    .flat_map(|i| [(i, lo.1), (i, hi.1 - 1)])
                    .chain((lo.1..hi.1).flat_map(|j| [(lo.0, j), (hi.0 - 1, j)]));
                for (i, j) in edge {
                    let (pi, pj, _, _) = parent_of(i, j);
                    assert!(par.idx(pi, pj).is_some(), "level {w}'s cell ({i}, {j}) lies past the level above's bounds");
                }
            }
            levels.push(Level::new(w, lo, hi, k == 0, p.ring));
        }
        let count = levels.len();
        let mut ladder = Ladder {
            levels,
            ground,
            p,
            route_stats: vec![[0; 4]; count],
            timing: vec![[0.0; 6]; count],
        };
        match top {
            Some(top) => {
                let lv = &ladder.levels[0];
                assert_eq!((top.w, top.lo, top.n, top.lo_j, top.n_j), (lv.w, lv.lo, lv.n, lv.lo_j, lv.n_j), "a top over the ladder's top bounds");
                ladder.levels[0] = top;
            }
            None => ladder.build_top(),
        }
        ladder
    }

    /// Reads the ground at each vertex of every cell of a mask not yet read
    /// ([`Level::own_vertices`]), and the rock, the plate and the sky at
    /// its centre.
    fn read_ground(&mut self, k: usize, mask: &Mask) {
        let lv = &self.levels[k];
        let todo: Vec<usize> = mask.iter().filter(|&c| lv.exists[c] && lv.ground[c].is_nan()).collect();
        if todo.is_empty() {
            return;
        }
        let started = Instant::now();
        let (ground, level): (&G, &Level) = (self.ground, lv);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let chunk = todo.len().div_ceil(threads).max(1);
        let read: Vec<Vec<Point>> = std::thread::scope(|s| {
            let handles: Vec<_> = todo
                .chunks(chunk)
                .map(|part| {
                    s.spawn(move || {
                        part.iter()
                            .map(|&c| {
                                let (i, j) = level.ij(c);
                                level
                                    .own_vertices(c)
                                    .map(|(t, _)| {
                                        let (q3, r3) = pos3(t, i, j, level.w);
                                        ground.at(q3 / 3, r3 / 3)
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().expect("a reading thread")).collect()
        });
        let lv = &mut self.levels[k];
        for (&c, points) in todo.iter().zip(read) {
            let vertices: Vec<(u8, usize)> = lv.own_vertices(c).collect();
            for ((_, v), point) in vertices.into_iter().zip(&points) {
                lv.ground[v] = point.height;
            }
            lv.erod[c] = points[0].erodibility;
            lv.age[c] = points[0].age;
            lv.rain[c] = points[0].moisture;
        }
        self.timing[k][Stage::Read as usize] += started.elapsed().as_secs_f64();
    }

    /// The top: the ground at its vertices, rounded to steps, routed once
    /// from its edge and the sea.
    fn build_top(&mut self) {
        let top = &self.levels[0];
        let all = Mask::new((0..top.cap()).filter(|&c| top.exists[c]));
        self.read_ground(0, &all);
        let p = self.p;
        let lv = &mut self.levels[0];
        let vertices: Vec<usize> = all.iter().flat_map(|c| lv.own_vertices(c).map(|(_, v)| v).collect::<Vec<_>>()).collect();
        for &v in &vertices {
            let h = lv.ground[v];
            lv.carved[v] = h;
            lv.sunk[v] = h;
            lv.banks[v] = h;
            lv.z[v] = (h + 0.5).floor();
            lv.zd[v] = lv.z[v];
        }
        lv.supplied.fill(true);
        lv.built.fill(true);
        top::route_top(lv, &p);
        lv.routed.fill(true);
        // The top's own valleys, cut along its troughs into the ground it
        // routed over.
        let edges = water::trough_edges(lv, &all, &p);
        water::top_cut(lv, &edges, &p);
        for &v in &vertices {
            lv.z[v] = (lv.ground[v] + lv.cut[v] + 0.5).floor();
        }
        for c in all.iter() {
            if lv.water[c] <= lv.z[c] {
                lv.water[c] = f64::NAN;
            }
        }
        corners_over(lv, &all);
    }

    /// Each requested cell's level k routed and built, and its corners,
    /// which its neighbours' heights decide.
    pub fn request(&mut self, k: usize, cells: &[(i64, i64)]) {
        let lv = &self.levels[k];
        // A request for cells already finished reads what is kept.
        if k == 0 || cells.iter().all(|&(i, j)| lv.idx(i, j).is_none_or(|c| !lv.exists[c] || lv.finished[c])) {
            return;
        }
        let mask = Mask::new(cells.iter().filter_map(|&(i, j)| lv.idx(i, j)).filter(|&c| lv.exists[c]));
        self.ensure_routed(k, &mask);
        self.cornered(k, &mask);
    }

    /// Level k built two rings past a mask and its corners one ring past
    /// it, so every corner of every cell of the mask is final.
    fn cornered(&mut self, k: usize, mask: &Mask) {
        let lv = &self.levels[k];
        let (one, two) = (lv.dilate(mask, 1), lv.dilate(mask, 2));
        self.ensure_built(k, &two);
        let started = Instant::now();
        corners_over(&mut self.levels[k], &one);
        self.timing[k][Stage::Corners as usize] += started.elapsed().as_secs_f64();
    }

    fn ensure_routed(&mut self, k: usize, mask: &Mask) {
        if k == 0 {
            return;
        }
        let lv = &self.levels[k];
        let need = mask.filter(|c| !lv.routed[c] && lv.exists[c]);
        if need.is_empty() {
            return;
        }
        let pend = lv.pending(Pass::Route, &need);
        let fp = lv.footprint(Pass::Route, &pend);
        self.ensure_supplied(k, &fp);
        self.parent_over(k, &fp);
        let started = Instant::now();
        let (above, rest) = self.levels.split_at_mut(k);
        let lv = &mut rest[0];
        tile_corners(lv, &fp);
        let stats = route::route_blocks(lv, &above[k - 1], &pend, &self.p, births(&above[k - 1], lv, &self.p));
        for (a, b) in self.route_stats[k].iter_mut().zip(stats) {
            *a += b;
        }
        lv.cost[Pass::Route as usize] += pend.len() as u64;
        self.timing[k][Stage::Route as usize] += started.elapsed().as_secs_f64();
    }

    /// The level above under a mask and a ring of its cells: read, routed,
    /// built and cornered.
    fn parent_over(&mut self, k: usize, mask: &Mask) {
        let (lv, par) = (&self.levels[k], &self.levels[k - 1]);
        let under = Mask::new(mask.iter().filter_map(|c| {
            let (i, j) = lv.ij(c);
            let (pi, pj, _, _) = parent_of(i, j);
            par.idx(pi, pj)
        }));
        let pmask = par.dilate(&under, 1);
        self.read_ground(k - 1, &pmask);
        if k - 1 > 0 {
            self.ensure_routed(k - 1, &pmask);
            self.cornered(k - 1, &pmask);
        }
    }

    fn ensure_supplied(&mut self, k: usize, mask: &Mask) {
        let lv = &self.levels[k];
        let need = mask.filter(|c| !lv.supplied[c] && lv.exists[c]);
        if need.is_empty() {
            return;
        }
        let pend = lv.pending(Pass::Ground, &need);
        let fp = lv.footprint(Pass::Ground, &pend);
        self.parent_over(k, &fp);
        self.read_ground(k, &fp);
        let bands = self.rivers_above(k, &fp);
        let started = Instant::now();
        let lv = &mut self.levels[k];
        let todo: Vec<usize> = fp.iter().filter(|&c| lv.exists[c] && !lv.supplied[c]).collect();
        let carved = water::carved_over(lv, &bands, &todo);
        for (&c, (at, near, beside)) in todo.iter().zip(carved) {
            let vertices: Vec<(u8, usize)> = lv.own_vertices(c).collect();
            for (t, v) in vertices {
                lv.carved[v] = at[t as usize];
                lv.sunk[v] = near[t as usize];
                lv.banks[v] = beside[t as usize];
            }
            lv.supplied[c] = true;
        }
        lv.cost[Pass::Ground as usize] += pend.len() as u64;
        self.timing[k][Stage::Supply as usize] += started.elapsed().as_secs_f64();
    }

    /// Every coarser level's rivers within reach of a mask of level k, a
    /// band to each: its troughs over its ring of cells past the mask, read
    /// out to its ring.
    fn rivers_above(&mut self, k: usize, mask: &Mask) -> Vec<water::Band> {
        let mut bands = Vec::with_capacity(k);
        for j in 0..k {
            let (lv, up) = (&self.levels[k], &self.levels[j]);
            let under = Mask::new(mask.iter().filter_map(|c| {
                let (i, jj) = lv.ij(c);
                let (a, b) = ancestor(i, jj, (k - j) as u32);
                up.idx(a, b)
            }));
            // Rings of cells a ring's reach in tiles past the mask, a cell's
            // half width beside and a cell more for a meander's swing
            // ([`water::meander`]): k rings stand k·√3/2 of the level's
            // width out at their nearest.
            let rings = ((self.p.ring + 2.0) * 2.0 / crate::SQRT_3).ceil() as usize + 1;
            let reach = up.dilate(&under, rings);
            // Routed two rings past what is read, so each trough's
            // receiver has its floor and its plain, and its receiver's.
            let routed = up.dilate(&reach, 2);
            self.ensure_routed(j, &routed);
            let up = &self.levels[j];
            let read = self.levels[k].w;
            bands.push(water::Band::new(up.w, &water::trough_edges(up, &reach, &self.p), -1.0, self.p.ring * up.w as f64, read, &self.p));
        }
        bands
    }

    fn ensure_built(&mut self, k: usize, mask: &Mask) {
        let lv = &self.levels[k];
        let need = mask.filter(|c| !lv.built[c] && lv.exists[c]);
        if need.is_empty() {
            return;
        }
        let sel = need.filter(|c| !lv.watered[c]);
        if !sel.is_empty() {
            let pend = lv.pending(Pass::Water, &sel);
            let fp = lv.footprint(Pass::Water, &pend);
            // Routed two rings past the footprint, so each line's next
            // vertex has its floor, and that vertex's next.
            let routed = lv.dilate(&fp, 2);
            self.ensure_supplied(k, &fp);
            self.ensure_routed(k, &routed);
            let started = Instant::now();
            let lv = &mut self.levels[k];
            let band = water::Band::new(lv.w, &water::trough_edges(lv, &fp, &self.p), -1.0, self.p.ring * lv.w as f64, lv.w, &self.p);
            water::level_cut(lv, &band, &pend, &self.p);
            lv.cost[Pass::Water as usize] += pend.len() as u64;
            self.timing[k][Stage::Water as usize] += started.elapsed().as_secs_f64();
        }
        self.ensure_supplied(k, &need);
        let started = Instant::now();
        let lv = &mut self.levels[k];
        let mut built = 0;
        for c in need.iter() {
            if lv.supplied[c] && lv.watered[c] {
                let vertices: Vec<(u8, usize)> = lv.own_vertices(c).collect();
                for (_, v) in vertices {
                    lv.z[v] = (lv.ground[v] + lv.cut[v] + 0.5).floor();
                    lv.zd[v] = (lv.ground[v] + 0.5).floor();
                }
                if lv.water[c] <= lv.z[c] {
                    lv.water[c] = f64::NAN;
                }
                // A lake, water standing over no river's bed, covers the
                // whole cell.
                if !lv.water[c].is_nan() && lv.wwet[c] < 0 {
                    lv.wparts[c] = (1 << CHILD_OFFSETS.len()) - 1;
                }
                lv.built[c] = true;
                built += 1;
            }
        }
        lv.cost[4] += built;
        assert!(need.iter().all(|c| lv.built[c]), "a built cell read something its deform did not cover");
        self.timing[k][Stage::Build as usize] += started.elapsed().as_secs_f64();
    }

    /// For the probes: the cut at each of `cells` of level k from every river
    /// the level has drawn, however far, where its water reads only its
    /// ring.
    #[cfg(feature = "inspect")]
    pub fn unlimited_cut(&self, k: usize, cells: &[(i64, i64)]) -> Vec<f64> {
        let lv = &self.levels[k];
        let routed = Mask::new(lv.routed.written().flat_map(|first| first..first + PAGE).filter(|&c| lv.routed[c]));
        let edges = water::trough_edges(lv, &routed, &self.p);
        let at: Vec<usize> = cells.iter().map(|&(i, j)| lv.idx(i, j).expect("a cell the level holds")).collect();
        water::whole_cut(lv, &edges, &at, &self.p)
    }
}

/// Whether troughs are born at a level: only where its head is below the
/// level above's.
fn births(par: &Level, lv: &Level, p: &Params) -> bool {
    head_of(lv.w, 1.0, HEAD_SLOPE, p) < head_of(par.w, 1.0, HEAD_SLOPE, p)
}

/// Every built cell of a mask, and every cell outside it with a corner the
/// mask owns, marked finished where all six of its corners are built, so a
/// cell's corners never wait on a request that names it.
fn corners_over(lv: &mut Level, mask: &Mask) {
    let mut draw: Vec<usize> = Vec::new();
    for c in mask.iter() {
        if !lv.exists[c] || !lv.built[c] {
            continue;
        }
        let (i, j) = lv.ij(c);
        draw.push(c);
        draw.extend([(i + 1, j - 1), (i + 1, j), (i, j + 1)].iter().filter_map(|&(x, y)| lv.idx(x, y)));
    }
    draw.sort_unstable();
    draw.dedup();
    for c in draw {
        if lv.exists[c] && lv.built[c] && !lv.finished[c] {
            let (i, j) = lv.ij(c);
            lv.finished[c] = [(i - 1, j + 1), (i - 1, j), (i, j - 1)]
                .iter()
                .all(|&(a, b)| lv.idx(a, b).is_none_or(|d| !lv.exists[d] || lv.built[d]));
        }
    }
}

/// A tile's corners over a mask: the dry, the carved and the sunk ground
/// and the banks of each the mean of the three tiles sharing it, where all
/// three are supplied. A corner of a coarser level is read where it stands.
fn tile_corners(lv: &mut Level, mask: &Mask) {
    if lv.w >= 3 {
        return;
    }
    let nn = lv.cap();
    for c in mask.iter() {
        if !lv.exists[c] || !lv.supplied[c] {
            continue;
        }
        let (i, j) = lv.ij(c);
        let (a, b, ready_a, ready_b) = lv.corner_means(i, j, &lv.carved, &lv.supplied);
        let (da, db, _, _) = lv.corner_means(i, j, &lv.ground, &lv.supplied);
        let (sa, sb, _, _) = lv.corner_means(i, j, &lv.sunk, &lv.supplied);
        let (ba, bb, _, _) = lv.corner_means(i, j, &lv.banks, &lv.supplied);
        if ready_a {
            lv.carved[nn + c] = a;
            lv.ground[nn + c] = da;
            lv.sunk[nn + c] = sa;
            lv.banks[nn + c] = ba;
        }
        if ready_b {
            lv.carved[2 * nn + c] = b;
            lv.ground[2 * nn + c] = db;
            lv.sunk[2 * nn + c] = sb;
            lv.banks[2 * nn + c] = bb;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A channel falls at the head's slope where it begins, flatter the more
    /// water it gathers, and steeper in harder rock.
    #[test]
    fn a_graded_channel_flattens_downstream_and_stands_steeper_in_hard_rock() {
        let p = Params::default();
        assert_eq!(graded_slope(cut_head(1.0, &p), 1.0, &p), HEAD_SLOPE);
        assert_eq!(graded_slope(1.0, 1.0, &p), HEAD_SLOPE);
        let mut last = HEAD_SLOPE;
        for catchment in [1e4, 1e5, 1e6, 1e7, 1e8] {
            let s = graded_slope(catchment, 1.0, &p);
            assert!(s < last, "{catchment} tiles fall at {s}, no flatter than {last}");
            assert!(graded_slope(catchment, HARDEST, &p) > s);
            last = s;
        }
    }

    /// A floor never stands above its ground, cuts nothing where the river
    /// is at grade, cuts deeper the older its plate, and no deeper than its
    /// walls climb from the nearer divide.
    #[test]
    fn a_floor_cuts_toward_its_profile_by_age_and_within_its_room() {
        let p = Params::default();
        assert_eq!(drained(40.0, 50.0, 1.0, 1.0, 100.0, &p), 40.0);
        assert_eq!(drained(40.0, f64::NAN, 1.0, 1.0, 100.0, &p), 40.0);
        let (young, old) = (drained(200.0, 20.0, 0.0, 1.0, 1e9, &p), drained(200.0, 20.0, 1.0, 1.0, 1e9, &p));
        assert!(old < young && young < 200.0 && old >= 20.0);
        assert_eq!(drained(200.0, 20.0, 1.0, 1.0, 5.0, &p), 200.0 - wall_slope(1.0, p.wall) * 5.0);
    }
}
