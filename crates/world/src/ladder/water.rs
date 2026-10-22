//! Water: one target surface, the valley drainage's law cuts: a channel
//! along each trough, as wide as its catchment makes it, its water standing
//! at the trough's floor and its bed cut [`depth`] below that, walls rising
//! from the bed at water's slope in the rock they stand in, the plain a
//! step above the water out to its own width ([`plain`]), and walls rising
//! from there to where they meet the ground. The water fills its valley to the floor:
//! out over the bed and up the walls to where they climb to it, so it
//! meets its banks at its own height.
//!
//! The top cuts its valley wholly, met by its ground at each vertex. A
//! level below carves its own troughs' valleys within `Params::ring` of its
//! cells into its carved ground, the level above's carve handed down, met
//! by that at each vertex. A valley is an absolute height, so where the
//! level above already cut a river to its floor the level's own valley of
//! it cuts nothing more.
//!
//! Water stands on a cell where a bed covers its centre and is as wide as
//! the cell, corner to corner: a river is wet at a level once its catchment
//! makes it that wide, so the finer the level, the smaller the rivers that
//! show. It stands at the lowest such bed's floor rounded, never above the
//! ground the river drains, and never more than [`depth`] over a cell
//! carved lower, so water stands on the carve, never over it. A cell under a lake takes
//! the lake's surface rounded where it stands higher, over the whole cell:
//! the lake is the one routing keeps over the cell's centre, its outlet cut
//! as far as erosion has carried it.

use std::collections::BTreeMap;

use super::geom::{cube_round, hex_offsets, parent_of, pos3, project, vnbr, CHILD_OFFSETS};
use super::mask::Mask;
use super::{wall_slope, Level, Params, Pass};
use crate::SQRT_3;

/// A trough edge the valley is read along: its ends' positions in tiles,
/// their floors, its flow and the width of the level its trough was
/// born at, whether it begins its river, and the edge its river runs on
/// along.
pub(super) struct Edge {
    from: (f64, f64),
    to: (f64, f64),
    floors: (f64, f64),
    flow: f64,
    born: i64,
    /// The erodibility of the rock at the vertex the edge leaves.
    erod: f64,
    /// The vertex the edge leaves, as an index of its level.
    src: usize,
    /// The half width of the plain at each end ([`plain`]), what a floor's
    /// width runs between along the edge.
    plains: (f64, f64),
    /// The graded slope of the channel the edge leaves, in steps a tile.
    fall: f64,
    /// The width of the level the edge is drawn at, which spaces its samples.
    w: i64,
    /// Whether no trough drains into the vertex the edge leaves.
    head: bool,
    /// The edge leaving the vertex this one reaches, where its river runs
    /// on and that vertex's receiver is routed.
    next: Option<Next>,
}

/// The edge a river runs on along: where it reaches, the floor and plain
/// there, and its own flow, birth, rock, vertex and fall.
struct Next {
    to: (f64, f64),
    floor: f64,
    plain: f64,
    flow: f64,
    born: i64,
    erod: f64,
    src: usize,
    fall: f64,
}

fn tiles(lv: &Level, v: usize) -> (f64, f64) {
    let (t, i, j) = lv.vertex(v);
    let (q3, r3) = pos3(t, i, j, lv.w);
    (q3 as f64 / 3.0, r3 as f64 / 3.0)
}

/// A level's troughs over a mask, each edge to its receiver. The mask's
/// cells and the next two rings must be routed, so each edge knows
/// whether its river begins there and where it runs on.
pub(super) fn trough_edges(lv: &Level, mask: &Mask, p: &Params) -> Vec<Edge> {
    let nn = lv.cap();
    let runs = |v: usize| lv.trough[v] && lv.rec[v] >= 0;
    let head = |v: usize| {
        let (t, i, j) = lv.vertex(v);
        !(0..6).filter_map(|k| {
            let (tt, a, b) = vnbr(t, i, j, k);
            lv.vid(tt, a, b)
        })
        .any(|u| runs(u) && lv.rec[u] == v as i64)
    };
    // A vertex's plain, held to its room where routing measured one.
    let plain_at = |u: usize| {
        let half = bed(lv.flow[u], p);
        let room = lv.room[u];
        let wide = plain(half, lv.acc[u], rock(lv, u), lv.age[u % nn], p);
        if room.is_nan() { wide } else { wide.min(room.max(half)) }
    };
    (0..3)
        .flat_map(|t| mask.iter().map(move |c| t * nn + c))
        .filter(|&v| runs(v))
        .map(|v| {
            let to = lv.rec[v] as usize;
            let next = (runs(to) && lv.routed[lv.rec[to] as usize % nn]).then(|| {
                let on = lv.rec[to] as usize;
                Next {
                    to: tiles(lv, on),
                    floor: lv.floor[on].max(0.0),
                    plain: plain_at(on),
                    flow: lv.flow[to],
                    born: lv.born[to],
                    erod: rock(lv, to),
                    src: to,
                    fall: super::graded_slope(lv.acc[to], rock(lv, to), p),
                }
            });
            Edge {
                from: tiles(lv, v),
                to: tiles(lv, to),
                // The sea is every river's base level: a floor running out
                // over the sea floor stands at the sea.
                floors: (lv.floor[v].max(0.0), lv.floor[to].max(0.0)),
                flow: lv.flow[v],
                born: lv.born[v],
                erod: rock(lv, v),
                src: v,
                plains: (plain_at(v), plain_at(to)),
                fall: super::graded_slope(lv.acc[v], rock(lv, v), p),
                w: lv.w,
                head: head(v),
                next,
            }
        })
        .collect()
}

