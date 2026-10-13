//! The terrain ladder (`design/ladder.md`): the ground built on hex levels,
//! each three times finer than the one above, down to the tiles, a level
//! answered on request one cell at a time.
//!
//! A request names a level and its cells. Each pass at a level deforms in
//! cells of its own size, read with one ring: the pass declares how far its
//! work reads, and its deform cell is the smallest ladder width whose ring
//! clears that ([`geom::ring_clearance`]). A deform reads its cell and ring
//! at its level, the passes before it there, and the level above under the
//! same footprint and one ring of the level above's cells; it writes its
//! own cell only. Deform cascades upward to the top, built once over the
//! whole domain, its edge and the sea the outlet, and kept.
//!
//! Below the top a level runs its passes in order:
//!
//! - **ground**: the level above's surface predicted at each centre, and
//!   every layer's change there: each layer's value at the cell less its
//!   own value predicted from the level above.
//! - **lines**: each trough of the level above routed again on the ground,
//!   inside a corridor [`CORRIDOR`] cells either side, pinned where the level
//!   above's line has a source, a junction or an end, or crosses a route
//!   cell; between pins it goes downhill, never farther from its end.
//! - **water**: one target surface, the valley drainage's law cuts: a floor
//!   along each line, walls rising from it at water's slope, steeper in hard
//!   rock, out to where they meet the ground. A line's floor is the ground
//!   less a share of its height above base level growing with catchment,
//!   the plate's age and the rock's erodibility ([`drained`]).
//! - **route**: every vertex's receiver on the ground, every trough's
//!   catchment and floor, and the troughs born at the level.
//!
//! Water comes last and reads the level's lines and its births, never the
//! other way: routing reads only the ground.
//!
//! Lines, routing and water's valleys stop at [`LAST_WATER`]. A level finer
//! than that is its prediction and the ground's own detail, its cut handed
//! down unchanged. The tiles cut channels along [`LAST_WATER`]'s rivers,
//! drawn from its routing when they read it ([`channel`]).
//!
//! Every layer, water among them, writes its value averaged over the
//! level's cell; its raise there is that less what the level above handed
//! down, the level above's values predicted at the cell ([`handed_down`]).
//! Water's value is its cut below the ground its layers make: the valley
//! drawn from the level's own lines, met by that ground, averaged over the
//! cell, less the ground. A wall far from the channel is a plane, which
//! averages to itself, so it belongs to the coarse levels, which hold it
//! within a few cells; a level below writes its own valley near its lines
//! and hands the rest down unchanged. A level is
//! that ground and that cut, rounded to whole steps. Nothing makes it
//! walkable: a step past walkable is a face.
//!
//! Every value is a function of its position: what a level keeps is filled
//! as requests reach it, and never read in place of a recomputation that
//! would differ.

pub mod channel;
pub mod geom;
#[cfg(feature = "inspect")]
pub mod ground;
mod lines;
mod route;
mod top;
mod water;

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use geom::*;

/// How the water layer cuts.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The least catchment a trough carries, in tiles, and the catchment at
    /// which a channel begins to cut, in the softest rock.
    pub channel_head: f64,
    /// The catchment at which a trough's bed is a trunk's, in tiles.
    pub catchment_full: f64,
    /// The catchment at which a channel begins to cut, in the softest rock,
    /// and at which it has cut its full share, in tiles.
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
        Params { channel_head: 1500.0, catchment_full: 40_000.0, cut_from: 13_122.0, cut_full: 1_102_500.0, wall: 1.0, ring: RING, water: true }
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
/// How far inside the ring water's correction fades to nothing, in cells
/// of the level, so it never stops dead.
pub const FADE: f64 = 3.0;
/// The least erodible rock, whose walls stand at [`WALL_HARDEST`].
pub const HARDEST: f64 = 0.3;
/// The steepest a wall stands, in the hardest rock, in steps per tile.
pub const WALL_HARDEST: f64 = 3.0;
/// How far upstream a trough born at a level may gather, in its cells.
pub const ROUTE_REACH: f64 = 11.0;
/// A line's corridor, either side of the level above's line, in cells of
/// the level: one cell of the level above.
pub const CORRIDOR: f64 = 3.0;
/// The finest level that routes, draws lines and cuts water's valleys.
pub const LAST_WATER: i64 = 27;

/// The stages a level's work is timed by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// Each layer's value read from the ground.
    Read = 0,
    /// The prediction and the layers' changes.
    Supply = 1,
    Lines = 2,
    Route = 3,
    Water = 4,
    /// Heights rounded.
    Build = 5,
    /// Corners met and drawn.
    Corners = 6,
    /// Channels: drawn at [`LAST_WATER`], handed down, cut at the tiles.
    Channel = 7,
}

pub const STAGES: [&str; 8] = ["read", "supply", "lines", "route", "water", "build", "corners", "channel"];

/// The passes that deform in cells of their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pass {
    Ground = 0,
    Water = 1,
    Route = 2,
    /// The channels, cut at the tiles as far as they reach.
    Channel = 3,
}

