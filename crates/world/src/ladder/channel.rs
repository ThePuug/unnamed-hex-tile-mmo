//! The tiles' rivers. At [`LAST_WATER`] each river is drawn as a smooth
//! curve through the midpoints of its edges, bending at its vertices and
//! meandering within [`MEANDER`] tiles of it, and sampled into banks: a
//! bed as wide as its catchment makes it, at its floor, with walls rising at
//! water's slope in the rock at its river vertex. The levels below hand the
//! banks down unchanged, each cell holding those lying in it, and the tiles
//! cut each bank's channel until its walls meet the ground, or
//! [`channel_reach`] tiles out.
//!
//! A vertex owns the curve through it, from the midpoint of each edge in
//! to the midpoint of its edge out, and a vertex the rivers end at owns
//! their last half edges: every bank lies within half an edge and the
//! meander of the vertex owning it, so a cell's banks are its own and its
//! ring's work. A curve's floor runs between its vertices' floors the way
//! its position does, so it falls wherever they fall. The meander is read
//! from where the curve runs, so a river bends alike whichever request
//! draws it.

use std::collections::HashMap;

use super::geom::{cube_round, pos3, ring_clearance, vnbr};
use super::water::bed;
use super::{head_of, wall_slope, Level, Params, LAST_WATER};
use crate::noise::simplex_2d;
use crate::SQRT_3;

/// How far a river wanders from its curve, in tiles.
pub const MEANDER: f64 = 2.0;
/// How far apart a river's bends lie, in tiles.
const MEANDER_WAVE: f64 = 24.0;
const MEANDER_SEED: u64 = 0x4d65_616e_6465_7273;
/// The tiles' channel deform cell, in tiles: one ring of an 81-wide cell
/// clears 46.8 tiles, and in soft rock a channel's wall lies on the valley
/// wall its level drew, so it reaches as far as that wall runs.
pub const CHANNEL_CELL: i64 = 243;
/// How wide the buckets a tile's channels are searched in are, in tiles.
const BUCKET: f64 = 27.0;

/// The farthest a channel reaches from a bank, in tiles, its bed's half
/// width and its walls together: what one ring of [`CHANNEL_CELL`] clears.
/// A wall still below the ground there stops.
pub fn channel_reach() -> f64 {
    ring_clearance(CHANNEL_CELL)
}

/// A point a curve is drawn through: its position in world units a tile
/// apart, its floor and its catchment.
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    floor: f64,
    acc: f64,
}

impl Point {
    fn mid(a: Point, b: Point) -> Point {
        Point { x: (a.x + b.x) / 2.0, y: (a.y + b.y) / 2.0, floor: (a.floor + b.floor) / 2.0, acc: (a.acc + b.acc) / 2.0 }
    }
}

/// A point of a channel's bed: its position in world units a tile apart,
/// its floor, its half width, its walls' slope, and the river vertex of
/// [`LAST_WATER`]'s level that owns it.
#[derive(Clone, Copy, Debug)]
pub struct Bank {
    pub x: f64,
    pub y: f64,
    pub floor: f64,
    pub bed: f64,
    pub slope: f64,
    pub river: usize,
}

fn world(q: f64, r: f64) -> (f64, f64) {
    (1.5 * q / SQRT_3, r + q / 2.0)
}

/// The cell of a level `w` wide a point in world units lies in.
fn cell_of(x: f64, y: f64, w: f64) -> (i64, i64) {
    let q = x * SQRT_3 / 1.5;
    cube_round(q / w, (y - q / 2.0) / w)
}

/// A river vertex of the source level, with where it runs next and its
/// floor and catchment: a line carrying the head of the level above it, or
/// a trough born at the level, either on land.
fn river(src: &Level, v: usize, p: &Params) -> Option<(usize, f64, f64)> {
    if src.line_on[v] {
        let ok = src.line_rec[v] >= 0 && src.line_acc[v] >= head_of(3 * src.w, p) && src.line_fl[v].is_finite();
        ok.then(|| (src.line_rec[v] as usize, src.line_fl[v], src.line_acc[v]))
    } else if src.trough[v] && src.rec[v] >= 0 && src.floor[v].is_finite() && src.gelev[v] >= 0.0 {
        Some((src.rec[v] as usize, src.floor[v], src.acc[v]))
    } else {
        None
    }
}