/// The erodibility of the rock under a vertex, its cell's.
fn rock(lv: &Level, v: usize) -> f64 {
    let e = lv.erod[v % (lv.cap())];
    if e.is_nan() { 1.0 } else { e }
}

/// A position in tiles as world units a tile apart.
fn world(q: f64, r: f64) -> (f64, f64) {
    (1.5 * q / SQRT_3, r + q / 2.0)
}

/// The half width of a trough's bed, in tiles: growing as the square root
/// of its flow, the way a channel's width grows with discharge, a tile and
/// a half at a trunk's.
pub fn bed(flow: f64, p: &Params) -> f64 {
    1.5 * (flow / p.catchment_full).max(0.0).sqrt()
}

/// The half width of a river's flat floor, in tiles: its bed's, times
/// how much gentler its graded fall is than a channel head's, times how
/// far its plate's age has carried erosion. A river cuts down as fast as
/// it falls and wanders sideways at a pace its fall does not slow, so a
/// steep one cuts a gorge no wider than its bed and a gentle one planes a
/// floor many beds wide.
pub fn plain(bed: f64, catchment: f64, erodibility: f64, age: f64, p: &Params) -> f64 {
    let gentler = super::HEAD_SLOPE / super::graded_slope(catchment, erodibility, p);
    bed * gentler.max(1.0) * crate::tectonic::aged(age)
}

/// How many half bends a trough edge winds in and how far its bed swings
/// to either side, in tiles, as a finer level reads it: none but where its
/// trough was born at the edge's level, since a river winds once and every
/// level below follows the valley it reads. A river's meander is eleven
/// channel widths long, so the edge bends a whole number of half meanders,
/// at least one, zero at both ends; it swings a quarter meander, no further
/// than its plain leaves room beside the bed, than half a bend's length or
/// than a cell of its level, the margin a reader routes it past its reach,
/// so a gorge, whose plain is its bed, runs straight. Which way the first
/// bend turns, and how far of that it swings, is drawn from the edge's
/// start. A level's own beds bend only at its vertices, which are too far
/// apart to show a meander.
fn meander(e: &Edge, length: f64, bed: f64, plain: f64) -> (u32, f64) {
    if e.born != e.w || length <= 0.0 {
        return (0, 0.0);
    }
    let wave = 11.0 * 2.0 * bed;
    let bows = ((length / (0.5 * wave)).round() as u32).max(1);
    let swing = (0.25 * wave).min(plain - bed).min(0.5 * length / bows as f64).min(e.w as f64);
    if swing <= 0.0 {
        return (0, 0.0);
    }
    let start = ((3.0 * e.from.0).round() as i64, (3.0 * e.from.1).round() as i64);
    let draw = crate::noise::hash_f64(start.0, start.1, MEANDER_SEED);
    // Which way it turns, and how far of its room it swings, half to all.
    let turn = if draw < 0.5 { 1.0 } else { -1.0 };
    let share = 0.5 + (2.0 * draw).fract() * 0.5;
    (bows, turn * share * swing)
}

const MEANDER_SEED: u64 = 0x4d65_616e_6465_7273; // "Meanders"

/// How deep a river's water stands, in levels, its channel cut that far
/// below its floor: a level, and a level more for each two tiles of its
/// half width past the first.
pub fn depth(bed: f64) -> f64 {
    1.0 + ((bed - 1.0) / 2.0).max(0.0).floor()
}

