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
//! cells into its carved ground: its ground met by the plains and walls of
//! every coarser level's valleys. A river's channel and its water are drawn
//! by the finest level whose steps are as long as the channel is wide
//! ([`drawn_at`]), along that level's course, so it bends no sharper than
//! the river can; the levels coarser draw their own, and the finer levels
//! route the river but read its channel and its water from that one.
//!
//! Water stands on a cell where a bed covers its centre and is as wide as
//! the cell, corner to corner: a river is wet at a level once its catchment
//! makes it that wide, so the finer the level, the smaller the rivers that
//! show. It stands at the lowest such bed's floor rounded, never above the
//! ground the river drains, and never more than [`depth`] over a cell
//! carved lower, so water stands on the carve, never over it. A cell under a lake takes
//! the lake's surface rounded where it stands higher, over the whole cell:
//! the lake is the one routing keeps over the cell's centre, its outlet cut
//! as far as erosion has carried it, or the lake the level above hands
//! down, flooded whole at the level ([`super::route::flood_zone`]), so a
//! lake wider than a level's routing reads stands at every level below the
//! one that found it, at one surface, as high as each level's own ground
//! holds it. A lake is as deep as its surface
//! stands over the ground routing found it on, both rounded, and stands
//! on the carve as a river does, so a lake that rounds to its own ground
//! covers nothing.

use std::collections::{BTreeMap, HashSet};

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
    /// The graded profile at each end.
    graded: (f64, f64),
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
    graded: f64,
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
    // A vertex's graded profile, its floor where routing could not grade it.
    let graded_at = |u: usize| if lv.graded[u].is_nan() { lv.floor[u].max(0.0) } else { lv.graded[u].max(0.0) };
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
                    graded: graded_at(on),
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
                graded: (graded_at(v), graded_at(to)),
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

/// How deep a river's water stands, in levels, its channel cut that far
/// below its floor: a level, and a level more for each two tiles of its
/// half width past the first.
pub fn depth(bed: f64) -> f64 {
    1.0 + ((bed - 1.0) / 2.0).max(0.0).floor()
}

/// The width of the level that draws a channel `bed` tiles in half width:
/// the finest whose steps are as long as the channel is wide, so the curve
/// it is drawn along bends no sharper than the river can. A finer level
/// routes the river but reads its channel and its water from that one.
pub(super) fn drawn_at(bed: f64) -> i64 {
    let mut w = 1;
    while (w as f64) < 2.0 * SQRT_3 * bed {
        w *= 3;
    }
    w
}

/// A bed's valley as a level `w` wide draws it ([`profile`]): its channel
/// only where the level is as coarse as the level that draws it
/// ([`drawn_at`]), its banks alone where a coarser level draws the channel.
fn drawn(s: &Sample, at: (f64, f64), nearer: f64, w: i64) -> f64 {
    if drawn_at(s.bed) <= w { profile(s, at, nearer) } else { parts(s, at, nearer).1 }
}

