//! Water: one target surface, the valley drainage's law cuts: a channel
//! along each trough, as wide as its catchment makes it, its water standing
//! at the trough's floor and its bed cut [`depth`] below that, and walls
//! rising from the bed at water's slope in the rock they stand in, out to
//! where they meet the ground. The water fills its valley to the floor:
//! out over the bed and up the walls to where they climb to it, so it
//! meets its banks at its own height.
//!
//! The top cuts its valley wholly, met by its ground and averaged over each
//! cell. A level below carves its own troughs' valleys within
//! `Params::ring` of its cells into its carved ground, the level above's
//! carve handed down, met by that and averaged over the cell. A valley is an absolute
//! height, so where the level above already cut a river to its floor the
//! level's own valley of it cuts nothing more.
//!
//! Water stands on a cell where a bed covers its centre and is as wide as
//! the cell, corner to corner: a river is wet at a level once its catchment
//! makes it that wide, so the finer the level, the smaller the rivers that
//! show. It stands at the lowest such bed's floor rounded, never above the
//! ground the river drains, and never more than [`depth`] over a cell
//! carved lower, so water stands on the carve, never over it. A cell under a lake takes
//! the lake's surface rounded where it stands higher, over the whole cell:
//! the lake is measured on the level's cells ([`cell_lakes`]), the heights
//! it draws.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};

use super::geom::{cube_round, descendants, hex_offsets, pos3, CHILD_OFFSETS, NB};
use super::mask::Mask;
use super::{wall_slope, Level, Params, Pass};
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
}

fn tiles(lv: &Level, v: usize) -> (f64, f64) {
    let (t, i, j) = lv.vertex(v);
    let (q3, r3) = pos3(t, i, j, lv.w);
    (q3 as f64 / 3.0, r3 as f64 / 3.0)
}

/// A level's troughs over a mask, each edge to its receiver.
pub(super) fn trough_edges(lv: &Level, mask: &Mask) -> Vec<Edge> {
    let nn = lv.cap();
    (0..3)
        .flat_map(|t| mask.iter().map(move |c| t * nn + c))
        .filter(|&v| lv.trough[v] && lv.rec[v] >= 0)
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
/// of the catchment, the way a channel's width grows with discharge, a tile
/// and a half at a trunk's catchment.
pub fn bed(catchment: f64, p: &Params) -> f64 {
    1.5 * (catchment / p.catchment_full).max(0.0).sqrt()
}

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

/// A point of a trough's bed: its position in world units a tile apart,
/// its floor, its half width, the width of the level its trough was born
/// at and of the level it was read from, and how far from a cell that
/// level's rivers are read: past `near`, out to `far`, in tiles.
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
    w: i64,
    near: f64,
    far: f64,
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
    /// `near` tiles and out to `far`.
    pub(super) fn new(w: i64, edges: &[Edge], near: f64, far: f64, p: &Params) -> Self {
        let wf = w as f64;
        let mut cells: BTreeMap<(i64, i64), Vec<Sample>> = BTreeMap::new();
        for e in edges {
            let ((q0, r0), (q1, r1)) = (e.from, e.to);
            let (x0, y0) = world(q0, r0);
            let (x1, y1) = world(q1, r1);
            let ns = 2usize.max(((x1 - x0).hypot(y1 - y0) / (0.5 * e.w as f64)).ceil() as usize + 1);
            for s in 0..ns {
                let f = s as f64 / (ns - 1) as f64;
                let (q, r) = (q0 + (q1 - q0) * f, r0 + (r1 - r0) * f);
                let (x, y) = world(q, r);
                let floor = e.floors.0 + (e.floors.1 - e.floors.0) * f;
                cells.entry(cube_round(q / wf, r / wf)).or_default().push(Sample {
                    x,
                    y,
                    floor,
                    bed: bed(e.acc, p),
                    born: e.born,
                    slope: wall_slope(e.erod, p.wall),
                    src: e.src,
                    w: e.w,
                    near,
                    far,
                });
            }
        }
        Band { w, far, cells }
    }

    /// Every sample within `reach` tiles of a position in tiles.
    fn within(&self, q: f64, r: f64, reach: f64) -> Vec<Sample> {
        let wf = self.w as f64;
        let (ci, cj) = cube_round(q / wf, r / wf);
        let (x, y) = world(q, r);
        let k = (reach / wf).ceil() as i64 + 1;
        let mut out = Vec::new();
        for (di, dj, _) in hex_offsets(k) {
            let Some(samples) = self.cells.get(&(ci + di, cj + dj)) else { continue };
            out.extend(samples.iter().filter(|s| (s.x - x).hypot(s.y - y) <= reach));
        }
        out
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
        let v = s.floor - depth(s.bed) + s.slope * (d - s.bed).max(0.0);
        if best.is_none_or(|b| v < b.0) {
            best = Some((v, *s, d));
        }
    }
    best
}

