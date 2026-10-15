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
//! - **ground**: the level above's dry surface predicted at each centre,
//!   and every layer's change there, each layer's value at the cell less its
//!   own value predicted from the level above: the level's dry ground. Over
//!   it the level above's carve predicted the same way: its carved ground.
//! - **route**: every vertex's receiver downhill on the carved ground, its
//!   catchment, and the level's troughs: the level above's rivers run on
//!   from its nodes, and troughs born at the level ([`route`]). A trough's
//!   floor is the level's own dry ground less a share of its height above
//!   the level its water ends at, growing with the catchment it carries,
//!   the plate's age and the rock's erodibility ([`drained`]), and never
//!   above the floors draining into it.
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
//! Every layer writes its value averaged over the level's cell; its raise
//! there is that less what the level above handed down, the level above's
//! values predicted at the cell ([`handed_down`]).
//!
//! **A river is handed down as where it is and what it carries, never its
//! course.** The level above hands down its trough vertices, each with its
//! catchment and the level it was born at; each is one of the level's
//! vertices, since levels nest, and the level's own water runs the river on
//! from there, downhill, wherever its ground leads; a river starts once,
//! where it first comes into reach. Where troughs are born, routing picks
//! between the two neighbours bracketing the downslope by chance weighted
//! by the slope, so on even ground they wander and join as drainage does;
//! below, a river follows its valley. Water never climbs ([`route`]).
//!
//! **A level carves into what the level above carved.** It is handed the
//! dry ground's prediction and the carve's: routing reads the carved
//! ground, so water gathers into the valleys above; floors read the dry
//! ground, so no floor deepens by what the level above already cut. A
//! level's height is its own troughs' valleys within its ring met by its
//! carved ground, averaged over the cell and rounded to whole steps: a
//! valley is an absolute height, so a river the level above cut to its
//! floor is not cut again. Its water fills that carve. Nothing makes it
//! walkable: a step past walkable is a face.
//!
//! **Prediction is apart from drawing.** A level hands its shape to the
//! level below as its prediction surface ([`handed_down`]): sloped fans
//! through its cells, a centre the cell's value, a corner the mean of the
//! three cells sharing it, linear between, nothing clamped, met or rounded.
//! Everything in the ladder that reads a surface reads that one: the dry
//! ground, every layer, routing.
//! The corner rule, faces as walls, is the drawing's alone: meshes, physics
//! and summaries draw a level's cells by it (`common::corner`), and nothing
//! here reads it.
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
    /// The catchment at which a trough's bed is a trunk's, in tiles.
    pub catchment_full: f64,
    /// The catchment at which a channel begins to cut, and a trough is born,
    /// in the softest rock, and at which it has cut its full share, in
    /// tiles.
    pub cut_from: f64,
    pub cut_full: f64,
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
        Params { catchment_full: 400_000.0, cut_from: 4_000.0, cut_full: 118_098.0, wall: 1.0, ring: RING, water: true }
    }
}

/// What the ladder is seeded with: each layer's value over a cell of any
/// level, in steps, and the rock and the plate at a cell's centre.
pub trait Ground: Sync {
    /// How many layers the ground is made of.
    fn layers(&self) -> usize;
    /// Each layer's value over cell (i, j) of the level `w` tiles wide.
    fn read(&self, w: i64, i: i64, j: i64, out: &mut [f64]);
    /// The erodibility of the rock at the cell's centre.
    fn erodibility(&self, w: i64, i: i64, j: i64) -> f64;
    /// The age of the plate at the cell's centre, 0 to 1.
    fn age(&self, w: i64, i: i64, j: i64) -> f64;
}

