//! Water: one target surface, the valley drainage's law cuts: a bed along
//! each trough at its floor, as wide as its catchment makes it, and walls
//! rising from the bed at water's slope in the rock they stand in, out to
//! where they meet the ground.
//!
//! The top cuts its valley, met by its ground and averaged over each cell,
//! wholly. A level below hands that down and corrects it: its own valley
//! averaged over its cells less the level above's valley averaged over the
//! level above's cells and handed down, each from the rivers running within
//! a corridor inside the ring, `Params::ring` cells, of the cell, a river
//! in or out by where the level above's line runs, so a line and the level
//! above's line are read together. The level above's term reads what the
//! level above wrote, its ground, lines and slopes, so it reproduces it but
//! for rivers outside the ring. The correction stands whole to [`FADE`]
//! cells inside where rivers are read from where the river governing the
//! valley at the cell runs, fades to nothing across the last [`FADE`], and
//! past that the surface handed down stands.

use super::geom::{cube_round, descendants, hex_offsets, parent_of, pos3, NB};
use super::{handed_down, wall_slope, Level, Params, Pass, CORRIDOR, FADE};
use crate::SQRT_3;

/// A trough edge the valley is read along: its ends' positions in tiles,
/// their floors, its catchment and the width of the level its trough was
/// born at.
pub(super) struct Edge {
    from: (f64, f64),
    to: (f64, f64),
    floors: (f64, f64),
    acc: f64,
    born: i64,
    /// The erodibility of the rock at the vertex the edge leaves.
    erod: f64,
    /// The vertex the edge leaves, as an index of its level.
    src: usize,
    /// The width of the level the edge is drawn at, which spaces its samples.
    w: i64,
    /// Where the river runs at each end, in world units a tile apart: on
    /// the level above's line for a line that has one, else the edge's own.
    anchors: ([f64; 2], [f64; 2]),
}

fn tiles(lv: &Level, v: usize) -> (f64, f64) {
    let (t, i, j) = lv.vertex(v);
    let (q3, r3) = pos3(t, i, j, lv.w);
    (q3 as f64 / 3.0, r3 as f64 / 3.0)
}

/// A position's own place, in world units a tile apart.
fn own_anchor(lv: &Level, v: usize) -> [f64; 2] {
    let (q, r) = tiles(lv, v);
    let (x, y) = world(q, r);
    [x, y]
}

/// A level's lines over a mask, where they carry `head` and have a floor:
/// a trough born at the level runs in its ground, and a line has none
/// offshore. Each is in or out of water's ring by where
/// the level above's line runs, or, read as the level above's own rivers
/// by the level below (`own`), by where it runs itself.
pub(super) fn line_edges(lv: &Level, mask: &[bool], head: f64, own: bool) -> Vec<Edge> {
    let nn = lv.n * lv.n;
    (0..3 * nn)
        .filter(|&v| lv.line_on[v] && lv.line_rec[v] >= 0 && mask[v % nn] && lv.line_acc[v] >= head && lv.line_fl[v].is_finite())
        .map(|v| {
            let to = lv.line_rec[v] as usize;
            let ends = lv.line_on[to] && lv.line_fl[to].is_finite();
            let (f1, a1) = if ends { (lv.line_fl[to], lv.line_anchor[to]) } else { (lv.line_fl[v], lv.line_anchor[v]) };
            let anchors = if own { (own_anchor(lv, v), own_anchor(lv, to)) } else { (lv.line_anchor[v], a1) };
            Edge {
                from: tiles(lv, v),
                to: tiles(lv, to),
                floors: (lv.line_fl[v], f1),
                acc: lv.line_acc[v],
                born: lv.line_born[v],
                erod: rock(lv, v),
                src: v,
                w: lv.w,
                anchors,
            }
        })
        .collect()
}