/// The water standing over the centre of a cell of a level `w` wide, at a
/// position in tiles, from the beds at least as wide as the cell, out to
/// its corners, whose water reaches it: over the bed, or up the wall to
/// where it climbs to the floor. The lowest such bed's floor rounded, its
/// depth, and the river vertex it stands over. None where none reaches.
fn standing(beds: &[Sample], q: f64, r: f64, w: i64) -> Option<(f64, f64, usize)> {
    let (x, y) = world(q, r);
    let corner = w as f64 / SQRT_3;
    beds.iter()
        .filter(|s| s.bed >= corner && (s.x - x).hypot(s.y - y) <= s.bed + depth(s.bed) / s.slope)
        .map(|s| ((s.floor + 0.5).floor(), depth(s.bed), s.src))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// How tall a cliff band stands in rock that holds a wall steeper than
/// the angle of repose, in steps: the thinnest formation of a rock column,
/// each holding its face over a shelf cut back into the weak bed below.
pub const CLIFF: f64 = 20.0;

/// A bed's valley at a position in world units a tile apart, as a level
/// `w` wide draws it, each distance to its stretch taken `nearer` tiles
/// shorter: the
/// channel, its walls rising from the bed, and its plain, a step above the
/// river's water out to the plain's width from the valley's axis with
/// walls rising from there, whichever stands lower, so the bed winds
/// within its plain and the plain's edge runs with the valley. A plain no
/// wider than the bed leaves the channel's walls running on up. A wall in
/// rock that holds steeper than repose rises as cliff bands [`CLIFF`] tall
/// over flat shelves, keeping its slope on the whole, where the level's
/// points are close enough to show a shelf; coarser, it rises evenly. It
/// never falls as either distance grows.
fn profile(s: &Sample, (x, y): (f64, f64), nearer: f64, w: i64) -> f64 {
    let to_stretch = |(fx, fy): (f64, f64), (dx, dy): (f64, f64)| {
        let (f, d) = project((x, y), (fx, fy), (fx + dx, fy + dy));
        (s.floor + f * s.down, (d * SQRT_3 - nearer).max(0.0))
    };
    let (floor, d) = to_stretch((s.x, s.y), s.on);
    let (level, off) = if (s.ax, s.ay) == (s.x, s.y) { (floor, d) } else { to_stretch((s.ax, s.ay), s.along) };
    let channel = floor - depth(s.bed) + s.slope * (d - s.bed).max(0.0);
    // The plain rises from its river faster than the river falls, so a
    // point of it stands on the river beside it, never on a floor far
    // downstream whose plain reaches it.
    let across = (2.0 * s.fall + 0.01) * off.min(s.plain);
    let rise = across + s.slope * (off - s.plain).max(0.0);
    let shelved = s.slope > 1.0 && CLIFF / s.slope >= 2.0 * w as f64;
    let rise = if shelved { CLIFF * (rise / CLIFF).floor() } else { rise };
    channel.min(level + 1.0 + rise)
}

/// The least [`profile`] over a disc `radius` tiles round a position: its
/// distance from the disc's nearest point, its floor wherever along the
/// stretch.
fn lowest_over(s: &Sample, at: (f64, f64), radius: f64, w: i64) -> f64 {
    profile(s, at, radius, w) - s.down.abs()
}

/// The most [`profile`] over a disc `radius` tiles round a position.
fn highest_over(s: &Sample, at: (f64, f64), radius: f64, w: i64) -> f64 {
    profile(s, at, -radius, w) + s.down.abs()
}

/// A point of a trough's bed and the stretch of it to the next point,
/// which it stands for, so the channel runs unbroken however far apart
/// the points lie: its position in world units a tile apart,
/// its floor, its half width, the width of the level its trough was born
/// at and of the level it was read from, and how far from a cell that
/// level's rivers are read: past `near`, out to `far`, in tiles, from the
/// point.
#[derive(Clone, Copy)]
struct Sample {
    x: f64,
    y: f64,
    /// The way to the next point of its bed.
    on: (f64, f64),
    /// Where its valley's axis lies beside it: the bed before its meander
    /// swings it, and the way to the axis beside the next point.
    ax: f64,
    ay: f64,
    along: (f64, f64),
    /// Its floor, and how far that falls to the next point's along the
    /// stretch.
    floor: f64,
    down: f64,
    bed: f64,
    /// The half width of its flat floor, the bed's or wider.
    plain: f64,
    /// How far the river falls a tile along its course at grade, in steps.
    fall: f64,
    born: i64,
    /// Its wall's slope: the rock's at its river vertex.
    slope: f64,
    /// The river vertex its edge leaves.
    src: usize,
    w: i64,
    near: f64,
    far: f64,
}

/// What a stretch of a river carries: its flow, birth, rock, vertex and
/// fall.
#[derive(Clone, Copy)]
struct River {
    flow: f64,
    born: i64,
    erod: f64,
    src: usize,
    fall: f64,
}

/// The way an edge's river is drawn, in world units a tile apart: from the
/// edge's middle round the vertex it reaches to the next edge's middle, a
/// curve with that vertex its control, and from the edge's start where
/// its river begins there. Where it runs on no further, to the vertex.
/// So a river bends at its vertices rather than turning at them, and the
/// ways of rivers meeting join where their trunk's begins. Within half a
/// step of its vertices.
struct Way {
    /// The head's straight run, its length, and the curve's points by
    /// length along it, what a distance along the way is found by.
    head: Option<((f64, f64), f64)>,
    curve: Vec<((f64, f64), f64, f64)>,
    length: f64,
    ends: [(f64, f64); 3],
    floors: [f64; 4],
    plains: [f64; 4],
    rivers: [River; 2],
}

impl Way {
    /// Points along the curve its length is measured by.
    const SPAN: usize = 16;

    fn of(e: &Edge) -> Self {
        let a = world(e.from.0, e.from.1);
        let b = world(e.to.0, e.to.1);
        let mid = |p: (f64, f64), q: (f64, f64)| ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
        let m1 = mid(a, b);
        let own = River { flow: e.flow, born: e.born, erod: e.erod, src: e.src, fall: e.fall };
        let (m2, f2, p2, on) = match &e.next {
            Some(n) => (mid(b, world(n.to.0, n.to.1)), (e.floors.1 + n.floor) / 2.0, (e.plains.1 + n.plain) / 2.0, River { flow: n.flow, born: n.born, erod: n.erod, src: n.src, fall: n.fall }),
            None => (b, e.floors.1, e.plains.1, own),
        };
        let head = e.head.then(|| (a, (m1.0 - a.0).hypot(m1.1 - a.1)));
        let start = head.map_or(0.0, |h| h.1);
        let mut curve = Vec::with_capacity(Self::SPAN + 1);
        let mut run = start;
        let mut last = m1;
        for k in 0..=Self::SPAN {
            let t = k as f64 / Self::SPAN as f64;
            let pt = quad(m1, b, m2, t);
            run += (pt.0 - last.0).hypot(pt.1 - last.1);
            curve.push((pt, t, run));
            last = pt;
        }
        // On a level floor its water runs on downstream, to where the
        // river carries more.
        let gathered = |flow: f64| -FLAT_FALL * flow.max(1.0).ln();
        let (g1, g2) = (gathered(own.flow), gathered(on.flow));
        Way {
            head,
            length: run,
            curve,
            ends: [m1, b, m2],
            floors: [e.floors.0 + g1, (e.floors.0 + e.floors.1) / 2.0 + g1, e.floors.1 + (g1 + g2) / 2.0, f2 + g2],
            plains: [e.plains.0, (e.plains.0 + e.plains.1) / 2.0, e.plains.1, p2],
            rivers: [own, on],
        }
    }

    /// The point `s` along the way, the unit normal to its left, and its
    /// floor, plain and river there.
    fn at(&self, s: f64) -> ((f64, f64), (f64, f64), (f64, f64, River)) {
        let normal = |d: (f64, f64)| {
            let l = d.0.hypot(d.1);
            if l > 0.0 { (-d.1 / l, d.0 / l) } else { (0.0, 0.0) }
        };
        let lerp = |a: f64, b: f64, f: f64| a + (b - a) * f;
        if let Some((a, len)) = self.head {
            if s < len {
                let f = s / len;
                let [m1, ..] = self.ends;
                let pt = (lerp(a.0, m1.0, f), lerp(a.1, m1.1, f));
                let at = (lerp(self.floors[0], self.floors[1], f), lerp(self.plains[0], self.plains[1], f), self.rivers[0]);
                return (pt, normal((m1.0 - a.0, m1.1 - a.1)), at);
            }
        }
        let k = self.curve.partition_point(|c| c.2 < s).clamp(1, Self::SPAN);
        let ((_, t0, s0), (_, t1, s1)) = (self.curve[k - 1], self.curve[k]);
        let t = if s1 > s0 { lerp(t0, t1, ((s - s0) / (s1 - s0)).clamp(0.0, 1.0)) } else { t1 };
        let [m1, b, m2] = self.ends;
        let pt = quad(m1, b, m2, t);
        let d = (2.0 * (1.0 - t) * (b.0 - m1.0) + 2.0 * t * (m2.0 - b.0), 2.0 * (1.0 - t) * (b.1 - m1.1) + 2.0 * t * (m2.1 - b.1));
        let at = if t <= 0.5 {
            let f = 2.0 * t;
            (lerp(self.floors[1], self.floors[2], f), lerp(self.plains[1], self.plains[2], f), self.rivers[0])
        } else {
            let f = 2.0 * t - 1.0;
            (lerp(self.floors[2], self.floors[3], f), lerp(self.plains[2], self.plains[3], f), self.rivers[1])
        };
        (pt, normal(d), at)
    }
}

/// How far a river's floor falls, in steps, for each e-fold its flow
/// grows: too little to draw, enough that water on a floor its river
/// holds level runs on downstream rather than by the lattice.
const FLAT_FALL: f64 = 1e-6;

/// The quadratic from `a` to `c` with control `b` at `t` in [0, 1].
fn quad(a: (f64, f64), b: (f64, f64), c: (f64, f64), t: f64) -> (f64, f64) {
    let (u, v, w) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t);
    (u * a.0 + v * b.0 + w * c.0, u * a.1 + v * b.1 + w * c.1)
}