/// How far water reads lines from a cell, in cells of the level: what one
/// ring of its deform cell, 27 of the level's cells wide, clears.
pub const RING: f64 = 15.588_457_268_119_896;
/// The least erodible rock, whose walls stand at [`WALL_HARDEST`].
pub const HARDEST: f64 = 0.3;
/// The steepest a wall stands, in the hardest rock, in steps per tile.
pub const WALL_HARDEST: f64 = 3.0;
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
/// `erodibility`: where a channel begins to cut there ([`cut_head`]), and
/// never less than two of the level's cells, the narrowest trough a level
/// resolves.
pub fn head_of(w: i64, erodibility: f64, p: &Params) -> f64 {
    cut_head(erodibility, p).max(2.0 * (w * w) as f64)
}

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

/// The share of its height above base level a full trunk on a fully aged
/// plate has removed in the softest rock: the Grand Canyon's 1.6 km cut
/// through a 2.3 km plateau. The rest is the fall that keeps it flowing.
pub const RELIEF_SHARE_MAX: f64 = 0.7;

/// How far past the channel head on rock of `erodibility` a catchment of
/// `catchment` tiles has grown, as the square root of its share of the way
/// to a full trunk, the way a channel's width and depth grow with
/// discharge. Nothing below the head ([`cut_head`]).
pub fn growth(catchment: f64, erodibility: f64, p: &Params) -> f64 {
    let head = cut_head(erodibility, p);
    if catchment <= head {
        return 0.0;
    }
    ((catchment - head) / (p.cut_full - head).max(1.0)).min(1.0).sqrt()
}

/// The share of its height above base level a channel has cut: growing
/// with its catchment past the head, with its plate's age from a young
/// plate's share of the work to the whole, and with the rock, the whole in
/// shale and three tenths in basement.
pub fn relief_share(catchment: f64, age: f64, erodibility: f64, p: &Params) -> f64 {
    RELIEF_SHARE_MAX * growth(catchment, erodibility, p) * crate::tectonic::aged(age) * erodibility
}