/// The top's troughs carrying `head`.
pub(super) fn trough_edges(lv: &Level, head: f64) -> Vec<Edge> {
    (0..lv.trough.len())
        .filter(|&v| lv.trough[v] && lv.rec[v] >= 0 && lv.acc[v] >= head)
        .map(|v| {
            let to = lv.rec[v] as usize;
            Edge {
                from: tiles(lv, v),
                to: tiles(lv, to),
                floors: (lv.floor[v], lv.floor[to]),
                acc: lv.acc[v],
                born: lv.born[v],
                erod: rock(lv, v),
                src: v,
                w: lv.w,
                anchors: (own_anchor(lv, v), own_anchor(lv, to)),
            }
        })
        .collect()
}

/// The troughs born at a level over a mask, each at the floor it was born
/// with: a river of its own.
pub(super) fn born_edges(lv: &Level, mask: &[bool]) -> Vec<Edge> {
    let nn = lv.n * lv.n;
    (0..3 * nn)
        .filter(|&v| lv.trough[v] && !lv.line_on[v] && lv.rec[v] >= 0 && mask[v % nn])
        .map(|v| {
            let to = lv.rec[v] as usize;
            Edge {
                from: tiles(lv, v),
                to: tiles(lv, to),
                floors: (lv.floor[v], lv.floor[to]),
                acc: lv.acc[v],
                born: lv.w,
                erod: rock(lv, v),
                src: v,
                w: lv.w,
                anchors: (own_anchor(lv, v), own_anchor(lv, to)),
            }
        })
        .collect()
}

/// The erodibility of the rock under a vertex, its cell's.
fn rock(lv: &Level, v: usize) -> f64 {
    let e = lv.erod[v % (lv.n * lv.n)];
    if e.is_nan() { 1.0 } else { e }
}

/// A position in tiles as world units a tile apart.
fn world(q: f64, r: f64) -> (f64, f64) {
    (1.5 * q / SQRT_3, r + q / 2.0)
}

/// The half width of a trough's bed, in tiles: never narrower than 0.6 of
/// a tile either side of the line, so a tile whose corner the line passes
/// through stands in it, and growing as the square root of the catchment,
/// the way a channel's width grows with discharge.
pub fn bed(catchment: f64, p: &Params) -> f64 {
    0.6 + 0.9 * (catchment / p.catchment_full).max(0.0).sqrt()
}

/// A point of a trough's bed: its position in world units a tile apart,
/// its floor, its half width, and the width of the level its trough was
/// born at.
#[derive(Clone, Copy)]
struct Sample {
    x: f64,
    y: f64,
    floor: f64,
    bed: f64,
    born: i64,
    /// Its wall's slope: the rock's at its river vertex.
    slope: f64,
    /// The river vertex its edge leaves.
    src: usize,
    /// Where its river runs.
    ax: f64,
    ay: f64,
}

/// The troughs' beds sampled no more than half a cell apart, each sample
/// held by the cell it falls in.
struct Beds {
    w: i64,
    samples: Vec<Vec<Sample>>,
}

impl Beds {
    fn new(lv: &Level, edges: &[Edge], p: &Params) -> Self {
        let wf = lv.w as f64;
        let mut samples = vec![Vec::new(); lv.n * lv.n];
        for e in edges {
            let ((q0, r0), (q1, r1)) = (e.from, e.to);
            let (x0, y0) = world(q0, r0);
            let (x1, y1) = world(q1, r1);
            let ns = 2usize.max(((x1 - x0).hypot(y1 - y0) / (0.5 * e.w as f64)).ceil() as usize + 1);
            for s in 0..ns {
                let f = s as f64 / (ns - 1) as f64;
                let (q, r) = (q0 + (q1 - q0) * f, r0 + (r1 - r0) * f);
                let (ci, cj) = cube_round(q / wf, r / wf);
                if let Some(c) = lv.idx(ci, cj) {
                    let (x, y) = world(q, r);
                    let floor = e.floors.0 + (e.floors.1 - e.floors.0) * f;
                    let (ax, ay) = (e.anchors.0[0] + (e.anchors.1[0] - e.anchors.0[0]) * f, e.anchors.0[1] + (e.anchors.1[1] - e.anchors.0[1]) * f);
                    samples[c].push(Sample { x, y, floor, bed: bed(e.acc, p), born: e.born, slope: wall_slope(e.erod, p.wall), src: e.src, ax, ay });
                }
            }
        }
        Beds { w: lv.w, samples }
    }