/// One level's rivers: their beds sampled no more than half a cell apart,
/// each sample held by the cell of the level it falls in, in one order, so
/// a tie between samples falls the same way in every process; and how far
/// from a cell they are read, in tiles.
pub(super) struct Band {
    w: i64,
    far: f64,
    cells: BTreeMap<(i64, i64), Vec<Sample>>,
}

impl Band {
    /// The beds of `edges`, of a level `w` wide, read from a cell past
    /// `near` tiles and out to `far`, as the level `read` wide reads them.
    pub(super) fn new(w: i64, edges: &[Edge], near: f64, far: f64, read: i64, p: &Params) -> Self {
        let wf = w as f64;
        let mut cells: BTreeMap<(i64, i64), Vec<Sample>> = BTreeMap::new();
        for e in edges {
            let way = Way::of(e);
            // A level's own beds at half its width; a coarser level's at a
            // quarter of the plain they bound, so a floor's edge follows
            // its river, and never closer than half the reader's width or
            // two tiles, nor sparser than its own.
            let length = way.length;
            let (bows, swing) = if read < e.w { meander(e, length, bed(e.flow, p), e.plains.0.min(e.plains.1)) } else { (0, 0.0) };
            let mut spacing = if read >= e.w {
                0.5 * e.w as f64
            } else {
                (0.25 * e.plains.0.min(e.plains.1)).max(0.5 * read as f64).max(2.0).min(0.5 * e.w as f64)
            };
            if bows > 0 {
                spacing = spacing.min(length / (4.0 * bows as f64));
            }
            let ns = 2usize.max((length / spacing).ceil() as usize + 1);
            let points: Vec<_> = (0..ns)
                .map(|s| {
                    let f = s as f64 / (ns - 1) as f64;
                    let off = swing * (std::f64::consts::PI * bows as f64 * f).sin();
                    let ((ax, ay), (px, py), at) = way.at(f * length);
                    ((ax + px * off, ay + py * off), (ax, ay), at)
                })
                .collect();
            for (s, &((x, y), (ax, ay), at)) in points.iter().enumerate() {
                let (floor, plain, river) = at;
                let (on, along, down) = points.get(s + 1).map_or(((0.0, 0.0), (0.0, 0.0), 0.0), |&((nx, ny), (nax, nay), (next, _, _))| ((nx - x, ny - y), (nax - ax, nay - ay), next - floor));
                let q = x * 2.0 / SQRT_3;
                let r = y - q / 2.0;
                cells.entry(cube_round(q / wf, r / wf)).or_default().push(Sample {
                    x,
                    y,
                    on,
                    ax,
                    ay,
                    along,
                    floor,
                    down,
                    bed: bed(river.flow, p),
                    plain,
                    fall: river.fall,
                    born: river.born,
                    slope: wall_slope(river.erod, p.wall),
                    src: river.src,
                    w: e.w,
                    near,
                    far,
                });
            }
        }
        Band { w, far, cells }
    }