const PASSES: [Pass; 4] = [Pass::Ground, Pass::Water, Pass::Route, Pass::Channel];

/// The catchment a trough carries at a level: a level resolves a trough two
/// of its cells wide.
pub fn head_of(w: i64, p: &Params) -> f64 {
    p.channel_head.max(2.0 * (w * w) as f64)
}

/// How far from a cell a pass's work there reads, in tiles.
pub fn reach(pass: Pass, w: i64) -> f64 {
    w as f64
        * match pass {
            Pass::Ground => 0.0,
            Pass::Water => RING,
            Pass::Route => ROUTE_REACH,
            Pass::Channel if w == 1 => channel::channel_reach(),
            Pass::Channel => 0.0,
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
/// discharge: a hard rock needs proportionally more water before a channel
/// gathers. Nothing below the head.
pub fn growth(catchment: f64, erodibility: f64, p: &Params) -> f64 {
    let head = p.cut_from / erodibility.max(0.05);
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
    pub lo: i64,
    pub n: usize,
    pub exists: Vec<bool>,
    /// Each layer's value at each cell, NaN until read.
    pub layers: Vec<Vec<f64>>,
    pub erod: Vec<f64>,
    pub age: Vec<f64>,
    /// The level above's surface at each centre.
    pub p: Vec<f64>,
    /// The ground: the prediction and every layer's change but water's.
    pub ground: Vec<f64>,
    /// Water's value as the level above handed it down, predicted at each
    /// centre: what of the prediction is the level above's cut.
    pub wpred: Vec<f64>,
    /// Water's value: its cut below the uncut ground, never above it.
    pub cut: Vec<f64>,
    /// Water's correction before it fades: the level's valley averaged over
    /// the cell less what the level above handed down. NaN where none is
    /// made.
    pub wfull: Vec<f64>,
    /// How far each cell lies from the line governing its valley, in its
    /// cells; infinite past the ring.
    pub wdist: Vec<f64>,
    /// The width of the level the line governing each cell's valley was
    /// born at; -1 where none does.
    pub wborn: Vec<i64>,
    /// The river vertex whose valley governs each cell's correction, as an
    /// index of the level; -1 where none does.
    pub wgov: Vec<i64>,
    /// Whether any river water read at each cell runs where the correction
    /// fades and changes it.
    pub wfade: Vec<bool>,
    pub z: Vec<f64>,
    pub a3: Vec<[f64; 3]>,
    pub b3: Vec<[f64; 3]>,
    pub cc: Vec<[f64; 6]>,
    // Each vertex: a cell's centre, its corner A and its corner B.
    pub elev: Vec<f64>,
    pub psurf: Vec<f64>,
    pub gelev: Vec<f64>,
    /// The uncut ground at each vertex: a centre's own, a corner the mean of
    /// its sharers'. What drainage's floor is read from.
    pub uelev: Vec<f64>,
    /// The level water at each vertex runs down to: the sea, or the pit of
    /// the closed ground it drains into.
    pub base: Vec<f64>,
    /// The width of the level a trough at each vertex was born at; -1 off
    /// the troughs.
    pub born: Vec<i64>,
    pub rec: Vec<i64>,
    pub trough: Vec<bool>,
    pub acc: Vec<f64>,
    pub floor: Vec<f64>,
    pub line_on: Vec<bool>,
    pub line_rec: Vec<i64>,
    pub line_acc: Vec<f64>,
    pub line_fl: Vec<f64>,
    pub line_base: Vec<f64>,
    pub line_born: Vec<i64>,
    /// Where each line vertex lies along the level above's line, the point
    /// it was measured from, in world units a tile apart: where its river
    /// is, which decides whether water reads it.
    pub line_anchor: Vec<[f64; 2]>,
    /// The corridor a vertex was shared to: its segment's start, as a vertex
    /// index of the level.
    pub line_corr: Vec<i64>,
    pub line_dev: Vec<f64>,
    pub line_pin: Vec<f64>,
    pub line_wall: Vec<bool>,
    pub line_hollow: Vec<bool>,
    pub predicted: Vec<bool>,
    pub supplied: Vec<bool>,
    pub lined: Vec<bool>,
    pub watered: Vec<bool>,
    pub built: Vec<bool>,
    pub routed: Vec<bool>,
    pub corners: Vec<bool>,
    /// Whether each cell's six corner values are final: its own corners met,
    /// and the four its neighbours own, where those exist.
    pub finished: Vec<bool>,
    /// Each pass's deform cell of each cell.
    pub blocks: [Vec<(i64, i64)>; 4],
    /// Deforms run by each pass, and cells built.
    pub cost: [u64; 5],
}

const NONE: (i64, i64) = (i64::MIN, i64::MIN);

impl Level {
    fn new(w: i64, lo: i64, hi: i64, exists: Vec<bool>, top: bool, layers: usize, ring: f64) -> Self {
        let n = (hi - lo) as usize;
        let cells = n * n;
        let nan = |k: usize| vec![f64::NAN; k];
        let mut blocks: [Vec<(i64, i64)>; 4] = std::array::from_fn(|_| vec![NONE; cells]);
        for (b, pass) in PASSES.into_iter().enumerate() {
            let m = if top { 0 } else { (cell_size(pass, w, ring) / w).ilog(3) };
            for c in 0..cells {
                if exists[c] {
                    let (i, j) = (lo + (c / n) as i64, lo + (c % n) as i64);
                    blocks[b][c] = ancestor(i, j, m);
                }
            }
        }
        Level {
            w,
            lo,
            n,
            exists,
            layers: (0..layers).map(|_| nan(cells)).collect(),
            erod: nan(cells),
            age: nan(cells),
            p: nan(cells),
            ground: nan(cells),
            wpred: nan(cells),
            cut: nan(cells),
            wfull: nan(cells),
            wdist: nan(cells),
            wborn: vec![-1; cells],
            wgov: vec![-1; cells],
            wfade: vec![false; cells],
            z: nan(cells),
            a3: vec![[f64::NAN; 3]; cells],
            b3: vec![[f64::NAN; 3]; cells],
            cc: vec![[f64::NAN; 6]; cells],
            elev: nan(3 * cells),
            psurf: nan(3 * cells),
            gelev: nan(3 * cells),
            uelev: nan(3 * cells),
            base: nan(3 * cells),
            born: vec![-1; 3 * cells],
            rec: vec![-1; 3 * cells],
            trough: vec![false; 3 * cells],
            acc: vec![0.0; 3 * cells],
            floor: nan(3 * cells),
            line_on: vec![false; 3 * cells],
            line_rec: vec![-1; 3 * cells],
            line_acc: nan(3 * cells),
            line_fl: nan(3 * cells),
            line_base: nan(3 * cells),
            line_born: vec![-1; 3 * cells],
            line_anchor: vec![[f64::NAN; 2]; 3 * cells],
            line_corr: vec![-1; 3 * cells],
            line_dev: nan(3 * cells),
            line_pin: nan(3 * cells),
            line_wall: vec![false; 3 * cells],
            line_hollow: vec![false; 3 * cells],
            predicted: vec![false; cells],
            supplied: vec![false; cells],
            lined: vec![false; cells],
            watered: vec![false; cells],
            built: vec![false; cells],
            routed: vec![false; cells],
            corners: vec![false; cells],
            finished: vec![false; cells],
            blocks,
            cost: [0; 5],
        }
    }

    /// A cell's index, where the level holds it.
    pub fn idx(&self, i: i64, j: i64) -> Option<usize> {
        let (a, b) = (i - self.lo, j - self.lo);
        (a >= 0 && b >= 0 && (a as usize) < self.n && (b as usize) < self.n).then(|| a as usize * self.n + b as usize)
    }

    /// A cell's indices from its index.
    pub fn ij(&self, c: usize) -> (i64, i64) {
        (self.lo + (c / self.n) as i64, self.lo + (c % self.n) as i64)
    }

    /// A vertex's index, where the level holds its cell.
    pub fn vid(&self, t: u8, i: i64, j: i64) -> Option<usize> {
        self.idx(i, j).map(|c| t as usize * self.n * self.n + c)
    }

    /// A vertex's kind and cell from its index.
    pub fn vertex(&self, v: usize) -> (u8, i64, i64) {
        let nn = self.n * self.n;
        let (i, j) = self.ij(v % nn);
        ((v / nn) as u8, i, j)
    }

    /// The cell holding a vertex at a position in thirds of a tile.
    pub fn vid_at(&self, q3: i64, r3: i64) -> Option<usize> {
        let (t, i, j) = vertex_at(q3, r3, self.w);
        self.vid(t, i, j)
    }

    fn height(&self, i: i64, j: i64, of: &[f64], done: &[bool]) -> f64 {
        match self.idx(i, j) {
            Some(c) if self.exists[c] && done[c] => of[c],
            _ => f64::NAN,
        }
    }

    /// The pending deform cells of a pass over a mask.
    fn pending(&self, pass: Pass, mask: &[bool]) -> Vec<(i64, i64)> {
        let mut s: Vec<(i64, i64)> = mask.iter().enumerate().filter(|(_, &m)| m).map(|(c, _)| self.blocks[pass as usize][c]).collect();
        s.sort_unstable();
        s.dedup();
        s.retain(|&b| b != NONE);
        s
    }

    /// Cells whose deform cell of a pass is one of `pend` or in its ring.
    fn footprint(&self, pass: Pass, pend: &[(i64, i64)]) -> Vec<bool> {
        let mut keys = HashSet::new();
        for &(a, b) in pend {
            keys.insert((a, b));
            for (da, db) in NB {
                keys.insert((a + da, b + db));
            }
        }
        self.blocks[pass as usize].iter().map(|b| keys.contains(b)).collect()
    }

    /// For each pending deform cell of a pass, its own cells and its
    /// footprint's.
    fn block_lists(&self, pass: Pass, pend: &[(i64, i64)]) -> Vec<(Vec<usize>, Vec<usize>)> {
        let mut by: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (c, b) in self.blocks[pass as usize].iter().enumerate() {
            if *b != NONE {
                by.entry(*b).or_default().push(c);
            }
        }
        pend.iter()
            .map(|&(a, b)| {
                let own = by.get(&(a, b)).cloned().unwrap_or_default();
                let mut fp = own.clone();
                for (da, db) in NB {
                    if let Some(v) = by.get(&(a + da, b + db)) {
                        fp.extend(v);
                    }
                }
                (own, fp)
            })
            .collect()
    }

    /// Grows a mask by `k` rings of cells.
    fn dilate(&self, mask: &[bool], k: usize) -> Vec<bool> {
        let mut m = mask.to_vec();
        for _ in 0..k {
            let prev = m.clone();
            for c in 0..prev.len() {
                if !prev[c] {
                    continue;
                }
                let (i, j) = self.ij(c);
                for (di, dj) in NB {
                    if let Some(d) = self.idx(i + di, j + dj) {
                        m[d] = true;
                    }
                }
            }
        }
        m
    }

    /// The meet values at a cell's own corners A and B, from the heights the
    /// cells sharing each hold, NaN for a missing cell; each ready where every
    /// cell there that exists has its height.
    fn meet_at(&self, i: i64, j: i64, of: &[f64], done: &[bool]) -> ([f64; 3], [f64; 3], bool, bool) {
        let w = self.w as f64;
        let h = |a: i64, b: i64| self.height(a, b, of, done);
        let present = |a: i64, b: i64| self.idx(a, b).is_some_and(|c| self.exists[c]);
        let ready = |cells: [(i64, i64); 3]| cells.iter().all(|&(x, y)| !present(x, y) || !h(x, y).is_nan());
        let (ca, cb) = ([(i, j), (i + 1, j), (i, j + 1)], [(i, j), (i + 1, j - 1), (i + 1, j)]);
        (meet(ca.map(|(x, y)| h(x, y)), w), meet(cb.map(|(x, y)| h(x, y)), w), ready(ca), ready(cb))
    }

    /// A cell's six corner values: its own A and B, and the four its
    /// neighbours own; its height where a corner has none.
    fn corner_values(&self, i: i64, j: i64) -> [f64; 6] {
        let get = |a: i64, b: i64, f: &dyn Fn(usize) -> f64| self.idx(a, b).map_or(f64::NAN, f);
        let z = self.idx(i, j).map_or(f64::NAN, |c| self.z[c]);
        let cc = [
            get(i, j, &|c| self.b3[c][0]),
            get(i, j, &|c| self.a3[c][0]),
            get(i - 1, j + 1, &|c| self.b3[c][1]),
            get(i - 1, j, &|c| self.a3[c][1]),
            get(i - 1, j, &|c| self.b3[c][2]),
            get(i, j - 1, &|c| self.a3[c][2]),
        ];
        cc.map(|v| if v.is_nan() { z } else { v })
    }

    /// The level's drawn surface at a position in tiles: the fan triangle of
    /// the cell under it, from the cell's centre to two of its corners.
    pub fn surface_at(&self, q: f64, r: f64) -> f64 {
        let (u, v) = (q / self.w as f64, r / self.w as f64);
        let (i, j) = cube_round(u, v);
        let Some(c) = self.idx(i, j) else { return f64::NAN };
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
        l0 * self.z[c] + l1 * self.cc[c][s] + l2 * self.cc[c][s1]
    }
}

/// The ladder: the top, built once, and the levels below on request.
pub struct Ladder<'g, G: Ground> {
    pub levels: Vec<Level>,
    ground: &'g G,
    pub p: Params,
    /// Per level: lines with no way through their corridor, lines stalled,
    /// segments whose pins lay outside the footprint.
    pub line_stats: Vec<[u64; 3]>,
    /// Per level: born troughs whose upstream reached the read edge, vertices
    /// that did, own vertices left undecided, own vertices in a cycle.
    pub route_stats: Vec<[u64; 4]>,
    /// The channel curves drawn, those whose floor rose, and the tiles a
    /// channel lowered.
    pub channel_stats: [u64; 3],
    /// Each tile a channel reaches, where it is below [`LAST_WATER`]'s drawn
    /// surface: how far from its governing bank it lies, in tiles, and how
    /// far below that surface the channel is, in steps.
    pub channel_reach: Vec<[f64; 2]>,
    /// The most a channel lowers a tile at the edge of the zone it cuts in.
    pub channel_edge: f64,
    /// The rivers whose walls reached [`channel::channel_reach`] still below
    /// [`LAST_WATER`]'s drawn surface, as vertices of its level: the tiles
    /// where they did, and the largest step left there.
    pub channel_capped: HashMap<(u8, i64, i64), (u64, f64)>,
    /// Per level, the wall time each stage took there, in seconds, nested
    /// work at other levels not counted.
    pub timing: Vec<[f64; 8]>,
}

impl<'g, G: Ground> Ladder<'g, G> {
    /// A ladder over `extents`, each level's width and its cells' index range
    /// `lo..hi` on both axes, coarsest first; the top built over all of its.
    pub fn new(ground: &'g G, extents: &[(i64, i64, i64)], p: Params) -> Self {
        let mut levels: Vec<Level> = Vec::new();
        for (k, &(w, lo, hi)) in extents.iter().enumerate() {
            let n = (hi - lo) as usize;
            let mut exists = vec![true; n * n];
            if k > 0 {
                let par = &levels[k - 1];
                for c in 0..n * n {
                    let (i, j) = (lo + (c / n) as i64, lo + (c % n) as i64);
                    let (pi, pj, _, _) = parent_of(i, j);
                    exists[c] = par.idx(pi, pj).is_some_and(|pc| par.exists[pc]);
                }
            }
            levels.push(Level::new(w, lo, hi, exists, k == 0, ground.layers(), p.ring));
        }
        let count = levels.len();
        let mut ladder = Ladder {
            levels,
            ground,
            p,
            line_stats: vec![[0; 3]; count],
            route_stats: vec![[0; 4]; count],
            channel_stats: [0; 3],
            channel_reach: Vec::new(),
            channel_edge: 0.0,
            channel_capped: HashMap::new(),
            timing: vec![[0.0; 8]; count],
        };
        ladder.build_top();
        ladder
    }

    /// Reads each layer's value at every cell of a mask not yet read.
    fn read_layers(&mut self, k: usize, mask: &[bool]) {
        let lv = &self.levels[k];
        let todo: Vec<usize> = (0..mask.len()).filter(|&c| mask[c] && lv.exists[c] && lv.layers[0][c].is_nan()).collect();
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
        let all = vec![true; self.levels[0].exists.len()];
        self.read_layers(0, &all);
        let p = self.p;
        let lv = &mut self.levels[0];
        for c in 0..lv.exists.len() {
            let h: f64 = lv.layers.iter().map(|l| l[c]).sum();
            lv.p[c] = h;
            lv.ground[c] = h;
            lv.wpred[c] = 0.0;
            lv.z[c] = (h + 0.5).floor();
        }
        lv.predicted.iter_mut().for_each(|x| *x = true);
        lv.supplied.iter_mut().for_each(|x| *x = true);
        lv.built.iter_mut().for_each(|x| *x = true);
        corners_over(lv, &all);
        top::route_top(lv, &p);
        lv.routed.iter_mut().for_each(|x| *x = true);
        // The top's own valleys, cut along its troughs into the ground it
        // routed over.
        let edges = water::trough_edges(lv, head_of(lv.w, &p));
        water::top_cut(lv, &edges, &p);
        for c in 0..lv.exists.len() {
            lv.z[c] = (lv.ground[c] + lv.cut[c] + 0.5).floor();
        }
        // The cut moved every height, so every corner is met again.
        lv.corners.fill(false);
        lv.finished.fill(false);
        corners_over(lv, &all);
    }

    /// Each requested cell's level k routed and built, and drawn: its
    /// corners, which its neighbours' heights decide.
    pub fn request(&mut self, k: usize, cells: &[(i64, i64)]) {
        let lv = &self.levels[k];
        let mut mask = vec![false; lv.exists.len()];
        for &(i, j) in cells {
            if let Some(c) = lv.idx(i, j) {
                mask[c] = lv.exists[c];
            }
        }
        // A request for cells already finished reads what is kept.
        if k == 0 || (0..mask.len()).all(|c| !mask[c] || lv.finished[c]) {
            return;
        }
        self.ensure_routed(k, &mask);
        self.drawn(k, &mask);
    }

    /// Level k built two rings past a mask and its corners drawn one ring
    /// past it, so every corner of every cell of the mask is drawn.
    fn drawn(&mut self, k: usize, mask: &[bool]) {
        let lv = &self.levels[k];
        let (one, two) = (lv.dilate(mask, 1), lv.dilate(mask, 2));
        let one: Vec<bool> = one.iter().zip(&lv.exists).map(|(a, b)| *a && *b).collect();
        let two: Vec<bool> = two.iter().zip(&lv.exists).map(|(a, b)| *a && *b).collect();
        self.ensure_built(k, &two);
        let started = Instant::now();
        corners_over(&mut self.levels[k], &one);
        self.timing[k][Stage::Corners as usize] += started.elapsed().as_secs_f64();
    }

    fn ensure_routed(&mut self, k: usize, mask: &[bool]) {
        if k == 0 || self.levels[k].w < LAST_WATER {
            return;
        }
        let lv = &self.levels[k];
        let need: Vec<bool> = (0..mask.len()).map(|c| mask[c] && !lv.routed[c] && lv.exists[c]).collect();
        if !need.iter().any(|&x| x) {
            return;
        }
        let pend = lv.pending(Pass::Route, &need);
        let fp = lv.footprint(Pass::Route, &pend);
        self.ensure_lines(k, &fp);
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
    /// built and drawn.
    fn parent_over(&mut self, k: usize, mask: &[bool]) {
        let (lv, par) = (&self.levels[k], &self.levels[k - 1]);
        let mut pmask = vec![false; par.exists.len()];
        for c in 0..mask.len() {
            if mask[c] {
                let (i, j) = lv.ij(c);
                let (pi, pj, _, _) = parent_of(i, j);
                if let Some(pc) = par.idx(pi, pj) {
                    pmask[pc] = true;
                }
            }
        }
        let pmask: Vec<bool> = par.dilate(&pmask, 1).iter().zip(&par.exists).map(|(a, b)| *a && *b).collect();
        self.read_layers(k - 1, &pmask);
        if k - 1 > 0 {
            self.ensure_routed(k - 1, &pmask);
            self.drawn(k - 1, &pmask);
        }
    }

    fn ensure_supplied(&mut self, k: usize, mask: &[bool]) {
        let lv = &self.levels[k];
        let need: Vec<bool> = (0..mask.len()).map(|c| mask[c] && !lv.supplied[c] && lv.exists[c]).collect();
        if !need.iter().any(|&x| x) {
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
        for c in 0..fp.len() {
            if !fp[c] || !lv.exists[c] || lv.supplied[c] {
                continue;
            }
            let (i, j) = lv.ij(c);
            let (pred, layer_pred) = predict(par, i, j);
            lv.p[c] = pred;
            let layers = lv.layers.len();
            let change: f64 = (0..layers).map(|l| lv.layers[l][c] - layer_pred[l]).sum();
            lv.ground[c] = pred + change;
            lv.wpred[c] = layer_pred[layers];
            lv.predicted[c] = true;
            lv.supplied[c] = true;
        }
        lv.cost[Pass::Ground as usize] += pend.len() as u64;
        self.timing[k][Stage::Supply as usize] += started.elapsed().as_secs_f64();
    }

    fn ensure_lines(&mut self, k: usize, mask: &[bool]) {
        let lv = &self.levels[k];
        let need: Vec<bool> = (0..mask.len()).map(|c| mask[c] && !lv.lined[c] && lv.exists[c]).collect();
        if !need.iter().any(|&x| x) {
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
        let stats = lines::line_blocks(lv, &above[k - 1], &pend, &fp);
        for (a, b) in self.line_stats[k].iter_mut().zip(stats) {
            *a += b;
        }
        self.timing[k][Stage::Lines as usize] += started.elapsed().as_secs_f64();
    }

    fn ensure_built(&mut self, k: usize, mask: &[bool]) {
        let lv = &self.levels[k];
        let need: Vec<bool> = (0..mask.len()).map(|c| mask[c] && !lv.built[c] && lv.exists[c]).collect();
        if !need.iter().any(|&x| x) {
            return;
        }
        if lv.w < LAST_WATER {
            self.ensure_dry(k, &need);
            return;
        }
        let lv = &self.levels[k];
        let sel: Vec<bool> = (0..need.len()).map(|c| need[c] && !lv.watered[c]).collect();
        if sel.iter().any(|&x| x) {
            let pend = lv.pending(Pass::Water, &sel);
            let fp = lv.footprint(Pass::Water, &pend);
            self.ensure_supplied(k, &fp);
            self.ensure_routed(k, &fp);
            let started = Instant::now();
            let head = head_of(self.levels[k - 1].w, &self.p);
            let lv = &mut self.levels[k];
            let mut own = water::line_edges(lv, &fp, head);
            own.extend(water::born_edges(lv, &fp));
            water::level_cut(lv, &own, &pend, &self.p);
            lv.cost[Pass::Water as usize] += pend.len() as u64;
            self.timing[k][Stage::Water as usize] += started.elapsed().as_secs_f64();
        }
        self.ensure_supplied(k, &need);
        let started = Instant::now();
        let lv = &mut self.levels[k];
        let mut built = 0;
        for c in 0..need.len() {
            if need[c] && lv.supplied[c] && lv.watered[c] {
                let uncut = lv.ground[c] - lv.wpred[c];
                lv.z[c] = (uncut + lv.cut[c] + 0.5).floor();
                lv.built[c] = true;
                built += 1;
            }
        }
        lv.cost[4] += built;
        assert!((0..need.len()).all(|c| !need[c] || lv.built[c]), "a built cell read something its deform did not cover");
        self.timing[k][Stage::Build as usize] += started.elapsed().as_secs_f64();
    }

    /// A level finer than [`LAST_WATER`] built over a mask: its ground, the
    /// cut handed down unchanged, and at the tiles the channels the level
    /// above holds, met by it.
    fn ensure_dry(&mut self, k: usize, need: &[bool]) {
        self.ensure_supplied(k, need);
        if self.levels[k].w == 1 {
            self.ensure_channels(k, need);
        }
        let started = Instant::now();
        let lv = &mut self.levels[k];
        let mut built = 0;
        for c in 0..need.len() {
            if need[c] && !lv.built[c] {
                lv.cut[c] = lv.wpred[c];
                lv.z[c] = (lv.ground[c] + 0.5).floor();
                lv.watered[c] = true;
                lv.built[c] = true;
                built += 1;
            }
        }
        lv.cost[4] += built;
        self.timing[k][Stage::Build as usize] += started.elapsed().as_secs_f64();
    }

    /// The tiles of a mask cut by their channels, a channel deform cell at a
    /// time: [`LAST_WATER`]'s rivers under its footprint and one ring of that
    /// level's cells drawn, routed a ring past those so each curve's
    /// neighbours are read, and each tile the lower of its ground and the
    /// channels below [`LAST_WATER`]'s drawn surface there.
    fn ensure_channels(&mut self, k: usize, need: &[bool]) {
        let lv = &self.levels[k];
        let pend = lv.pending(Pass::Channel, need);
        let fp = lv.footprint(Pass::Channel, &pend);
        let s = self.levels.iter().position(|l| l.w == LAST_WATER).expect("a level water stops at");
        let (src, up) = (&self.levels[s], (k - s) as u32);
        let mut under = vec![false; src.exists.len()];
        for c in (0..fp.len()).filter(|&c| fp[c]) {
            let (i, j) = lv.ij(c);
            let (a, b) = ancestor(i, j, up);
            if let Some(d) = src.idx(a, b) {
                under[d] = true;
            }
        }
        let keep = |m: Vec<bool>| -> Vec<bool> { m.iter().zip(&src.exists).map(|(a, b)| *a && *b).collect() };
        let (draw, read) = (keep(src.dilate(&under, 1)), keep(src.dilate(&under, 2)));
        self.ensure_routed(s, &read);
        let started = Instant::now();
        let lv = &self.levels[k];
        let cells: Vec<usize> = (0..need.len()).filter(|&c| need[c] && !lv.built[c]).collect();
        let reached = {
            let (lv, src, p) = (&self.levels[k], &self.levels[s], &self.p);
            let reach = channel::channel_reach();
            let owners: Vec<usize> = (0..draw.len()).filter(|&c| draw[c]).collect();
            let drawn = water::in_parallel(&owners, |c| channel::own_banks(src, c, p));
            for (_, curves, rising) in &drawn {
                self.channel_stats[0] += curves;
                self.channel_stats[1] += rising;
            }
            let index = channel::Index::new(drawn.into_iter().flat_map(|(banks, _, _)| banks), reach);
            water::in_parallel(&cells, |c| {
                let (i, j) = lv.ij(c);
                index.at(i as f64, j as f64, src.surface_at(i as f64, j as f64), reach)
            })
        };
        // The zone a channel cuts in, and how much it lowers each tile there.
        let lowered: HashMap<usize, f64> = cells
            .iter()
            .zip(&reached)
            .filter(|(_, at)| at.governing.is_some())
            .map(|(&c, at)| (c, (self.levels[k].ground[c] - at.surface).max(0.0)))
            .collect();
        let done: HashSet<usize> = cells.iter().copied().collect();
        for (&c, &lower) in &lowered {
            let (i, j) = self.levels[k].ij(c);
            let edge = NB.iter().filter_map(|&(di, dj)| self.levels[k].idx(i + di, j + dj)).any(|d| done.contains(&d) && !lowered.contains_key(&d));
            if edge {
                self.channel_edge = self.channel_edge.max(lower);
            }
        }
        for (&c, at) in cells.iter().zip(&reached) {
            let Some((river, d)) = at.governing else { continue };
            self.channel_reach.push([d, at.below]);
            let step = self.levels[k].ground[c] - at.surface;
            self.channel_stats[2] += (step > 0.0) as u64;
            if at.capped {
                let e = self.channel_capped.entry(self.levels[s].vertex(river)).or_insert((0, 0.0));
                e.0 += 1;
                e.1 = e.1.max(step.max(0.0));
            }
        }
        let lv = &mut self.levels[k];
        for (&c, at) in cells.iter().zip(&reached) {
            let ground = lv.ground[c];
            let low = ground.min(at.surface);
            lv.cut[c] = lv.wpred[c] + low - ground;
            lv.z[c] = (low + 0.5).floor();
            lv.watered[c] = true;
            lv.built[c] = true;
        }
        lv.cost[4] += cells.len() as u64;
        lv.cost[Pass::Channel as usize] += pend.len() as u64;
        self.timing[k][Stage::Channel as usize] += started.elapsed().as_secs_f64();
    }

    /// For the probes: the cut at each of `cells` of level k from every river
    /// the level has routed, however far, where its water reads only its
    /// ring.
    #[cfg(feature = "inspect")]
    pub fn unlimited_cut(&self, k: usize, cells: &[(i64, i64)]) -> Vec<f64> {
        let lv = &self.levels[k];
        let mut edges = water::line_edges(lv, &lv.routed, head_of(self.levels[k - 1].w, &self.p));
        edges.extend(water::born_edges(lv, &lv.routed));
        let at: Vec<usize> = cells.iter().map(|&(i, j)| lv.idx(i, j).expect("a cell the level holds")).collect();
        water::whole_cut(lv, &edges, &at, &self.p)
    }
}

/// Whether troughs are born at a level: only where its head is below the
/// level above's.
fn births(par: &Level, lv: &Level, p: &Params) -> bool {
    head_of(lv.w, p) < head_of(par.w, p)
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

/// A child cell's prediction from the level above's drawn surface, and each
/// layer's own value predicted from the level above's, water's cut last: at
/// the parent's centre, a fan's centroid, or a corner. A layer's corner is
/// the mean of the cells sharing it; the surface's, the median of their
/// meet values there.
fn predict(par: &Level, i: i64, j: i64) -> (f64, Vec<f64>) {
    let (a, b, oi, oj) = parent_of(i, j);
    let pc = par.idx(a, b).expect("a parent held by the level above");
    let zc = par.z[pc];
    let median = |v: [f64; 3]| -> f64 {
        let mut s: Vec<f64> = v.into_iter().filter(|x| !x.is_nan()).collect();
        s.sort_by(f64::total_cmp);
        let m = s.len();
        if m % 2 == 1 { s[m / 2] } else { (s[m / 2 - 1] + s[m / 2]) / 2.0 }
    };
    let drawn = match (oi, oj) {
        (0, 0) => zc,
        (1, 1) => median(par.a3[pc]),
        (2, -1) => median(par.b3[pc]),
        _ => {
            let (ca, cb) = axis_fan(oi, oj).expect("an axis child");
            (zc + par.cc[pc][ca] + par.cc[pc][cb]) / 3.0
        }
    };
    let fields = par.layers.iter().map(|l| l.as_slice()).chain([par.cut.as_slice()]);
    let layers = fields.map(|f| handed_down(i, j, |x, y| par.idx(x, y).map_or(f64::NAN, |c| f[c]))).collect();
    (drawn, layers)
}

/// Corner meet values and routing heights for every cell of a mask whose
/// sharers are all built, and the six corner values of each cell, and of
/// every cell outside the mask reading a corner met here, so a cell's
/// corners never wait on a request that names it.
fn corners_over(lv: &mut Level, mask: &[bool]) {
    let nn = lv.n * lv.n;
    // Every cell of the mask, and every cell reading a corner met here.
    let mut draw: Vec<usize> = Vec::new();
    for c in 0..mask.len() {
        if !mask[c] || !lv.exists[c] || !lv.built[c] {
            continue;
        }
        draw.push(c);
        if lv.corners[c] {
            continue;
        }
        let (i, j) = lv.ij(c);
        let (a, b, ready_a, ready_b) = lv.meet_at(i, j, &lv.z, &lv.built);
        lv.elev[c] = lv.z[c];
        if ready_a {
            lv.a3[c] = a;
            lv.elev[nn + c] = lowest(a);
        }
        if ready_b {
            lv.b3[c] = b;
            lv.elev[2 * nn + c] = lowest(b);
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

/// The level above's surface at every vertex of a mask, and the ground at
/// each: a centre's own, a corner the lowest its sharers reach by the meet
/// rule.
fn ground_fields(lv: &mut Level, par: &Level, mask: &[bool]) {
    let nn = lv.n * lv.n;
    for c in 0..mask.len() {
        if !mask[c] || !lv.exists[c] {
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
            lv.gelev[c] = lv.ground[c];
            let (a, b, ready_a, ready_b) = lv.meet_at(i, j, &lv.ground, &lv.supplied);
            if ready_a {
                lv.gelev[nn + c] = lowest(a);
            }
            if ready_b {
                lv.gelev[2 * nn + c] = lowest(b);
            }
            lv.uelev[c] = lv.ground[c] - lv.wpred[c];
            for t in 1..3u8 {
                let (cells, _) = vertex_cells(t, i, j);
                let uncut: Vec<f64> = cells
                    .iter()
                    .filter_map(|&(x, y)| lv.idx(x, y))
                    .filter(|&d| lv.exists[d])
                    .map(|d| if lv.supplied[d] { lv.ground[d] - lv.wpred[d] } else { f64::NAN })
                    .collect();
                if uncut.iter().all(|u| !u.is_nan()) {
                    lv.uelev[t as usize * nn + c] = uncut.iter().sum::<f64>() / uncut.len() as f64;
                }
            }
        }
    }
}

/// The lowest of a corner's values, where water stands at the foot of a face.
fn lowest(v: [f64; 3]) -> f64 {
    v.iter().copied().filter(|x| !x.is_nan()).fold(f64::INFINITY, f64::min)
}