    /// Every sample whose river runs within `ring` tiles of a position in
    /// tiles, the samples themselves read as far as `reach`.
    fn within(&self, lv: &Level, q: f64, r: f64, ring: f64, reach: f64) -> Vec<Sample> {
        let wf = self.w as f64;
        let (ci, cj) = cube_round(q / wf, r / wf);
        let (x, y) = world(q, r);
        let keep = |s: &&Sample| (s.x - x).hypot(s.y - y) <= reach && (s.ax - x).hypot(s.ay - y) <= ring;
        let k = (reach / wf).ceil() as i64 + 1;
        if k as usize >= lv.n {
            // A reach past the level's whole extent reads all of it.
            return self.samples.iter().flatten().filter(keep).copied().collect();
        }
        let mut out = Vec::new();
        for (di, dj, _) in hex_offsets(k) {
            let Some(c) = lv.idx(ci + di, cj + dj) else { continue };
            out.extend(self.samples[c].iter().filter(keep));
        }
        out
    }
}

/// The valley at a position in tiles over `beds`: the lowest of every
/// bed's floor and the wall rising from its edge at the slope of the rock
/// at its river vertex, and the sample governing it and how far off, in
/// tiles. None over no beds.
fn valley(beds: &[Sample], q: f64, r: f64) -> Option<(f64, Sample, f64)> {
    let (x, y) = world(q, r);
    let mut best: Option<(f64, Sample, f64)> = None;
    for s in beds {
        let d = (s.x - x).hypot(s.y - y);
        let v = s.floor + s.slope * (d - s.bed).max(0.0);
        if best.is_none_or(|b| v < b.0) {
            best = Some((v, *s, d));
        }
    }
    best
}

/// The valley met by `ground` and averaged over cell (i, j) of a level `w`
/// wide, at its centre and its children's, less that ground: a cut.
fn cut_of(beds: &[Sample], w: i64, i: i64, j: i64, ground: f64) -> f64 {
    let offs = descendants(w, if w >= 3 { 1 } else { 0 });
    let sum: f64 = offs
        .iter()
        .map(|&(dq, dr)| valley(beds, (i * w + dq) as f64, (j * w + dr) as f64).map_or(ground, |v| v.0.min(ground)))
        .sum();
    sum / offs.len() as f64 - ground
}

/// The top's cut over every cell: its valley wholly.
pub(super) fn top_cut(lv: &mut Level, edges: &[Edge], p: &Params) {
    let beds = Beds::new(lv, edges, p);
    let w = lv.w;
    let radius = p.ring * w as f64;
    let cells: Vec<usize> = (0..lv.exists.len()).filter(|&c| lv.exists[c]).collect();
    let view: &Level = lv;
    let cuts = in_parallel(&cells, |c| {
        let (i, j) = view.ij(c);
        if !p.water {
            return 0.0;
        }
        let near = beds.within(view, (i * w) as f64, (j * w) as f64, radius, radius);
        cut_of(&near, w, i, j, view.ground[c])
    });
    for (&c, cut) in cells.iter().zip(cuts) {
        lv.cut[c] = cut;
        lv.watered[c] = true;
    }
}

/// What a level's correction at one cell came to.
struct Correction {
    cut: f64,
    whole: f64,
    moved: f64,
    off: f64,
    born: i64,
    river: usize,
    parity: f64,
    fading: bool,
}