    /// Every sample within `reach` tiles of a position in tiles, in place
    /// of what `out` held.
    fn within(&self, q: f64, r: f64, reach: f64, out: &mut Vec<Sample>) {
        let wf = self.w as f64;
        let (ci, cj) = cube_round(q / wf, r / wf);
        let (x, y) = world(q, r);
        // k rings of cells stand k·√3/2 of the level's width out at their
        // nearest, and a sample lies within half a cell of its own.
        let k = (reach / (wf * SQRT_3 / 2.0)).ceil() as i64 + 1;
        out.clear();
        for (di, dj, _) in hex_offsets(k) {
            let Some(samples) = self.cells.get(&(ci + di, cj + dj)) else { continue };
            out.extend(samples.iter().filter(|s| (s.x - x).hypot(s.y - y) <= reach));
        }
    }

    fn all(&self) -> Vec<Sample> {
        self.cells.values().flatten().copied().collect()
    }
}

/// The valley at a position in tiles over `beds`: the lowest of every
/// bed, cut [`depth`] below its floor, and the wall rising from its edge
/// at the slope of the rock at its river vertex, and the sample governing
/// it and how far off, in tiles. None over no beds.
fn valley(beds: &[Sample], q: f64, r: f64, w: i64) -> Option<(f64, Sample, f64)> {
    let (x, y) = world(q, r);
    let mut best: Option<(f64, Sample, f64)> = None;
    for s in beds {
        let d = (s.x - x).hypot(s.y - y);
        let v = profile(s, (x, y), 0.0, w);
        if best.is_none_or(|b| v < b.0) {
            best = Some((v, *s, d));
        }
    }
    best
}

/// The valley at a position in tiles, each bed's at its lowest within
/// half a step of it, a step the distance between the level's points, met
/// by `sunk`, the sunk ground there, less `ground`, the dry ground there:
/// a cut. A point stands for the ground about it, so a valley narrower
/// than the level's steps draws as a trench, never a row of pits.
fn cut_at(beds: &[Sample], q: f64, r: f64, w: i64, sunk: f64, ground: f64) -> f64 {
    let (x, y) = world(q, r);
    let half_step = w as f64 / SQRT_3 / 2.0;
    beds.iter().map(|s| profile(s, (x, y), half_step, w)).fold(sunk, f64::min) - ground
}

/// A vertex's position in tiles.
fn vertex_tiles(t: u8, i: i64, j: i64, w: i64) -> (f64, f64) {
    let (q3, r3) = pos3(t, i, j, w);
    (q3 as f64 / 3.0, r3 as f64 / 3.0)
}