/// The banks a cell of [`LAST_WATER`]'s level owns, which must be routed
/// over the cell and its ring, and how many curves were drawn and how many
/// of those rose.
pub(super) fn own_banks(src: &Level, c: usize, p: &Params) -> (Vec<Bank>, u64, u64) {
    debug_assert_eq!(src.w, LAST_WATER);
    let nn = src.n * src.n;
    let at = |v: usize, floor: f64, acc: f64| {
        let (t, i, j) = src.vertex(v);
        let (q3, r3) = pos3(t, i, j, src.w);
        let (x, y) = world(q3 as f64 / 3.0, r3 as f64 / 3.0);
        Point { x, y, floor, acc }
    };
    let mut drawn = Drawn { banks: Vec::new(), curves: 0, rising: 0 };
    for t in 0..3 {
        let v = t * nn + c;
        let (_, i, j) = src.vertex(v);
        let ins: Vec<(usize, f64, f64)> = (0..6)
            .filter_map(|k| {
                let (tu, iu, ju) = vnbr(t as u8, i, j, k);
                let u = src.vid(tu, iu, ju)?;
                river(src, u, p).filter(|r| r.0 == v).map(|r| (u, r.1, r.2))
            })
            .collect();
        let e = src.erod[c];
        let slope = wall_slope(if e.is_nan() { 1.0 } else { e }, p.wall);
        match river(src, v, p) {
            Some((x, fv, av)) => {
                let here = at(v, fv, av);
                let there = river(src, x, p).map_or(at(x, fv, av), |(_, fx, ax)| at(x, fx, ax));
                let out = Point::mid(here, there);
                if ins.is_empty() {
                    drawn.curve(here, here, out, slope, v, p);
                }
                for &(u, fu, au) in &ins {
                    drawn.curve(Point::mid(at(u, fu, au), here), here, out, slope, v, p);
                }
            }
            // The rivers ending here run on to it at their own floors.
            None => {
                for &(u, fu, au) in &ins {
                    let end = at(v, fu, au);
                    let from = Point::mid(at(u, fu, au), end);
                    let ue = src.erod[u % nn];
                    let slope = wall_slope(if ue.is_nan() { 1.0 } else { ue }, p.wall);
                    drawn.curve(from, Point::mid(from, end), end, slope, u, p);
                }
            }
        }
    }
    (drawn.banks, drawn.curves, drawn.rising)
}

struct Drawn {
    banks: Vec<Bank>,
    curves: u64,
    rising: u64,
}

impl Drawn {
    /// A curve from `a` through control `b` to `c`, sampled no more than
    /// half a tile apart and meandered across its course.
    fn curve(&mut self, a: Point, b: Point, c: Point, slope: f64, river: usize, p: &Params) {
        let len = (b.x - a.x).hypot(b.y - a.y) + (c.x - b.x).hypot(c.y - b.y);
        let ns = 2usize.max((len / 0.5).ceil() as usize + 1);
        let mut last = f64::INFINITY;
        let mut rose = false;
        for s in 0..ns {
            let t = s as f64 / (ns - 1) as f64;
            let (u, m, v) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t);
            let x = u * a.x + m * b.x + v * c.x;
            let y = u * a.y + m * b.y + v * c.y;
            let floor = u * a.floor + m * b.floor + v * c.floor;
            let acc = u * a.acc + m * b.acc + v * c.acc;
            let (tx, ty) = ((1.0 - t) * (b.x - a.x) + t * (c.x - b.x), (1.0 - t) * (b.y - a.y) + t * (c.y - b.y));
            let tl = tx.hypot(ty);
            let (nx, ny) = if tl > 1e-12 { (-ty / tl, tx / tl) } else { (0.0, 0.0) };
            let wander = MEANDER * simplex_2d(x / MEANDER_WAVE, y / MEANDER_WAVE, MEANDER_SEED).clamp(-1.0, 1.0);
            rose |= floor > last + 1e-9;
            last = floor;
            self.banks.push(Bank { x: x + nx * wander, y: y + ny * wander, floor, bed: bed(acc, p), slope, river });
        }
        self.curves += 1;
        self.rising += rose as u64;
    }
}