/// The cut over each pending water cell's own cells: what the level above
/// handed down, corrected near the lines.
pub(super) fn level_cut(lv: &mut Level, par: &Level, own: &[Edge], above: &[Edge], pend: &[(i64, i64)], p: &Params) {
    let mine = Beds::new(lv, own, p);
    let theirs = Beds::new(lv, above, p);
    let lists = lv.block_lists(Pass::Water, pend);
    let cells: Vec<usize> = lists.iter().flat_map(|(o, _)| o.iter().copied()).collect();
    let w = lv.w;
    let wf = w as f64;
    let view: &Level = lv;
    let found = in_parallel(&cells, |c| {
        let (i, j) = view.ij(c);
        let carried = view.wpred[c];
        let none = Correction { cut: carried, whole: f64::NAN, moved: f64::NAN, off: f64::INFINITY, born: -1, river: usize::MAX, parity: f64::NAN, fading: false };
        if !p.water {
            return Correction { cut: 0.0, ..none };
        }
        let (q, r) = ((i * w) as f64, (j * w) as f64);
        let (radius, river_in) = (p.ring * wf, p.ring - CORRIDOR);
        let near = mine.within(view, q, r, river_in * wf, radius);
        let Some((_, governing, _)) = valley(&near, q, r) else { return none };
        let (x, y) = world(q, r);
        let off = (governing.ax - x).hypot(governing.ay - y) / wf;
        let share = ((river_in - off) / FADE).clamp(0.0, 1.0);
        if share <= 0.0 {
            return Correction { off, ..none };
        }
        let ground = view.ground[c] - carried;
        let here = cut_of(&near, w, i, j, ground);
        // The level above's cells around this one, each its valley from
        // `beds` met by its ground, averaged over it, and handed down here.
        let handed = |beds: &[Sample]| {
            let (a, b, _, _) = parent_of(i, j);
            let mut cuts = std::collections::HashMap::new();
            for (pa, pb) in std::iter::once((a, b)).chain(NB.iter().map(|&(da, db)| (a + da, b + db))) {
                let cut = par.idx(pa, pb).filter(|&pc| par.exists[pc] && par.built[pc]).map_or(f64::NAN, |pc| {
                    cut_of(beds, par.w, pa, pb, par.ground[pc] - par.wpred[pc])
                });
                cuts.insert((pa, pb), cut);
            }
            handed_down(i, j, |pa, pb| cuts.get(&(pa, pb)).copied().unwrap_or(f64::NAN))
        };
        let theirs_near = theirs.within(view, q, r, river_in * wf, radius);
        let from_above = handed(&theirs_near);
        let whole = here - from_above;
        if !whole.is_finite() {
            return Correction { off, ..none };
        }
        let moved = handed(&near) - from_above;
        // Whether rivers where the correction fades change the cell: its
        // terms read again without them.
        let (x, y) = world(q, r);
        let core = |s: &&Sample| (s.ax - x).hypot(s.ay - y) / wf <= river_in - FADE;
        let near_core: Vec<Sample> = near.iter().filter(core).copied().collect();
        let theirs_core: Vec<Sample> = theirs_near.iter().filter(core).copied().collect();
        let fading = near_core.len() < near.len() && cut_of(&near_core, w, i, j, ground) != here
            || theirs_core.len() < theirs_near.len() && handed(&theirs_core) != from_above;
        Correction { cut: (carried + share * whole).min(0.0), whole, moved, off, born: governing.born, river: governing.src, parity: from_above - carried, fading }
    });
    for (&c, k) in cells.iter().zip(found) {
        lv.cut[c] = k.cut;
        lv.wfull[c] = k.whole;
        lv.wshift[c] = k.moved;
        lv.wdist[c] = k.off;
        lv.wborn[c] = k.born;
        lv.wgov[c] = if k.river == usize::MAX { -1 } else { k.river as i64 };
        lv.wpar[c] = k.parity;
        lv.wfade[c] = k.fading;
        lv.watered[c] = true;
    }
}

/// `f` over every item, on every thread, in order.
fn in_parallel<T: Send>(items: &[usize], f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(chunk).map(|part| s.spawn(|| part.iter().map(|&c| f(c)).collect::<Vec<T>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("a water thread")).collect()
    })
}