/// Which of cell (i, j)'s nine children, of a level `w` wide, stand under
/// water above the sea over `beds`, a bit to each in [`CHILD_OFFSETS`]'
/// order: none at the tiles, which have no children.
fn wet_children(beds: &[Sample], w: i64, i: i64, j: i64) -> u16 {
    if w < 3 {
        return 0;
    }
    let d = w / 3;
    CHILD_OFFSETS.iter().enumerate().fold(0, |wet, (k, &(oi, oj))| {
        let (q, r) = ((i * w + oi * d) as f64, (j * w + oj * d) as f64);
        match standing(beds, q, r, d) {
            Some((surface, _, _)) if surface > 0.0 => wet | 1 << k,
            _ => wet,
        }
    })
}

/// The water a cell's bed puts over it, `(surface, depth, river)`: at the
/// bed's floor, and never more than its depth over the cell's carved height
/// `z`.
fn on_carve(bed: Option<(f64, f64, usize)>, z: f64) -> Option<(f64, f64, usize)> {
    bed.map(|(surface, depth, river)| (surface.min(z + depth), depth, river))
}

/// The carved ground at each vertex of `cells` of a level: its ground met
/// by the valleys of every coarser level's rivers in `bands`, each band
/// read out to its own reach, the valleys drawn at the level's own width;
/// and its sunk ground, the same with each valley at its lowest within
/// half a step of the vertex: a valley rises from its river, so there it
/// stands half a step nearer.
/// The cells are taken by the water pass's deform cells, each reading once
/// the beds whose valleys may reach below its highest ground, and then by
/// each coarser cell's children, lowest the valley can stand among them
/// first, so a vertex stops at the first bed that cannot go below what it
/// has found.
pub(super) fn carved_over(lv: &Level, bands: &[Band], cells: &[usize]) -> Vec<([f64; 3], [f64; 3])> {
    let w = lv.w;
    let wf = w as f64;
    let half_step = wf / SQRT_3 / 2.0;
    // A group of the cells: its centre in tiles and in world units, how
    // far its vertices reach from that, and its highest ground.
    let span = |xs: &[usize]| {
        let at: Vec<(f64, f64)> = xs.iter().map(|&x| lv.ij(cells[x])).map(|(i, j)| ((i * w) as f64, (j * w) as f64)).collect();
        let (cq, cr) = at.iter().fold((0.0, 0.0), |a, &(q, r)| (a.0 + q, a.1 + r));
        let (cq, cr) = (cq / at.len() as f64, cr / at.len() as f64);
        let (cx, cy) = world(cq, cr);
        let radius = at.iter().map(|&(q, r)| world(q, r)).map(|(x, y)| (x - cx).hypot(y - cy)).fold(0.0, f64::max) + wf + half_step;
        let high = xs
            .iter()
            .flat_map(|&x| lv.own_vertices(cells[x]).map(|(_, v)| lv.ground[v]).collect::<Vec<_>>())
            .filter(|h| h.is_finite())
            .fold(f64::NEG_INFINITY, f64::max);
        ((cq, cr), (cx, cy), radius, high)
    };
    // The beds a group may read, each with the lowest its valley stands in
    // the group: within reach and below the group's highest ground.
    let reaching = |s: &Sample, (cx, cy): (f64, f64), radius: f64, high: f64| {
        let lowest = lowest_over(s, (cx, cy), radius, w);
        ((s.x - cx).hypot(s.y - cy) - radius <= s.far && lowest < high).then_some((lowest, *s))
    };
    let mut by: BTreeMap<(i64, i64), BTreeMap<(i64, i64), Vec<usize>>> = BTreeMap::new();
    for (x, &c) in cells.iter().enumerate() {
        let (i, j) = lv.ij(c);
        let (pi, pj, _, _) = parent_of(i, j);
        by.entry(lv.block(Pass::Water, c)).or_default().entry((pi, pj)).or_default().push(x);
    }
    let groups: Vec<Vec<Vec<usize>>> = by.into_values().map(|g| g.into_values().collect()).collect();
    let each: Vec<usize> = (0..groups.len()).collect();
    let lists: Vec<Vec<(f64, Sample)>> = in_parallel(&each, |found, g| {
        let xs: Vec<usize> = groups[g].iter().flatten().copied().collect();
        let ((cq, cr), centre, radius, high) = span(&xs);
        let mut list = Vec::new();
        for band in bands {
            band.within(cq, cr, band.far + radius, found);
            list.extend(found.iter().filter_map(|s| reaching(s, centre, radius, high)));
        }
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        list
    });
    // Each coarser cell's children read their group's beds again, nearer:
    // lowest the valley can stand among them first. No vertex of theirs
    // stands above a bed's valley at their far side, so a bed whose valley
    // cannot come below the least of those is passed by, and the rest
    // after it.
    let subs: Vec<(usize, &Vec<usize>)> = groups.iter().enumerate().flat_map(|(g, subs)| subs.iter().map(move |xs| (g, xs))).collect();
    let mut sub_of = vec![0usize; cells.len()];
    for (k, (_, xs)) in subs.iter().enumerate() {
        for &x in xs.iter() {
            sub_of[x] = k;
        }
    }
    let each: Vec<usize> = (0..subs.len()).collect();
    let near_lists: Vec<Vec<(f64, Sample)>> = in_parallel(&each, |_, k| {
        let (g, xs) = subs[k];
        let (_, centre, radius, high) = span(xs);
        let mut ceiling = high;
        let mut list: Vec<(f64, Sample)> = Vec::new();
        for (lowest, s) in &lists[g] {
            if *lowest >= ceiling {
                break;
            }
            if let Some(near) = reaching(s, centre, radius, ceiling) {
                if (s.x - centre.0).hypot(s.y - centre.1) + radius <= s.far {
                    ceiling = ceiling.min(highest_over(s, centre, radius, w));
                }
                list.push(near);
            }
        }
        // A banded wall stands level at its bands, so the bed that set the
        // ceiling may stand at it everywhere.
        list.retain(|n| n.0 <= ceiling);
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        list
    });
    let index: Vec<usize> = (0..cells.len()).collect();
    in_parallel(&index, |_, x| {
        let c = cells[x];
        let (i, j) = lv.ij(c);
        let (mut out, mut sunk) = ([f64::NAN; 3], [f64::NAN; 3]);
        for (t, v) in lv.own_vertices(c) {
            let (q, r) = vertex_tiles(t, i, j, w);
            let (px, py) = world(q, r);
            let ground = if lv.ground[v].is_nan() { f64::INFINITY } else { lv.ground[v] };
            let (mut low, mut near) = (ground, ground);
            for (lowest, s) in &near_lists[sub_of[x]] {
                if *lowest >= low {
                    break;
                }
                let d = (s.x - px).hypot(s.y - py);
                if d <= s.far {
                    low = low.min(profile(s, (px, py), 0.0, w));
                    near = near.min(profile(s, (px, py), half_step, w));
                }
            }
            out[t as usize] = if low.is_infinite() { lv.ground[v] } else { low };
            sunk[t as usize] = if near.is_infinite() { lv.ground[v] } else { near };
        }
        (out, sunk)
    })
}