/// Drainage's floor under ground at `elevation`: the ground less its
/// share of the height above `base`, the level its water runs down to, and
/// never deeper than a wall at the rock's slope climbs over `room` tiles,
/// the distance to the divide of the basin it drains.
pub fn drained(elevation: f64, base: f64, catchment: f64, age: f64, erodibility: f64, room: f64, p: &Params) -> f64 {
    let cut = (elevation - base).max(0.0) * relief_share(catchment, age, erodibility, p);
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
    /// Each layer's value at each cell, NaN until read.
    pub layers: Vec<Paged<f64>>,
    pub erod: Paged<f64>,
    pub age: Paged<f64>,
    /// The level above's surface at each centre.
    pub p: Paged<f64>,
    /// The ground: the prediction and every layer's change but water's.
    pub ground: Paged<f64>,
    /// The carved ground: the ground with the level above's carve, its
    /// heights less its dry heights, predicted at the cell.
    pub carved: Paged<f64>,
    /// The level's carve: its valleys met by its carved ground, averaged
    /// over the cell, less the ground; never above the carved ground.
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
    pub z: Paged<f64>,
    /// Each cell's dry height: its ground before the carve, rounded, what
    /// the level below predicts its own dry ground from.
    pub zd: Paged<f64>,
    /// Each cell's six corners on the prediction surface of its heights.
    pub cc: Paged<[f64; 6]>,
    // Each vertex: a cell's centre, its corner A and its corner B.
    /// The prediction surface of the heights at each vertex: a centre's
    /// own, a corner the mean of the cells sharing it.
    pub elev: Paged<f64>,
    pub psurf: Paged<f64>,
    /// The carved ground at each vertex: a centre's own, a corner the mean
    /// of its sharers'. What lines and routing are read from.
    pub gelev: Paged<f64>,
    /// The dry ground at each vertex, the same way: what drainage's floor
    /// is read from.
    pub gdry: Paged<f64>,
    /// The level water at each vertex runs down to: the sea, or the pit of
    /// the closed ground it drains into.
    pub base: Paged<f64>,
    /// The width of the level a trough at each vertex was born at; -1 off
    /// the troughs.
    pub born: Paged<i64>,
    pub rec: Paged<i64>,
    pub trough: Paged<bool>,
    pub acc: Paged<f64>,
    pub floor: Paged<f64>,
    pub predicted: Paged<bool>,
    pub supplied: Paged<bool>,
    pub watered: Paged<bool>,
    pub built: Paged<bool>,
    pub routed: Paged<bool>,
    /// Whether each cell's own two corners are final: every cell sharing
    /// them built.
    pub corners: Paged<bool>,
    /// Whether each cell's six corner values are final: its own two, and
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
    fn new(w: i64, lo: (i64, i64), hi: (i64, i64), top: bool, layers: usize, ring: f64) -> Self {
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
            layers: (0..layers).map(|_| nan(cells)).collect(),
            erod: nan(cells),
            age: nan(cells),
            p: nan(cells),
            ground: nan(cells),
            carved: nan(cells),
            cut: nan(cells),
            wdist: nan(cells),
            wborn: Paged::new(-1, cells),
            wsrc: Paged::new(-1, cells),
            wgov: Paged::new(-1, cells),
            wgfloor: nan(cells),
            wwet: Paged::new(-1, cells),
            water: nan(cells),
            wdepth: nan(cells),
            wparts: Paged::new(0, cells),
            z: nan(cells),
            zd: nan(cells),
            cc: Paged::new([f64::NAN; 6], cells),
            elev: nan(3 * cells),
            psurf: nan(3 * cells),
            gelev: nan(3 * cells),
            gdry: nan(3 * cells),
            base: nan(3 * cells),
            born: Paged::new(-1, 3 * cells),
            rec: Paged::new(-1, 3 * cells),
            trough: Paged::new(false, 3 * cells),
            acc: Paged::new(0.0, 3 * cells),
            floor: nan(3 * cells),
            predicted: Paged::new(false, cells),
            supplied: Paged::new(false, cells),
            watered: Paged::new(false, cells),
            built: Paged::new(false, cells),
            routed: Paged::new(false, cells),
            corners: Paged::new(false, cells),
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

    /// The prediction surface's values at a cell's own corners A and B: the
    /// mean of what the cells sharing each hold; each ready where every cell
    /// there that exists holds its value.
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

    /// A cell's six corner values on the prediction surface of its heights:
    /// its own A and B, and the four its neighbours own; its height where a
    /// corner has none.
    fn corner_values(&self, i: i64, j: i64) -> [f64; 6] {
        let nn = self.cap();
        let get = |a: i64, b: i64, t: usize| self.idx(a, b).map_or(f64::NAN, |c| self.elev[t * nn + c]);
        let z = self.idx(i, j).map_or(f64::NAN, |c| self.z[c]);
        let cc = [get(i, j, 2), get(i, j, 1), get(i - 1, j + 1, 2), get(i - 1, j, 1), get(i - 1, j, 2), get(i, j - 1, 1)];
        cc.map(|v| if v.is_nan() { z } else { v })
    }

    /// The level's prediction surface at a position in tiles: the fan
    /// triangle of the cell under it, from the cell's centre to two of its
    /// corners.
    pub fn surface_at(&self, q: f64, r: f64) -> f64 {
        let (i, j) = cube_round(q / self.w as f64, r / self.w as f64);
        let Some(c) = self.idx(i, j) else { return f64::NAN };
        fan_at(self.w, i, j, self.z[c], &self.cc[c], q, r)
    }
}

/// A surface of fans at a position in tiles over cell (i, j) of a level `w`
/// wide, its height `z` and six corner values `cc`: the fan triangle under
/// it, from the cell's centre to two of its corners.
pub(crate) fn fan_at(w: i64, i: i64, j: i64, z: f64, cc: &[f64; 6], q: f64, r: f64) -> f64 {
    {
        let (u, v) = (q / w as f64, r / w as f64);
        let (du, dv) = (u - i as f64, v - j as f64);
        let (px, py) = (1.5 * du, crate::SQRT_3 * (dv + du / 2.0));
        let sixth = std::f64::consts::PI / 3.0;
        let ang = py.atan2(px).rem_euclid(2.0 * std::f64::consts::PI);
        let s = ((ang / sixth).floor() as usize) % 6;
        let s1 = (s + 1) % 6;
        let (c0x, c0y) = ((s as f64 * sixth).cos(), (s as f64 * sixth).sin());
        let (c1x, c1y) = ((s1 as f64 * sixth).cos(), (s1 as f64 * sixth).sin());
        let det = c0x * c1y - c0y * c1x;
        let l1 = (px * c1y - py * c1x) / det;
        let l2 = (c0x * py - c0y * px) / det;
        let l0 = 1.0 - l1 - l2;
        l0 * z + l1 * cc[s] + l2 * cc[s1]
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
            levels.push(Level::new(w, lo, hi, k == 0, ground.layers(), p.ring));
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

    /// Reads each layer's value at every cell of a mask not yet read.
    fn read_layers(&mut self, k: usize, mask: &Mask) {
        let lv = &self.levels[k];
        let todo: Vec<usize> = mask.iter().filter(|&c| lv.exists[c] && lv.layers[0][c].is_nan()).collect();
        if todo.is_empty() {
            return;
        }
        let started = Instant::now();
        let (w, layers, ground) = (lv.w, self.ground.layers(), self.ground);
        let cells: Vec<(i64, i64)> = todo.iter().map(|&c| lv.ij(c)).collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let chunk = cells.len().div_ceil(threads).max(1);
        let read: Vec<(Vec<f64>, f64, f64)> = std::thread::scope(|s| {
            let handles: Vec<_> = cells
                .chunks(chunk)
                .map(|part| {
                    s.spawn(move || {
                        part.iter()
                            .map(|&(i, j)| {
                                let mut out = vec![0.0; layers];
                                ground.read(w, i, j, &mut out);
                                (out, ground.erodibility(w, i, j), ground.age(w, i, j))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().expect("a reading thread")).collect()
        });
        let lv = &mut self.levels[k];
        for (&c, (vals, erod, age)) in todo.iter().zip(read) {
            for (l, v) in vals.into_iter().enumerate() {
                lv.layers[l][c] = v;
            }
            lv.erod[c] = erod;
            lv.age[c] = age;
        }
        self.timing[k][Stage::Read as usize] += started.elapsed().as_secs_f64();
    }

    /// The top: the sum of the layers over its cells, rounded to steps,
    /// routed once from its edge and the sea.
    fn build_top(&mut self) {
        let top = &self.levels[0];
        let all = Mask::new((0..top.cap()).filter(|&c| top.exists[c]));
        self.read_layers(0, &all);
        let p = self.p;
        let lv = &mut self.levels[0];
        for c in 0..lv.cap() {
            let h: f64 = lv.layers.iter().map(|l| l[c]).sum();
            lv.p[c] = h;
            lv.ground[c] = h;
            lv.carved[c] = h;
            lv.z[c] = (h + 0.5).floor();
            lv.zd[c] = lv.z[c];
        }
        lv.predicted.fill(true);
        lv.supplied.fill(true);
        lv.built.fill(true);
        corners_over(lv, &all);
        top::route_top(lv, &p);
        lv.routed.fill(true);
        // The top's own valleys, cut along its troughs into the ground it
        // routed over.
        let edges = water::trough_edges(lv, &all);
        water::top_cut(lv, &edges, &p);
        for c in 0..lv.cap() {
            lv.z[c] = (lv.ground[c] + lv.cut[c] + 0.5).floor();
            if lv.water[c] <= lv.z[c] {
                lv.water[c] = f64::NAN;
            }
        }
        // The cut moved every height, so every corner is made again.
        lv.corners.fill(false);
        lv.finished.fill(false);
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
        ground_fields(lv, &above[k - 1], &fp);
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
        self.read_layers(k - 1, &pmask);
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
        self.read_layers(k, &fp);
        let started = Instant::now();
        let (above, rest) = self.levels.split_at_mut(k);
        let lv = &mut rest[0];
        let par = &above[k - 1];
        for c in fp.iter() {
            if !lv.exists[c] || lv.supplied[c] {
                continue;
            }
            let (i, j) = lv.ij(c);
            let (pred, layer_pred) = predict(par, i, j);
            lv.p[c] = pred;
            let layers = lv.layers.len();
            let change: f64 = (0..layers).map(|l| lv.layers[l][c] - layer_pred[l]).sum();
            lv.ground[c] = pred + change;
            lv.carved[c] = lv.ground[c] + handed_down(i, j, |x, y| par.carve(x, y));
            lv.predicted[c] = true;
            lv.supplied[c] = true;
        }
        lv.cost[Pass::Ground as usize] += pend.len() as u64;
        self.timing[k][Stage::Supply as usize] += started.elapsed().as_secs_f64();
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
            // Routed a ring past the footprint, so each line's next vertex
            // has its floor.
            let routed = lv.dilate(&fp, 1);
            self.ensure_supplied(k, &fp);
            self.ensure_routed(k, &routed);
            let started = Instant::now();
            let lv = &mut self.levels[k];
            let band = water::Band::new(lv.w, &water::trough_edges(lv, &fp), -1.0, self.p.ring * lv.w as f64, &self.p);
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
                lv.z[c] = (lv.ground[c] + lv.cut[c] + 0.5).floor();
                lv.zd[c] = (lv.ground[c] + 0.5).floor();
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
        let edges = water::trough_edges(lv, &routed);
        let at: Vec<usize> = cells.iter().map(|&(i, j)| lv.idx(i, j).expect("a cell the level holds")).collect();
        water::whole_cut(lv, &edges, &at, &self.p)
    }
}

/// Whether troughs are born at a level: only where its head is below the
/// level above's.
fn births(par: &Level, lv: &Level, p: &Params) -> bool {
    head_of(lv.w, 1.0, p) < head_of(par.w, 1.0, p)
}

/// The cells sharing parent corner `k` of cell (a, b), as
/// [`Level::corner_values`] numbers them.
fn corner_cells(a: i64, b: i64, k: usize) -> [(i64, i64); 3] {
    let (t, ci, cj) = [(2, a, b), (1, a, b), (2, a - 1, b + 1), (1, a - 1, b), (2, a - 1, b), (1, a, b - 1)][k];
    vertex_cells(t, ci, cj).0
}

/// A value the level above holds at each of its cells, `at`, handed down to
/// child cell (i, j): the parent's own at its centre, a corner's the mean
/// of the cells sharing it that hold one, an axis child's the mean of its
/// fan's centre and two corners.
pub fn handed_down(i: i64, j: i64, at: impl Fn(i64, i64) -> f64) -> f64 {
    let (a, b, oi, oj) = parent_of(i, j);
    let corner = |k: usize| {
        let vals: Vec<f64> = corner_cells(a, b, k).iter().map(|&(x, y)| at(x, y)).filter(|v| !v.is_nan()).collect();
        vals.iter().sum::<f64>() / vals.len() as f64
    };
    match (oi, oj) {
        (0, 0) => at(a, b),
        (1, 1) => corner(1),
        (2, -1) => corner(0),
        _ => {
            let (ca, cb) = axis_fan(oi, oj).expect("an axis child");
            (at(a, b) + corner(ca) + corner(cb)) / 3.0
        }
    }
}

/// A child cell's prediction: the level above's dry ground, and each
/// layer's own value, each read off the level above's prediction surface
/// ([`handed_down`]).
pub(crate) fn predict(par: &impl Above, i: i64, j: i64) -> (f64, Vec<f64>) {
    let dry = handed_down(i, j, |x, y| par.dry(x, y));
    let layers = (0..par.layer_count()).map(|l| handed_down(i, j, |x, y| par.field(l, x, y))).collect();
    (dry, layers)
}

/// What a level hands the level below it of its heights: each built cell's
/// dry height and its carve, and each layer's value at a cell; NaN where it
/// holds none.
pub(crate) trait Above {
    fn layer_count(&self) -> usize;
    fn dry(&self, i: i64, j: i64) -> f64;
    /// The carve at cell (i, j): its height less its dry height.
    fn carve(&self, i: i64, j: i64) -> f64;
    /// Layer `l`'s value at cell (i, j).
    fn field(&self, l: usize, i: i64, j: i64) -> f64;
}

impl Above for Level {
    fn layer_count(&self) -> usize {
        self.layers.len()
    }

    fn dry(&self, i: i64, j: i64) -> f64 {
        self.idx(i, j).filter(|&c| self.exists[c] && self.built[c]).map_or(f64::NAN, |c| self.zd[c])
    }

    fn carve(&self, i: i64, j: i64) -> f64 {
        self.idx(i, j).filter(|&c| self.exists[c] && self.built[c]).map_or(f64::NAN, |c| self.z[c] - self.zd[c])
    }

    fn field(&self, l: usize, i: i64, j: i64) -> f64 {
        self.idx(i, j).map_or(f64::NAN, |c| self.layers[l][c])
    }
}

/// The prediction surface of the heights at the corners of every cell of a
/// mask whose sharers are all built, and the six corner values of each
/// cell, and of every cell outside the mask reading a corner made here, so
/// a cell's corners never wait on a request that names it.
fn corners_over(lv: &mut Level, mask: &Mask) {
    let nn = lv.cap();
    // Every cell of the mask, and every cell reading a corner made here.
    let mut draw: Vec<usize> = Vec::new();
    for c in mask.iter() {
        if !lv.exists[c] || !lv.built[c] {
            continue;
        }
        draw.push(c);
        if lv.corners[c] {
            continue;
        }
        let (i, j) = lv.ij(c);
        let (a, b, ready_a, ready_b) = lv.corner_means(i, j, &lv.z, &lv.built);
        lv.elev[c] = lv.z[c];
        if ready_a {
            lv.elev[nn + c] = a;
        }
        if ready_b {
            lv.elev[2 * nn + c] = b;
        }
        lv.corners[c] = ready_a && ready_b;
        if ready_a || ready_b {
            draw.extend([(i + 1, j - 1), (i + 1, j), (i, j + 1)].iter().filter_map(|&(x, y)| lv.idx(x, y)));
        }
    }
    draw.sort_unstable();
    draw.dedup();
    for c in draw {
        if lv.exists[c] && lv.built[c] && !lv.finished[c] {
            let (i, j) = lv.ij(c);
            lv.cc[c] = lv.corner_values(i, j);
            lv.finished[c] = [(i, j), (i - 1, j + 1), (i - 1, j), (i, j - 1)]
                .iter()
                .all(|&(a, b)| lv.idx(a, b).is_none_or(|d| !lv.exists[d] || lv.corners[d]));
        }
    }
}

/// The level above's prediction surface at every vertex of a mask, and the
/// carved and dry ground's at each: a centre's own, a corner the mean of
/// the cells sharing it.
fn ground_fields(lv: &mut Level, par: &Level, mask: &Mask) {
    let nn = lv.cap();
    for c in mask.iter() {
        if !lv.exists[c] {
            continue;
        }
        let (i, j) = lv.ij(c);
        for t in 0..3u8 {
            let v = t as usize * nn + c;
            if lv.psurf[v].is_nan() {
                let (q3, r3) = pos3(t, i, j, lv.w);
                lv.psurf[v] = par.surface_at(q3 as f64 / 3.0, r3 as f64 / 3.0);
            }
        }
        if lv.supplied[c] {
            lv.gelev[c] = lv.carved[c];
            lv.gdry[c] = lv.ground[c];
            let (a, b, ready_a, ready_b) = lv.corner_means(i, j, &lv.carved, &lv.supplied);
            let (da, db, _, _) = lv.corner_means(i, j, &lv.ground, &lv.supplied);
            if ready_a {
                lv.gelev[nn + c] = a;
                lv.gdry[nn + c] = da;
            }
            if ready_b {
                lv.gelev[2 * nn + c] = b;
                lv.gdry[2 * nn + c] = db;
            }
        }
    }
}