/// The valley met by `carved` and averaged over cell (i, j) of a level `w`
/// wide, at its centre and its children's, less `ground`: a cut.
fn cut_of(beds: &[Sample], w: i64, i: i64, j: i64, carved: f64, ground: f64) -> f64 {
    let offs = descendants(w, if w >= 3 { 1 } else { 0 });
    let sum: f64 = offs
        .iter()
        .map(|&(dq, dr)| valley(beds, (i * w + dq) as f64, (j * w + dr) as f64).map_or(carved, |v| v.0.min(carved)))
        .sum();
    sum / offs.len() as f64 - ground
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

/// A cell the flood reached, at the water level it was reached at, the
/// highest carved ground on the way from an outlet.
struct Filling {
    level: f64,
    c: usize,
}

impl PartialEq for Filling {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for Filling {}

impl PartialOrd for Filling {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Filling {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.level.total_cmp(&other.level).then(self.c.cmp(&other.c))
    }
}

/// The lakes over a footprint's cells, each cell's surface where a lake
/// stands over it above the sea: the footprint flooded on its carved cells
/// from its outlets, the sea and the cells at its edge, lowest water first,
/// so a hollow fills to the lowest cell of its rim, and a cell the flood
/// reached above its own carved height lies under that lake. Measured on
/// the cells the level draws, never on their corners, which stand higher
/// than a valley's cells.
fn cell_lakes(lv: &Level, fp: &[usize]) -> HashMap<usize, f64> {
    let inside: HashSet<usize> = fp.iter().copied().collect();
    let mut seen: HashSet<usize> = HashSet::new();
    let mut heap = BinaryHeap::new();
    for &c in fp {
        let (i, j) = lv.ij(c);
        let edge = NB.iter().any(|&(di, dj)| lv.idx(i + di, j + dj).is_none_or(|d| !inside.contains(&d)));
        if edge || lv.carved[c] < 0.0 {
            seen.insert(c);
            heap.push(Reverse(Filling { level: lv.carved[c], c }));
        }
    }
    let mut lakes = HashMap::new();
    while let Some(Reverse(f)) = heap.pop() {
        if f.level > lv.carved[f.c] && f.level > 0.0 {
            lakes.insert(f.c, f.level);
        }
        let (i, j) = lv.ij(f.c);
        for (di, dj) in NB {
            let Some(d) = lv.idx(i + di, j + dj) else { continue };
            if inside.contains(&d) && seen.insert(d) {
                heap.push(Reverse(Filling { level: f.level.max(lv.carved[d]), c: d }));
            }
        }
    }
    lakes
}

/// The water a cell's bed puts over it, `(surface, depth, river)`: at the
/// bed's floor, and never more than its depth over the cell's carved height
/// `z`.
fn on_carve(bed: Option<(f64, f64, usize)>, z: f64) -> Option<(f64, f64, usize)> {
    bed.map(|(surface, depth, river)| (surface.min(z + depth), depth, river))
}

/// The top's cut over every cell: its valley wholly.
pub(super) fn top_cut(lv: &mut Level, edges: &[Edge], p: &Params) {
    let w = lv.w;
    let radius = p.ring * w as f64;
    let beds = Band::new(w, edges, -1.0, radius, p);
    let cells: Vec<usize> = (0..lv.cap()).filter(|&c| lv.exists[c]).collect();
    let view: &Level = lv;
    let cuts = in_parallel(&cells, |c| {
        let (i, j) = view.ij(c);
        if !p.water {
            return (0.0, None, 0);
        }
        let (q, r) = ((i * w) as f64, (j * w) as f64);
        let near = beds.within(q, r, radius);
        let cut = cut_of(&near, w, i, j, view.ground[c], view.ground[c]);
        (cut, on_carve(standing(&near, q, r, w), (view.ground[c] + cut + 0.5).floor()), wet_children(&near, w, i, j))
    });
    for (&c, (cut, wet, parts)) in cells.iter().zip(cuts) {
        lv.cut[c] = cut;
        lv.wparts[c] = parts;
        (lv.water[c], lv.wdepth[c]) = wet.map_or((f64::NAN, f64::NAN), |w| (w.0, w.1));
        lv.wwet[c] = wet.map_or(-1, |w| w.2 as i64);
        lv.watered[c] = true;
    }
}

/// What a level's carve at one cell came to.
struct Carve {
    cut: f64,
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
            let radius = at.iter().map(|&(q, r)| world(q, r)).map(|(x, y)| (x - cx).hypot(y - cy)).fold(0.0, f64::max) + wf;
            let high = own.iter().map(|&c| view.ground[c]).filter(|g| g.is_finite()).fold(f64::NEG_INFINITY, f64::max);
            let beds: Vec<Sample> = band
                .within(cq, cr, band.far + radius)
                .into_iter()
                .filter(|s| {
                    let d = (s.x - cx).hypot(s.y - cy);
                    d + radius > s.near && d - radius <= s.far && s.floor - depth(s.bed) + s.slope * (d - radius - s.bed).max(0.0) < high
                })
                .collect();
            (own.clone(), beds)
        })
        .collect();
    let each: Vec<usize> = (0..lists.len()).collect();
    let lakes = in_parallel(&each, |b| cell_lakes(view, &lists[b].1));
    let jobs: Vec<(usize, usize)> = blocks.iter().enumerate().flat_map(|(b, (own, _))| own.iter().map(move |&c| (b, c))).collect();
    let index: Vec<usize> = (0..jobs.len()).collect();
    let found = in_parallel(&index, |x| {
        let (b, c) = jobs[x];
        let (i, j) = view.ij(c);
        let none = Carve { cut: 0.0, governing: None, water: None, parts: 0 };
        if !p.water {
            return none;
        }
        let (q, r) = ((i * w) as f64, (j * w) as f64);
        let (x, y) = world(q, r);
        let near: Vec<Sample> = blocks[b]
            .1
            .iter()
            .filter(|s| {
                let d = (s.x - x).hypot(s.y - y);
                d > s.near && d <= s.far
            })
            .copied()
            .collect();
        let (ground, carved) = (view.ground[c], view.carved[c]);
        let bed = standing(&near, q, r, w);
        let parts = wet_children(&near, w, i, j);
        let water = |z: f64| {
            let river = on_carve(bed, z);
            match lakes[b].get(&c).map(|&lake| ((lake + 0.5).floor(), 0.0, usize::MAX)) {
                Some(lake) if river.is_none_or(|r| lake.0 > r.0) => Some(lake),
                _ => river,
            }
        };
        let Some((_, governing, d)) = valley(&near, q, r) else {
            return Carve { cut: carved - ground, water: water((carved + 0.5).floor()), parts, ..none };
        };
        let cut = cut_of(&near, w, i, j, carved, ground);
        Carve { cut, governing: Some((governing, d)), water: water((ground + cut + 0.5).floor()), parts }
    });
    for (&(_, c), k) in jobs.iter().zip(found) {
        lv.cut[c] = k.cut;
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

/// The cut at each of `cells` from every river of `edges`, however far: the
/// valley a level would cut with no ring, met by its carved ground and
/// averaged over the cell.
#[cfg(feature = "inspect")]
pub(super) fn whole_cut(lv: &Level, edges: &[Edge], cells: &[usize], p: &Params) -> Vec<f64> {
    let all = Band::new(lv.w, edges, -1.0, f64::INFINITY, p).all();
    let w = lv.w;
    in_parallel(cells, |c| {
        let (i, j) = lv.ij(c);
        cut_of(&all, w, i, j, lv.carved[c], lv.ground[c])
    })
}

/// `f` over every item, on every thread, in order.
pub(super) fn in_parallel<T: Send>(items: &[usize], f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(chunk).map(|part| s.spawn(|| part.iter().map(|&c| f(c)).collect::<Vec<T>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("a water thread")).collect()
    })
}