/// The top's cut at every vertex: its valley wholly.
pub(super) fn top_cut(lv: &mut Level, edges: &[Edge], p: &Params) {
    let w = lv.w;
    let radius = p.ring * w as f64;
    let beds = Band::new(w, edges, -1.0, radius, w, p);
    let cells: Vec<usize> = (0..lv.cap()).filter(|&c| lv.exists[c]).collect();
    let view: &Level = lv;
    let cuts = in_parallel(&cells, |near, c| {
        let (i, j) = view.ij(c);
        let mut cuts = [0.0; 3];
        if !p.water {
            return (cuts, None, 0);
        }
        let (q, r) = vertex_tiles(0, i, j, w);
        beds.within(q, r, radius, near);
        let near: &[Sample] = near;
        for (t, v) in view.own_vertices(c) {
            let (vq, vr) = vertex_tiles(t, i, j, w);
            cuts[t as usize] = cut_at(near, vq, vr, w, view.ground[v], view.ground[v]);
        }
        (cuts, on_carve(standing(near, q, r, w), (view.ground[c] + cuts[0] + 0.5).floor()), wet_children(near, w, i, j))
    });
    for (&c, (cuts, wet, parts)) in cells.iter().zip(cuts) {
        let vertices: Vec<(u8, usize)> = lv.own_vertices(c).collect();
        for (t, v) in vertices {
            lv.cut[v] = cuts[t as usize];
        }
        lv.wparts[c] = parts;
        (lv.water[c], lv.wdepth[c]) = wet.map_or((f64::NAN, f64::NAN), |w| (w.0, w.1));
        lv.wwet[c] = wet.map_or(-1, |w| w.2 as i64);
        lv.watered[c] = true;
    }
}

/// What a level's carve at one cell came to.
struct Carve {
    /// The cut at each of its vertices.
    cut: [f64; 3],
    /// The sample governing the cell's valley, and how far off, in tiles.
    governing: Option<(Sample, f64)>,
    /// The water standing over the cell, its depth, and the river vertex
    /// it stands over.
    water: Option<(f64, f64, usize)>,
    /// Which of its children stand under water ([`wet_children`]).
    parts: u16,
}