/// The water standing over the centre of a cell of a level `w` wide, at a
/// position in tiles, from the beds at least as wide as the cell, out to
/// its corners, whose water reaches it: over the bed, or up the wall to
/// where it climbs to the floor, measured as the level draws the channel,
/// from the bed's stretch and half a step nearer ([`cut_at`]). The lowest
/// such bed's floor there rounded, its depth, and the river vertex it
/// stands over. None where none reaches.
fn standing(beds: &[Sample], q: f64, r: f64, w: i64) -> Option<(f64, f64, usize)> {
    let at = world(q, r);
    let corner = w as f64 / SQRT_3;
    let half_step = w as f64 / SQRT_3 / 2.0;
    beds.iter()
        .filter(|s| s.bed >= corner && drawn_at(s.bed) <= s.w.max(w))
        .filter_map(|s| {
            let (floor, d) = on_stretch(s, (s.x, s.y), s.on, at, half_step);
            (d <= s.bed + depth(s.bed) / s.slope).then(|| ((floor + 0.5).floor(), depth(s.bed), s.src))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// A bed's floor at the point of a stretch, from `from` on by `on`, nearest
/// a position, and how far that lies, taken `nearer` tiles shorter.
fn on_stretch(s: &Sample, from: (f64, f64), on: (f64, f64), (x, y): (f64, f64), nearer: f64) -> (f64, f64) {
    let (f, d) = project((x, y), from, (from.0 + on.0, from.1 + on.1));
    (s.floor + f * s.down, (d * SQRT_3 - nearer).max(0.0))
}

/// A bed's valley at a position in world units a tile apart, as a level
/// `w` wide draws it, each distance to its stretch taken `nearer` tiles
/// shorter: the
/// channel, its walls rising from the bed, and its plain, a step above the
/// river's water out to the plain's width from the bed with walls rising
/// from there, whichever stands lower. A plain no wider than the bed leaves the channel's walls running on up. The walls
/// beside a plain lean back ([`lean`]). It never falls as either distance
/// grows.
fn profile(s: &Sample, at: (f64, f64), nearer: f64) -> f64 {
    let (channel, banks) = parts(s, at, nearer);
    channel.min(banks)
}

/// A bed's valley as [`profile`] draws it, in two: its channel, and its
/// banks, the plain and the walls beside the bed, never lower than the
/// channel's walls where it has no plain.
fn parts(s: &Sample, (x, y): (f64, f64), nearer: f64) -> (f64, f64) {
    let (floor, d) = on_stretch(s, (s.x, s.y), s.on, (x, y), nearer);
    let channel = floor - depth(s.bed) + s.slope * (d - s.bed).max(0.0);
    // The plain rises from its river faster than the river falls, so a
    // point of it stands on the river beside it, never on a floor far
    // downstream whose plain reaches it.
    let across = (2.0 * s.fall + 0.01) * d.min(s.plain);
    let wall = lean(s) * (d - s.plain).max(0.0);
    (channel, floor + 1.0 + across + wall)
}

/// The slope of a valley's walls beside its plain, in steps a tile: the
/// rock's over a gorge, whose plain is its bed, leaning back toward repose,
/// one step a tile, as many times as the plain is wider than the bed. The
/// river cuts a wall's foot only while it runs against it; the wider it
/// wanders, the longer each wall has stood to weather back under its scree.
fn lean(s: &Sample) -> f64 {
    let wider = if s.plain > s.bed { s.plain / s.bed } else { 1.0 };
    (s.slope / wider).max(1.0)
}

/// The least [`profile`] over a disc `radius` tiles round a position: its
/// distance from the disc's nearest point, its floor wherever along the
/// stretch.
fn lowest_over(s: &Sample, at: (f64, f64), radius: f64) -> f64 {
    profile(s, at, radius) - s.down.abs()
}

/// The most a bed's banks stand over a disc `radius` tiles round a
/// position, never below its [`profile`] there.
fn highest_over(s: &Sample, at: (f64, f64), radius: f64) -> f64 {
    parts(s, at, -radius).1 + s.down.abs()
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
    /// Where it stands among its band's samples: the vertex its edge
    /// leaves, then how far along it; a tie between samples falls in this
    /// order.
    rank: (usize, u32),
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
        // On a level floor its water runs on downstream, the way its
        // graded profile falls.
        let g2 = e.next.as_ref().map_or(e.graded.1, |n| (e.graded.1 + n.graded) / 2.0);
        let [g0, gm, g1, g2] = [e.graded.0, (e.graded.0 + e.graded.1) / 2.0, e.graded.1, g2].map(|g| FLAT_FALL * g);
        Way {
            head,
            length: run,
            curve,
            ends: [m1, b, m2],
            floors: [e.floors.0 + g0, (e.floors.0 + e.floors.1) / 2.0 + gm, e.floors.1 + g1, f2 + g2],
            plains: [e.plains.0, (e.plains.0 + e.plains.1) / 2.0, e.plains.1, p2],
            rivers: [own, on],
        }
    }

    /// The point `s` along the way, and its floor, plain and river there.
    fn at(&self, s: f64) -> ((f64, f64), (f64, f64, River)) {
        let lerp = |a: f64, b: f64, f: f64| a + (b - a) * f;
        if let Some((a, len)) = self.head {
            if s < len {
                let f = s / len;
                let [m1, ..] = self.ends;
                let pt = (lerp(a.0, m1.0, f), lerp(a.1, m1.1, f));
                let at = (lerp(self.floors[0], self.floors[1], f), lerp(self.plains[0], self.plains[1], f), self.rivers[0]);
                return (pt, at);
            }
        }
        let k = self.curve.partition_point(|c| c.2 < s).clamp(1, Self::SPAN);
        let ((_, t0, s0), (_, t1, s1)) = (self.curve[k - 1], self.curve[k]);
        let t = if s1 > s0 { lerp(t0, t1, ((s - s0) / (s1 - s0)).clamp(0.0, 1.0)) } else { t1 };
        let [m1, b, m2] = self.ends;
        let pt = quad(m1, b, m2, t);
        let at = if t <= 0.5 {
            let f = 2.0 * t;
            (lerp(self.floors[1], self.floors[2], f), lerp(self.plains[1], self.plains[2], f), self.rivers[0])
        } else {
            let f = 2.0 * t - 1.0;
            (lerp(self.floors[2], self.floors[3], f), lerp(self.plains[2], self.plains[3], f), self.rivers[1])
        };
        (pt, at)
    }
}

/// How far a river's floor stands above itself, in steps, for each step
/// of its graded profile: too little to draw, enough that water on a floor
/// its river holds level runs on downstream rather than by the lattice,
/// since the graded profile falls wherever the river runs.
const FLAT_FALL: f64 = 1e-6;

/// The quadratic from `a` to `c` with control `b` at `t` in [0, 1].
fn quad(a: (f64, f64), b: (f64, f64), c: (f64, f64), t: f64) -> (f64, f64) {
    let (u, v, w) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t);
    (u * a.0 + v * b.0 + w * c.0, u * a.1 + v * b.1 + w * c.1)
}

/// One level's rivers as a level `read` wide reads them: their beds
/// sampled no more than half a cell apart, each edge once however often it
/// is asked for, each sample held by the cell of the level it falls in, in
/// one order ([`Sample::rank`]), so a tie between samples falls the same
/// way in every process whatever was asked for first; and how far from a
/// cell they are read, in tiles. A reader takes the samples within its
/// reach, so a band holding rivers past a reader's reach reads as one
/// holding only those within it.
pub(super) struct Band {
    w: i64,
    near: f64,
    far: f64,
    read: i64,
    cells: BTreeMap<(i64, i64), Vec<Sample>>,
    /// The edges sampled, by the vertex each leaves.
    sampled: HashSet<usize>,
}

impl Band {
    /// The beds of `edges`, of a level `w` wide, read from a cell past
    /// `near` tiles and out to `far`, as the level `read` wide reads them.
    pub(super) fn new(w: i64, edges: &[Edge], near: f64, far: f64, read: i64, p: &Params) -> Self {
        let mut band = Band::empty(w, near, far, read);
        band.add(edges, p);
        band
    }

    /// No beds yet, of a level `w` wide, read from a cell past `near`
    /// tiles and out to `far`, as the level `read` wide reads them.
    pub(super) fn empty(w: i64, near: f64, far: f64, read: i64) -> Self {
        Band { w, near, far, read, cells: BTreeMap::new(), sampled: HashSet::new() }
    }

    /// The beds of `edges` the band does not hold yet sampled into it. An
    /// edge's samples are final once its level has routed its vertex, its
    /// receiver and the vertices round both, as [`trough_edges`] reads
    /// them.
    pub(super) fn add(&mut self, edges: &[Edge], p: &Params) {
        let (w, near, far, read) = (self.w, self.near, self.far, self.read);
        let wf = w as f64;
        let mut touched = Vec::new();
        for e in edges {
            if !self.sampled.insert(e.src) {
                continue;
            }
            let way = Way::of(e);
            // A level's own beds at half its width; a coarser level's at a
            // quarter of the plain they bound, so a floor's edge follows
            // its river, and never closer than half the reader's width or
            // two tiles, nor sparser than its own.
            let length = way.length;
            let spacing = if read >= e.w {
                0.5 * e.w as f64
            } else {
                (0.25 * e.plains.0.min(e.plains.1)).max(0.5 * read as f64).max(2.0).min(0.5 * e.w as f64)
            };
            let ns = 2usize.max((length / spacing).ceil() as usize + 1);
            let points: Vec<_> = (0..ns).map(|s| way.at(s as f64 / (ns - 1) as f64 * length)).collect();
            for (s, &((x, y), at)) in points.iter().enumerate() {
                let (floor, plain, river) = at;
                let (on, down) = points.get(s + 1).map_or(((0.0, 0.0), 0.0), |&((nx, ny), (next, _, _))| ((nx - x, ny - y), next - floor));
                let q = x * 2.0 / SQRT_3;
                let r = y - q / 2.0;
                let at = cube_round(q / wf, r / wf);
                touched.push(at);
                self.cells.entry(at).or_default().push(Sample {
                    x,
                    y,
                    on,
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
                    rank: (e.src, s as u32),
                });
            }
        }
        touched.sort_unstable();
        touched.dedup();
        for at in touched {
            if let Some(samples) = self.cells.get_mut(&at) {
                samples.sort_by_key(|s| s.rank);
            }
        }
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
fn valley(beds: &[Sample], q: f64, r: f64) -> Option<(f64, Sample, f64)> {
    let (x, y) = world(q, r);
    let mut best: Option<(f64, Sample, f64)> = None;
    for s in beds {
        let d = (s.x - x).hypot(s.y - y);
        let v = profile(s, (x, y), 0.0);
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
    beds.iter().map(|s| drawn(s, (x, y), half_step, w)).fold(sunk, f64::min) - ground
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
/// by the plains and walls of every coarser level's valleys in `bands`,
/// and their channels where that level draws them ([`drawn_at`]), never
/// the channels a finer level draws ([`parts`]), each band read out to its
/// own reach,
/// the valleys drawn at the level's own width; and its sunk ground, the
/// same with each valley at its lowest within half a step of the vertex: a
/// valley rises from its river, so there it stands half a step nearer.
/// The cells are taken by the water pass's deform cells, each reading once
/// the beds whose valleys may reach below its highest ground, and then by
/// each coarser cell's children, lowest the valley can stand among them
/// first, so a vertex stops at the first bed that cannot go below what it
/// has found.
pub(super) fn carved_over(lv: &Level, bands: &[&Band], cells: &[usize]) -> Vec<Carved> {
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
        let lowest = lowest_over(s, (cx, cy), radius);
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
                    ceiling = ceiling.min(highest_over(s, centre, radius));
                }
                list.push(near);
            }
        }
        list.retain(|n| n.0 <= ceiling);
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        list
    });
    let index: Vec<usize> = (0..cells.len()).collect();
    in_parallel(&index, |_, x| {
        let c = cells[x];
        let (i, j) = lv.ij(c);
        let (mut out, mut sunk) = ([f64::NAN; 3], [f64::NAN; 3]);
        let mut held: Vec<Sample> = Vec::new();
        for (t, v) in lv.own_vertices(c) {
            let (q, r) = vertex_tiles(t, i, j, w);
            let (px, py) = world(q, r);
            let ground = if lv.ground[v].is_nan() { f64::INFINITY } else { lv.ground[v] };
            let (mut low, mut near) = (ground, ground);
            // A bed's banks never stand below its profile, so the least the
            // profile can come to bounds them too.
            for (lowest, s) in &near_lists[sub_of[x]] {
                if *lowest >= low {
                    break;
                }
                let d = (s.x - px).hypot(s.y - py);
                if d <= s.far {
                    let own = drawn_at(s.bed) == s.w;
                    let at = |nearer: f64| if own { profile(s, (px, py), nearer) } else { parts(s, (px, py), nearer).1 };
                    low = low.min(at(0.0));
                    near = near.min(at(half_step));
                    if own && t == 0 {
                        held.push(*s);
                    }
                }
            }
            out[t as usize] = if low.is_infinite() { lv.ground[v] } else { low };
            sunk[t as usize] = if near.is_infinite() { lv.ground[v] } else { near };
        }
        let (q, r) = vertex_tiles(0, i, j, w);
        Carved { carved: out, sunk, water: standing(&held, q, r, w), parts: wet_children(&held, w, i, j) }
    })
}