/// The banks of `par`'s cells around a cell of `lv`, the level below it,
/// that lie in the cell: the cell's parent and its ring hold every one.
pub(super) fn handed_banks(lv: &Level, par: &Level, i: i64, j: i64) -> Vec<Bank> {
    let (a, b, _, _) = super::geom::parent_of(i, j);
    let mut out = Vec::new();
    for (da, db) in std::iter::once((0, 0)).chain(super::geom::NB) {
        let Some(pc) = par.idx(a + da, b + db) else { continue };
        out.extend(par.banks[pc].iter().filter(|bk| cell_of(bk.x, bk.y, lv.w as f64) == (i, j)));
    }
    out
}

/// A tile's channels: their surface there, infinite where none reaches; and
/// where it is below the ground, how far off the bank governing it lies, and
/// whether that is at the reach, with the river.
pub(super) struct Reach {
    pub surface: f64,
    pub governing: Option<(usize, f64)>,
    pub capped: bool,
}

/// A bucket's banks, and what bounds their walls from below: the lowest
/// floor, gentlest slope and widest bed among them.
struct Held {
    banks: Vec<Bank>,
    floor: f64,
    slope: f64,
    bed: f64,
}

/// The banks the level above the tiles holds under a mask, bucketed for the
/// tiles' search, nearest buckets first.
pub(super) struct Index {
    buckets: HashMap<(i64, i64), Held>,
    order: Vec<(i64, i64)>,
}

impl Index {
    pub(super) fn new(par: &Level, mask: &[bool], reach: f64) -> Self {
        let mut buckets: HashMap<(i64, i64), Held> = HashMap::new();
        for c in (0..mask.len()).filter(|&c| mask[c]) {
            for b in &par.banks[c] {
                let held = buckets.entry(cell_of(b.x, b.y, BUCKET)).or_insert(Held { banks: Vec::new(), floor: f64::INFINITY, slope: f64::INFINITY, bed: 0.0 });
                held.banks.push(*b);
                held.floor = held.floor.min(b.floor);
                held.slope = held.slope.min(b.slope);
                held.bed = held.bed.max(b.bed);
            }
        }
        let k = (reach / BUCKET).ceil() as i64 + 2;
        let mut order: Vec<(i64, i64)> = (-k..=k).flat_map(|a| (-k..=k).map(move |b| (a, b))).collect();
        let far = |&(a, b): &(i64, i64)| {
            let (x, y) = world(a as f64 * BUCKET, b as f64 * BUCKET);
            x.hypot(y)
        };
        order.sort_by(|p, q| far(p).total_cmp(&far(q)));
        Index { buckets, order }
    }

    /// The channels at tile (q, r) over `ground`: the lowest of every
    /// bank's floor and the wall rising from its bed's edge, out to
    /// `reach`. A bucket whose banks cannot come below the lower of that and
    /// the ground is passed over.
    pub(super) fn at(&self, q: f64, r: f64, ground: f64, reach: f64) -> Reach {
        let (ci, cj) = cube_round(q / BUCKET, r / BUCKET);
        let (x, y) = world(q, r);
        let rad = BUCKET / SQRT_3;
        let mut best = f64::INFINITY;
        let mut governing: Option<(usize, f64)> = None;
        for &(di, dj) in &self.order {
            let Some(held) = self.buckets.get(&(ci + di, cj + dj)) else { continue };
            let (cx, cy) = world(((ci + di) as f64) * BUCKET, ((cj + dj) as f64) * BUCKET);
            let nearest = ((cx - x).hypot(cy - y) - rad).max(0.0);
            if nearest > reach || held.floor + held.slope * (nearest - held.bed).max(0.0) >= best.min(ground) {
                continue;
            }
            for b in &held.banks {
                let d = (b.x - x).hypot(b.y - y);
                if d <= reach {
                    let v = b.floor + b.slope * (d - b.bed).max(0.0);
                    if v < best {
                        best = v;
                        governing = Some((b.river, d));
                    }
                }
            }
        }
        let governing = governing.filter(|_| best < ground);
        Reach { surface: best, governing, capped: governing.is_some_and(|(_, d)| d > reach - 1.0) }
    }
}