/// The carve over each pending water cell's own cells, and the water
/// standing in it: the valley of every river in `band` within its reach,
/// met by the cell's carved ground.
pub(super) fn level_cut(lv: &mut Level, band: &Band, pend: &[(i64, i64)], p: &Params) {
    let lists = lv.block_lists(Pass::Water, pend);
    let w = lv.w;
    let wf = w as f64;
    let view: &Level = lv;
    // Each deform cell's beds read once: those whose walls may reach its
    // highest ground from where they lie.
    let blocks: Vec<(Vec<usize>, Vec<Sample>)> = lists
        .iter()
        .map(|(own, _)| {
            let at: Vec<(f64, f64)> = own.iter().map(|&c| view.ij(c)).map(|(i, j)| ((i * w) as f64, (j * w) as f64)).collect();
            let (cq, cr) = at.iter().fold((0.0, 0.0), |a, &(q, r)| (a.0 + q, a.1 + r));
            let (cq, cr) = (cq / at.len().max(1) as f64, cr / at.len().max(1) as f64);
            let (cx, cy) = world(cq, cr);
            let radius = at.iter().map(|&(q, r)| world(q, r)).map(|(x, y)| (x - cx).hypot(y - cy)).fold(0.0, f64::max) + wf + wf / SQRT_3 / 2.0;
            let high = own
                .iter()
                .flat_map(|&c| view.own_vertices(c).map(|(_, v)| view.ground[v]).collect::<Vec<_>>())
                .filter(|g| g.is_finite())
                .fold(f64::NEG_INFINITY, f64::max);
            let mut beds = Vec::new();
            band.within(cq, cr, band.far + radius, &mut beds);
            beds.retain(|s| {
                let d = (s.x - cx).hypot(s.y - cy);
                d + radius > s.near && d - radius <= s.far && lowest_over(s, (cx, cy), radius, w) < high
            });
            (own.clone(), beds)
        })
        .collect();
    let jobs: Vec<(usize, usize)> = blocks.iter().enumerate().flat_map(|(b, (own, _))| own.iter().map(move |&c| (b, c))).collect();
    let index: Vec<usize> = (0..jobs.len()).collect();
    let found = in_parallel(&index, |near, x| {
        let (b, c) = jobs[x];
        let (i, j) = view.ij(c);
        let none = Carve { cut: [0.0; 3], governing: None, water: None, parts: 0 };
        if !p.water {
            return none;
        }
        // Every vertex of a cell reads the beds its centre reads: the reach
        // a deform clears is measured from the cells it holds.
        let (q, r) = vertex_tiles(0, i, j, w);
        let (x, y) = world(q, r);
        near.clear();
        near.extend(blocks[b].1.iter().filter(|s| {
            let d = (s.x - x).hypot(s.y - y);
            d > s.near && d <= s.far
        }));
        let near: &[Sample] = near;
        let mut cut = [0.0; 3];
        for (t, v) in view.own_vertices(c) {
            let (vq, vr) = vertex_tiles(t, i, j, w);
            cut[t as usize] = cut_at(near, vq, vr, w, view.sunk[v], view.ground[v]);
        }
        let (ground, carved) = (view.ground[c], view.carved[c]);
        let bed = standing(&near, q, r, w);
        let parts = wet_children(&near, w, i, j);
        let water = |z: f64| {
            let river = on_carve(bed, z);
            let lake = view.lake[c];
            match (lake > 0.0).then(|| ((lake + 0.5).floor(), 0.0, usize::MAX)) {
                Some(lake) if river.is_none_or(|r| lake.0 > r.0) => Some(lake),
                _ => river,
            }
        };
        let Some((_, governing, d)) = valley(&near, q, r, w) else {
            return Carve { cut, water: water((carved + 0.5).floor()), parts, ..none };
        };
        Carve { cut, governing: Some((governing, d)), water: water((ground + cut[0] + 0.5).floor()), parts }
    });
    for (&(_, c), k) in jobs.iter().zip(found) {
        let vertices: Vec<(u8, usize)> = lv.own_vertices(c).collect();
        for (t, v) in vertices {
            lv.cut[v] = k.cut[t as usize];
        }
        lv.wdist[c] = k.governing.map_or(f64::INFINITY, |(_, d)| d / wf);
        lv.wborn[c] = k.governing.map_or(-1, |(s, _)| s.born);
        lv.wsrc[c] = k.governing.map_or(-1, |(s, _)| s.w);
        lv.wgov[c] = k.governing.map_or(-1, |(s, _)| s.src as i64);
        lv.wgfloor[c] = k.governing.map_or(f64::NAN, |(s, _)| s.floor);
        (lv.water[c], lv.wdepth[c]) = k.water.map_or((f64::NAN, f64::NAN), |w| (w.0, w.1));
        lv.wwet[c] = k.water.map_or(-1, |w| w.2 as i64);
        lv.wparts[c] = k.parts;
        lv.watered[c] = true;
    }
}

/// The cut at the centre of each of `cells` from every river of `edges`,
/// however far: the valley a level would cut with no ring, met by its
/// carved ground.
#[cfg(feature = "inspect")]
pub(super) fn whole_cut(lv: &Level, edges: &[Edge], cells: &[usize], p: &Params) -> Vec<f64> {
    let all = Band::new(lv.w, edges, -1.0, f64::INFINITY, lv.w, p).all();
    in_parallel(cells, |_, c| {
        let (i, j) = lv.ij(c);
        let (q, r) = vertex_tiles(0, i, j, lv.w);
        cut_at(&all, q, r, lv.w, lv.sunk[c], lv.ground[c])
    })
}

/// `f` over every item, on every thread, in order, each thread handing it
/// one list for the beds near an item, kept from item to item: Windows'
/// heap queues a list that size from every thread behind one lock.
fn in_parallel<T: Send>(items: &[usize], f: impl Fn(&mut Vec<Sample>, usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| {
                s.spawn(|| {
                    let mut near = Vec::new();
                    part.iter().map(|&c| f(&mut near, c)).collect::<Vec<T>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a water thread")).collect()
    })
}