/// What [`carved_over`] finds at a cell: the carved and the sunk ground at
/// each of its vertices, and the water a coarser level's channel puts over
/// its centre and its children, where that level draws the river.
pub(super) struct Carved {
    pub carved: [f64; 3],
    pub sunk: [f64; 3],
    pub water: Option<(f64, f64, usize)>,
    pub parts: u16,
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
                d + radius > s.near && d - radius <= s.far && lowest_over(s, (cx, cy), radius) < high
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
        let held = (!view.held[c].is_nan()).then(|| (view.held[c], view.held_depth[c], view.held_src[c] as usize));
        let bed = match (standing(&near, q, r, w), held) {
            (Some(a), Some(b)) => Some(if b.0 < a.0 { b } else { a }),
            (a, b) => a.or(b),
        };
        let parts = wet_children(&near, w, i, j) | view.held_parts[c];
        let water = |z: f64| {
            let river = on_carve(bed, z);
            let lake = lakes(view, c);
            let lake = (lake > 0.0).then(|| {
                let surface = lake;
                (surface, surface - (view.sunk[c] + 0.5).floor(), usize::MAX)
            });
            match on_carve(lake, z) {
                Some(lake) if river.is_none_or(|r| lake.0 > r.0) => Some(lake),
                _ => river,
            }
        };
        let Some((_, governing, d)) = valley(&near, q, r) else {
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
        lv.lakes[c] = lakes(lv, c);
        lv.watered[c] = true;
    }
}

/// The surface of the lake routing keeps over a cell's centre, rounded;
/// NaN where none.
pub(super) fn lakes(lv: &Level, c: usize) -> f64 {
    (lv.lake[c] + 0.5).floor()
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
