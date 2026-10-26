//! Routing below the top: every vertex's receiver over a route cell and its
//! ring, on the level's carved ground, every trough's catchment, graded
//! profile and floor.
//!
//! Every vertex off the rivers handed down goes to its lowest neighbour in
//! the strict order (height, position), so water never climbs. Where
//! troughs are born, it goes instead to one of its lower neighbours by a
//! draw from its position weighted by how steeply the ground falls to
//! each: on even ground born troughs wander and two that meet are one, as
//! drainage branches, where a lattice's steepest way would run them side
//! by side forever; in a valley its walls leave only the way down.
//!
//! The level above hands its rivers down with their courses: each of its
//! trough edges is three of the level's steps, and the level lays the
//! river along it, straight, or through the middle of the triangle on one
//! side where the river's plain is wider than its bed by half the level's
//! width, so it bends at every level whose steps its plain has room for.
//! Which side, or none, is drawn from the edge; a triangle's middle bends
//! the largest river wanting it, so two courses meet only where rivers
//! do. Along the way the river carries its catchment, flow and birth, and
//! its graded profile and floor fall from one end's to the other's: a
//! river is cut once, where it is born, and every level's copy of it
//! stands at the same floors. Whatever a trough drains into is a trough,
//! carrying the larger catchment, so a river born at the level runs on to
//! where it meets one handed down, or to its outlet.
//!
//! A hollow holds a lake: the footprint is flooded from its outlets, the
//! sea and its edge, lowest water first, its ground under a lake the level
//! above hands down standing at that lake's surface as the level's own
//! ground holds it ([`flood_zone`]), so every block reads one surface for
//! a lake however far it spreads; the flood leaves each hollow
//! over the lowest point of its rim at the level it filled to, and water
//! that would end in a hollow takes the flood's way out from where it
//! reaches the lake. A vertex
//! the flood reached above its own ground stands in the lake at that level.
//! The river leaving a lake cuts its outlet as it cuts any floor: the
//! lake's surface is lowered toward its outlet's graded profile, as far
//! as the plate's age has carried erosion, and a vertex the lowered
//! surface no longer covers is dry. A lake is the base level of the
//! rivers above it, and its floor is its surface, the lake handed down's
//! at a vertex the block reads but does not route; the floors below never
//! stand above it, so the outlet is cut to it. A level draws the lake it
//! keeps over a cell's centre.
//!
//! A vertex gathers what drains into it, capped at the level above's head
//! in its rock, since what it could carry past that the level above carries
//! in its own troughs; where it reaches the level's head, where a channel
//! begins to cut in its rock ([`head_of`]), a trough is born. Water short
//! of the head splits between the same two neighbours by those weights, so
//! a slope drains in a spread; a trough goes whole to its receiver.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::geom::{facet, parent_of, pos3, vertex_cells, vnbr, wander, xy};
use super::mask::Mask;
use super::paged::Paged;
use super::{drained, graded_slope, head_of, water, Level, Params, Pass};

pub(super) const WALK_SEED: u64 = 0x5761_6c6b_5f5f_5f5f; // "Walk____"

/// A vertex the flood reached: the water level it was reached at, the
/// highest ground on the way from an outlet, then its own key.
struct Flood {
    level: f64,
    key: (f64, i64, i64),
    x: usize,
}

impl PartialEq for Flood {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for Flood {}

impl PartialOrd for Flood {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Flood {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (a, b) = (self.key, other.key);
        self.level.total_cmp(&other.level).then(a.0.total_cmp(&b.0)).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
    }
}

/// What routing decided for one of a block's own vertices: its receiver,
/// whether it is a trough, its catchment, flow, graded profile, floor and
/// birth.
struct Decided {
    v: usize,
    rec: i64,
    trough: bool,
    acc: f64,
    flow: f64,
    graded: f64,
    room: f64,
    lake: f64,
    floor: f64,
    born: i64,
}

/// What one block's routing came to: its own vertices' values and its
/// share of the counts [`route_blocks`] returns.
struct Block {
    decided: Vec<Decided>,
    stats: [u64; 4],
}

/// A thread's working tables over the level, kept from block to block: the
/// block whose footprint each cell fell in last, and each footprint
/// vertex's index in the block, -1 outside it.
struct Scratch {
    stamp: Paged<u32>,
    loc: Paged<i64>,
}

/// Routes every pending route cell's own vertices. Returns the troughs
/// whose upstream reached the footprint's edge, the own vertices that did,
/// own vertices left undecided, and own vertices in a cycle.
///
/// A block reads the level's ground and the level above, never what
/// another block decides, so blocks are routed apart, on every thread.
pub(super) fn route_blocks(lv: &Level, par: &Level, pend: &[(i64, i64)], p: &Params, births: bool) -> [u64; 4] {
    let lists = lv.block_lists(Pass::Route, pend);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(lists.len());
    let next = AtomicUsize::new(0);
    let blocks: Vec<Block> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let nn = lv.cap();
                    let mut scratch = Scratch { stamp: Paged::new(0, nn), loc: Paged::new(-1, 3 * nn) };
                    let mut out = Vec::new();
                    loop {
                        let b = next.fetch_add(1, Ordering::Relaxed);
                        let Some(block) = lists.get(b) else { break };
                        out.push(route_block(lv, par, p, births, b, block, &mut scratch));
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a routing thread")).collect()
    });
    let mut stats = [0u64; 4];
    for block in blocks {
        for d in block.decided {
            lv.rec.set(d.v, d.rec);
            lv.trough.set(d.v, d.trough);
            lv.acc.set(d.v, d.acc);
            lv.flow.set(d.v, d.flow);
            lv.graded.set(d.v, d.graded);
            lv.room.set(d.v, d.room);
            lv.lake.set(d.v, d.lake);
            lv.floor.set(d.v, d.floor);
            lv.born.set(d.v, d.born);
        }
        for (a, b) in stats.iter_mut().zip(block.stats) {
            *a += b;
        }
    }
    for (own, _) in &lists {
        for &c in own {
            lv.routed.finish(c);
        }
    }
    stats
}

/// Routes block `b`, its own cells over its footprint, `scratch` left as
/// it was found.
fn route_block(lv: &Level, par: &Level, p: &Params, births: bool, b: usize, (own, fpl): &(Vec<usize>, Vec<usize>), scratch: &mut Scratch) -> Block {
    let nn = lv.cap();
    let pnn = par.cap();
    let w = lv.w;
    let Scratch { stamp, loc } = scratch;
    let mut stats = [0u64; 4];
    let cur = b as u32 + 1;
    for &c in fpl {
        stamp.set(c, cur);
    }
    let vids: Vec<usize> = fpl.iter().flat_map(|&c| (0..3).map(move |t| t * nn + c)).collect();
    for (x, &v) in vids.iter().enumerate() {
        loc.set(v, x as i64);
    }
    let nv = vids.len();
    let local = |g: usize| -> Option<usize> { (loc.get(g) >= 0).then(|| loc.get(g) as usize) };
    let mut known = vec![false; nv];
    let mut sink = vec![false; nv];
    let mut decided = vec![false; nv];
    let mut key = vec![(0.0f64, 0i64, 0i64); nv];
    let mut at = vec![(0.0f64, 0.0f64); nv];
    let mut pos = vec![(0i64, 0i64); nv];
    for (x, &v) in vids.iter().enumerate() {
        let (t, i, j) = lv.vertex(v);
        let (cells, m) = vertex_cells(t, i, j);
        known[x] = cells[..m].iter().all(|&(a, bb)| lv.idx(a, bb).is_none_or(|c| stamp.get(c) == cur && lv.supplied.done(c)));
        let (q3, r3) = pos3(t, i, j, w);
        key[x] = (lv.sunk.get(v), 2 * r3 + q3, q3);
        at[x] = xy(q3, r3);
        pos[x] = (q3, r3);
        if known[x] && lv.ground.get(v) < 0.0 {
            sink[x] = true;
            decided[x] = true;
        }
    }
    let less = |a: usize, b: usize| -> bool {
        let (x, y) = (key[a], key[b]);
        x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)).is_lt()
    };
    let mut rec: Vec<Option<usize>> = vec![None; nv];
    let mut recg = vec![-1i64; nv];
    // Where water short of the head goes, and the first's share.
    let mut split: Vec<Option<(usize, Option<usize>, f64)>> = vec![None; nv];
    // How steep each vertex's downslope is, in steps per tile.
    let mut slope = vec![0.0f64; nv];
    for x in 0..nv {
        if sink[x] || !known[x] {
            continue;
        }
        let (t, i, j) = lv.vertex(vids[x]);
        let (mut best, mut edge, mut undecided) = (None::<usize>, false, false);
        let mut around = Vec::with_capacity(6);
        for k in 0..6 {
            let (tt, ni, nj) = vnbr(t, i, j, k);
            let Some(c) = lv.idx(ni, nj).filter(|&c| lv.exists[c]) else {
                edge = true;
                break;
            };
            let Some(y) = local(tt as usize * nn + c).filter(|&y| known[y]) else {
                undecided = true;
                continue;
            };
            around.push((at[y].0 - at[x].0, at[y].1 - at[x].1, key[y].0 - key[x].0, y));
            if less(y, best.unwrap_or(x)) {
                best = Some(y);
            }
        }
        if edge {
            sink[x] = true;
            decided[x] = true;
            continue;
        }
        if undecided {
            continue;
        }
        decided[x] = true;
        let Some(y) = best else { continue };
        if let Some((a, bb, share, fall)) = facet(&mut around) {
            slope[x] = fall * crate::SQRT_3;
            let ok_a = less(a, x) && share > 1e-12;
            let ok_b = less(bb, x) && share < 1.0 - 1e-12;
            split[x] = match (ok_a, ok_b) {
                (true, true) => Some((a, Some(bb), share)),
                (true, false) => Some((a, None, 1.0)),
                (false, true) => Some((bb, None, 1.0)),
                (false, false) => None,
            };
        }
        let pick = if births {
            around.retain(|n| less(n.3, x));
            wander(&around, crate::noise::hash_f64(pos[x].0, pos[x].1, WALK_SEED)).unwrap_or(y)
        } else {
            y
        };
        rec[x] = Some(pick);
        recg[x] = vids[pick] as i64;
    }
    // Lakes: the footprint flooded from its outlets, lowest water first:
    // the sea, and the vertices at its edge it cannot route; the ground
    // under a lake handed down filled to its surface.
    let mut came = vec![usize::MAX; nv];
    let mut lake = vec![f64::NAN; nv];
    let mut seen = vec![false; nv];
    let mut heap = BinaryHeap::new();
    let base: Vec<f64> = (0..nv).map(|x| key[x].0.max(lv.lake_zone.get(vids[x]))).collect();
    for x in 0..nv {
        if known[x] && (sink[x] || !decided[x]) {
            seen[x] = true;
            heap.push(Reverse(Flood { level: base[x], key: key[x], x }));
        }
    }
    while let Some(Reverse(f)) = heap.pop() {
        if f.level > key[f.x].0 {
            lake[f.x] = f.level;
        }
        let (t, i, j) = lv.vertex(vids[f.x]);
        for k in 0..6 {
            let (tt, ni, nj) = vnbr(t, i, j, k);
            let Some(y) = lv.idx(ni, nj).and_then(|c| local(tt as usize * nn + c)) else { continue };
            if seen[y] || !known[y] || !decided[y] {
                continue;
            }
            seen[y] = true;
            came[y] = f.x;
            heap.push(Reverse(Flood { level: f.level.max(base[y]), key: key[y], x: y }));
        }
    }
    // Where each vertex's water ends going downhill: its own index in a
    // hollow, `usize::MAX` at an outlet or past what was routed. Water
    // ending in a hollow takes the flood's way out from where it stands in
    // the lake or enters it, each step to ground flooded earlier, so it
    // reaches an outlet; above the lake it keeps its own way down.
    let mut end = vec![usize::MAX - 1; nv];
    for x0 in 0..nv {
        let mut chain = Vec::new();
        let mut x = x0;
        let fin = loop {
            if end[x] != usize::MAX - 1 {
                break end[x];
            }
            chain.push(x);
            match rec[x] {
                Some(y) => x = y,
                None => break if sink[x] || !decided[x] { usize::MAX } else { x },
            }
        };
        for y in chain {
            end[y] = fin;
        }
    }
    for x in 0..nv {
        let above_lake = lake[x].is_nan() && rec[x].is_none_or(|y| lake[y].is_nan());
        if end[x] >= nv || came[x] == usize::MAX || above_lake {
            continue;
        }
        rec[x] = Some(came[x]);
        recg[x] = vids[came[x]] as i64;
        split[x] = None;
    }
    // The level above's rivers laid on the level's lattice: each of its
    // trough edges is three of the level's steps, and runs straight or
    // bends through the middle of the triangle on either side
    // ([`course_of`]), carrying its catchment, flow and birth, its graded
    // profile and floor shared out along the way.
    let under = Mask::new(fpl.iter().filter_map(|&c| {
        let (i, j) = lv.ij(c);
        let (pi, pj, _, _) = parent_of(i, j);
        par.idx(pi, pj)
    }));
    let mut src_acc = vec![f64::NAN; nv];
    let mut src_flow = vec![f64::NAN; nv];
    let mut src_graded = vec![f64::NAN; nv];
    let mut src_floor = vec![f64::NAN; nv];
    let mut src_born = vec![-1i64; nv];
    for pc in under.iter() {
        for t in 0..3 {
            let u = t * pnn + pc;
            let Some(way) = course_of(par, lv, u, p) else { continue };
            let r = par.rec.get(u) as usize;
            let steps = way.len() - 1;
            for k in 0..steps {
                let Some(x) = lv.vid_at(way[k].0, way[k].1).and_then(local) else { continue };
                if sink[x] {
                    continue;
                }
                let f = k as f64 / steps as f64;
                let lerp = |a: f64, b: f64| a + (b - a) * f;
                src_acc[x] = par.acc.get(u);
                src_flow[x] = par.flow.get(u);
                src_born[x] = par.born.get(u);
                src_graded[x] = lerp(par.graded.get(u), par.graded.get(r));
                src_floor[x] = lerp(par.floor.get(u), par.floor.get(r));
                if let Some(y) = lv.vid_at(way[k + 1].0, way[k + 1].1).and_then(local) {
                    rec[x] = Some(y);
                    recg[x] = vids[y] as i64;
                    split[x] = None;
                }
            }
        }
    }
    // Where water from outside the footprint may arrive unseen.
    let mut own_flag = vec![false; nv];
    for &c in own {
        for t in 0..3 {
            own_flag[loc.get(t * nn + c) as usize] = true;
        }
    }
    let mut trunc = vec![false; nv];
    let view: &Level = lv;
    let neighbours = |x: usize| {
        let (t, i, j) = view.vertex(vids[x]);
        (0..6).filter_map(move |k| {
            let (tt, ni, nj) = vnbr(t, i, j, k);
            view.vid(tt, ni, nj)
        })
    };
    for x in 0..nv {
        if !known[x] || !decided[x] || neighbours(x).any(|g| local(g).is_none()) {
            trunc[x] = true;
        }
    }
    // A vertex the footprint cannot route passes nothing on, so whatever
    // it would drain into is short.
    for x in 0..nv {
        if known[x] && decided[x] {
            continue;
        }
        for g in neighbours(x) {
            if let Some(y) = local(g) {
                trunc[y] = true;
            }
        }
    }
    // Every edge water may take: the receiver, and a split's two.
    let succ = |x: usize| -> [Option<usize>; 3] {
        let (a, bb) = split[x].map_or((None, None), |(a, bb, _)| (Some(a), bb));
        let a = a.filter(|&a| Some(a) != rec[x]);
        let bb = bb.filter(|&bb| Some(bb) != rec[x]);
        [rec[x], a, bb]
    };
    // What of a vertex's water each edge carries.
    let shares = |x: usize, whole: bool| -> [(Option<usize>, f64); 3] {
        match split[x] {
            Some((a, bb, share)) if !whole => {
                let s = |y: Option<usize>| {
                    if y.is_none() {
                        0.0
                    } else if y == Some(a) {
                        share
                    } else if y.is_some() && y == bb {
                        1.0 - share
                    } else {
                        0.0
                    }
                };
                let e = succ(x);
                [(e[0], s(e[0])), (e[1], s(e[1])), (e[2], s(e[2]))]
            }
            _ => [(rec[x], 1.0), (None, 0.0), (None, 0.0)],
        }
    };
    // Catchment, flow and troughs over the footprint in flow order: what a
    // trough drains into is a trough, carrying its catchment, flow and
    // birth. Catchment past the level above's head is the level above's
    // trough's to carry; the flow is held back in the same proportion.
    let mut indeg = vec![0u32; nv];
    for x in 0..nv {
        for y in succ(x).into_iter().flatten() {
            indeg[y] += 1;
        }
    }
    let mut val = vec![0.0f64; nv];
    let mut inflow = vec![0.0f64; nv];
    let mut wet = vec![0.0f64; nv];
    let mut wet_in = vec![0.0f64; nv];
    let mut trough = vec![false; nv];
    let mut acc = vec![0.0f64; nv];
    let mut flow = vec![0.0f64; nv];
    let mut born = vec![-1i64; nv];
    let mut fed = vec![false; nv];
    let mut fed_acc = vec![0.0f64; nv];
    let mut fed_flow = vec![0.0f64; nv];
    let mut fed_born = vec![-1i64; nv];
    let mut order = Vec::with_capacity(nv);
    let mut stack: Vec<usize> = (0..nv).filter(|&x| indeg[x] == 0).collect();
    while let Some(x) = stack.pop() {
        order.push(x);
        let v = vids[x];
        let c = v % nn;
        let area = if v < nn && lv.exists[c] { (w * w) as f64 } else { 0.0 };
        let (head, above) = (head_of(w, lv.erod.get(c), slope[x], p), head_of(par.w, lv.erod.get(c), slope[x], p));
        let gathered = area + inflow[x];
        val[x] = gathered.min(above);
        let kept = if gathered > 0.0 { val[x] / gathered } else { 0.0 };
        wet[x] = (if area > 0.0 { area * lv.rain.get(c) } else { 0.0 } + wet_in[x]) * kept;
        let headed = births && head < above && val[x] >= head;
        trough[x] = !sink[x] && (headed || !src_acc[x].is_nan() || fed[x]);
        acc[x] = val[x];
        flow[x] = wet[x];
        if trough[x] {
            acc[x] = val[x].max(fed_acc[x]).max(if src_acc[x].is_nan() { 0.0 } else { src_acc[x] });
            flow[x] = wet[x].max(fed_flow[x]).max(if src_flow[x].is_nan() { 0.0 } else { src_flow[x] });
            born[x] = (if headed { w } else { -1 }).max(src_born[x]).max(fed_born[x]);
        }
        for (y, frac) in shares(x, trough[x]) {
            let Some(y) = y else { continue };
            inflow[y] += frac * val[x];
            wet_in[y] += frac * wet[x];
            if trunc[x] && frac > 0.0 {
                trunc[y] = true;
            }
            if trough[x] && frac > 0.0 {
                fed[y] = true;
                fed_acc[y] = fed_acc[y].max(acc[x]);
                fed_flow[y] = fed_flow[y].max(flow[x]);
                fed_born[y] = fed_born[y].max(born[x]);
            }
        }
        for y in succ(x).into_iter().flatten() {
            indeg[y] -= 1;
            if indeg[y] == 0 {
                stack.push(y);
            }
        }
    }
    // Each trough's room: how far its hillslopes run to the nearer
    // divide, on each side of its flow the farthest any vertex entering
    // the troughs within two steps lies from where it enters, the nearer
    // side's, read within the footprint.
    let mut entry = vec![usize::MAX; nv];
    for &x in order.iter().rev() {
        entry[x] = if trough[x] { x } else { rec[x].map_or(usize::MAX, |y| entry[y]) };
    }
    let mut slope_len = vec![[0.0f64; 2]; nv];
    for x in 0..nv {
        let e = entry[x];
        if e == usize::MAX || e == x {
            continue;
        }
        let (gx, gy) = (at[x].0 - at[e].0, at[x].1 - at[e].1);
        let side = rec[e].map_or(0, |y| {
            let (fx, fy) = (at[y].0 - at[e].0, at[y].1 - at[e].1);
            usize::from(fx * gy - fy * gx < 0.0)
        });
        let d = gx.hypot(gy) / crate::SQRT_3;
        slope_len[e][side] = slope_len[e][side].max(d);
    }
    let shown: &Level = lv;
    let step = |x: usize| {
        let (t, i, j) = shown.vertex(vids[x]);
        (0..6).filter_map(move |k| {
            let (tt, ni, nj) = vnbr(t, i, j, k);
            shown.vid(tt, ni, nj)
        })
    };
    let room = |x: usize| -> f64 {
        let mut best = slope_len[x];
        let mut take = |y: usize| {
            best[0] = best[0].max(slope_len[y][0]);
            best[1] = best[1].max(slope_len[y][1]);
        };
        for g in step(x) {
            let Some(y) = local(g) else { continue };
            take(y);
            for g2 in step(y) {
                if let Some(z) = local(g2) {
                    take(z);
                }
            }
        }
        if best[0] > 0.0 && best[1] > 0.0 { best[0].min(best[1]) } else { best[0].max(best[1]) }
    };
    // Each vertex's graded profile, outlets first: the sea's its level, a
    // lake's its surface, and up from its receiver by its channel's graded
    // slope over the way between, never rising downstream. A node holds its
    // river to the level above's grading where that stands lower, and
    // never below its receiver's. Water leaving by the footprint's edge or
    // the level's runs to an outlet the block cannot see: its profile is
    // not known, and it cuts nothing. The sea is every river's base level.
    let above_sea = |g: f64| if g < 0.0 { 0.0 } else { g };
    let grade = |lake: &[f64]| {
        let mut graded = vec![f64::NAN; nv];
        for &x in order.iter().rev() {
            let v = vids[x];
            graded[x] = if !lake[x].is_nan() {
                above_sea(lake[x])
            } else if let Some(y) = rec[x] {
                let tiles = (at[x].0 - at[y].0).hypot(at[x].1 - at[y].1) / crate::SQRT_3;
                let up = graded[y] + graded_slope(acc[x], lv.erod.get(v % nn), p) * tiles;
                above_sea(src_graded[x].min(up).max(graded[y]))
            } else if sink[x] && lv.ground.get(v) < 0.0 {
                0.0
            } else {
                f64::NAN
            };
        }
        graded
    };
    // Each lake lowered as the river leaving it cuts its outlet, the first
    // vertex downstream it does not cover: the lake's surface cut toward
    // the outlet's graded profile the way a floor is, in the outlet's rock
    // and plate, so a lake holds one level. A vertex the lowered lake no
    // longer covers is dry, and the rivers above a lake grade to it.
    let mut graded = grade(&lake);
    let mut outlet: Vec<Option<usize>> = vec![None; nv];
    for &x in order.iter().rev() {
        if lake[x].is_nan() {
            continue;
        }
        outlet[x] = rec[x].and_then(|y| if lake[y].is_nan() { Some(y) } else { outlet[y] });
    }
    // A lake handed down was cut by the level that found it.
    let mut lowered = lake.clone();
    for x in 0..nv {
        let Some(o) = outlet[x].filter(|_| lv.lake_zone.get(vids[x]).is_nan()) else { continue };
        let c = vids[o] % nn;
        let level = above_sea(drained(lake[x], graded[o], lv.age.get(c), lv.erod.get(c), room(o), p));
        lowered[x] = if level > key[x].0 { level } else { f64::NAN };
    }
    let mut lake = lowered;
    // Floors: drainage's under each vertex's dry ground, toward its graded
    // profile and held to the room its hillslopes give, and never above its
    // carved ground; where a node hands its river down, the level above's,
    // cut once from the dry ground and never again, but never above the
    // ground every coarser valley left with each channel full, so a river
    // laid across another's valley runs down into it rather than over it;
    // along a trough never above the floors draining into it, upstream
    // first, through a lake as anywhere: a lake's floor is its surface,
    // and a river reaching it cut lower lowers the whole lake to its floor,
    // the rivers above grading to the lowered lake, until none is lowered.
    let mut flo: Vec<f64>;
    loop {
        graded = grade(&lake);
        flo = (0..nv)
            .map(|x| {
                if !lake[x].is_nan() {
                    return above_sea(lake[x]);
                }
                let v = vids[x];
                // A vertex the block does not route stands under the lake
                // handed down over it, as the block that routes it floods it.
                if !known[x] && !lv.lake_zone.get(v).is_nan() {
                    return above_sea(lv.lake_zone.get(v));
                }
                if !src_floor[x].is_nan() {
                    return src_floor[x].min(lv.brim.get(v));
                }
                let c = v % nn;
                drained(lv.ground.get(v), graded[x], lv.age.get(c), lv.erod.get(c), room(x), p).min(lv.carved.get(v))
            })
            .collect();
        let mut low = vec![f64::INFINITY; nv];
        let mut under: Vec<(usize, f64)> = Vec::new();
        for &x in &order {
            if !trough[x] {
                continue;
            }
            if lake[x].is_nan() {
                flo[x] = flo[x].min(low[x]);
            } else if low[x] < lake[x] {
                under.push((x, low[x]));
            }
            if let Some(y) = rec[x] {
                low[y] = low[y].min(flo[x]);
            }
        }
        if under.is_empty() {
            break;
        }
        // Each lake reached below its surface, every vertex joined to it at
        // that surface, lowered to the lowest floor reaching it; a vertex it
        // no longer covers is dry.
        let mut level: Vec<f64> = lake.clone();
        for (x, floor) in under {
            let surface = lake[x];
            let mut joined = vec![x];
            let mut seen = HashSet::from([x]);
            while let Some(y) = joined.pop() {
                level[y] = level[y].min(floor);
                let (t, i, j) = lv.vertex(vids[y]);
                for k in 0..6 {
                    let (tt, ni, nj) = vnbr(t, i, j, k);
                    let Some(z) = lv.idx(ni, nj).and_then(|c| local(tt as usize * nn + c)) else { continue };
                    if lake[z] == surface && seen.insert(z) {
                        joined.push(z);
                    }
                }
            }
        }
        for x in 0..nv {
            lake[x] = if level[x] > key[x].0 { level[x] } else { f64::NAN };
        }
    }
    let mut done = vec![false; nv];
    let mut out = Vec::with_capacity(3 * own.len());
    for &x in &order {
        done[x] = true;
    }
    for x in 0..nv {
        if !own_flag[x] {
            continue;
        }
        let v = vids[x];
        if !decided[x] {
            stats[2] += 1;
        }
        if !done[x] {
            stats[3] += 1;
        }
        if trough[x] && trunc[x] {
            stats[0] += 1;
        }
        if trunc[x] {
            stats[1] += 1;
        }
        out.push(Decided {
            v,
            rec: if sink[x] { -1 } else { recg[x] },
            trough: trough[x],
            acc: acc[x],
            flow: flow[x],
            graded: graded[x],
            room: room(x),
            lake: lake[x],
            floor: (flo[x] + 0.5).floor(),
            born: if trough[x] { born[x] } else { -1 },
        });
    }
    for &v in &vids {
        loc.set(v, -1);
    }
    Block { decided: out, stats }
}

/// The way a trough edge of the level above, from vertex `u` to its
/// receiver, runs on the level `w` wide below it, as positions in thirds of
/// a tile from `u` to the receiver: three steps straight, or four, bending
/// through the middle of the triangle on one side, where the river's plain
/// leaves it room beside its bed, which side or none drawn from the edge.
/// A triangle's middle is the bend of one edge only: the largest river
/// among those wanting it, so no two ways share a vertex but where rivers
/// meet. None where `u` runs nowhere.
fn course_of(par: &Level, lv: &Level, u: usize, p: &Params) -> Option<Vec<(i64, i64)>> {
    let w = lv.w;
    let sunk = |(q3, r3): (i64, i64)| lv.vid_at(q3, r3).map_or(f64::NAN, |v| lv.sunk.get(v));
    let want = |u: usize| -> Option<((i64, i64), (i64, i64), Option<(i64, i64)>)> {
        if !par.trough.get(u) || par.rec.get(u) < 0 {
            return None;
        }
        let (t, i, j) = par.vertex(u);
        let (rt, ri, rj) = par.vertex(par.rec.get(u) as usize);
        let (a, b) = (pos3(t, i, j, par.w), pos3(rt, ri, rj, par.w));
        let e = ((b.0 - a.0) / 3, (b.1 - a.1) / 3);
        // A level finer than draws the channel would bend it within its own
        // width.
        let bend = if w >= water::drawn_at(water::bed(par.flow.get(u), p)) {
            let draw = crate::noise::hash_f64(a.0 + 7 * b.0, a.1 + 7 * b.1, BEND_SEED);
            let turn = if draw < 1.0 / 3.0 { Some((-e.1, e.0 + e.1)) } else if draw >= 2.0 / 3.0 { Some((e.0 + e.1, -e.0)) } else { None };
            turn.map(|f| (a.0 + e.0 + f.0, a.1 + e.1 + f.1))
        } else {
            None
        };
        // The middle of a triangle stands half the level's width from the
        // edge: the river bends there only where the ground rises less
        // than half a step a tile from its way, never up a valley's wall.
        let way = sunk((a.0 + e.0, a.1 + e.1)).max(sunk((a.0 + 2 * e.0, a.1 + 2 * e.1)));
        let bend = bend.filter(|&m| sunk(m) < way + w as f64 / 4.0);
        Some((a, e, bend))
    };
    let (a, e, bend) = want(u)?;
    let mine = (par.acc.get(u), u);
    // The other edges of the triangle this one would bend through.
    let bend = bend.filter(|&m| {
        let r = (3 * m.0 - 2 * a.0 - 3 * e.0, 3 * m.1 - 2 * a.1 - 3 * e.1);
        let q = (a.0 + 3 * e.0, a.1 + 3 * e.1);
        let ends = [(a, r), (q, r)];
        ends.iter().all(|&(x, y)| {
            [(x, y), (y, x)].iter().all(|&(from, to)| {
                let Some(k) = par.vid_at(from.0, from.1) else { return true };
                let runs_to = par.rec.get(k) >= 0 && par.vid_at(to.0, to.1) == Some(par.rec.get(k) as usize);
                !runs_to || want(k).and_then(|o| o.2) != Some(m) || (par.acc.get(k), k) < mine
            })
        })
    });
    let step = |k: i64| (a.0 + k * e.0, a.1 + k * e.1);
    // Where the river turns sharply at its receiver, its main stream cuts
    // the corner, a step of the level from the vertex to the next edge's.
    let r = par.rec.get(u) as usize;
    let end = match par.rec.get(r) {
        n if par.trough.get(r) && n >= 0 => {
            let (nt, ni, nj) = par.vertex(n as usize);
            let (rq, rr) = step(3);
            let (nq, nr) = pos3(nt, ni, nj, par.w);
            let f = ((nq - rq) / 3, (nr - rr) / 3);
            let sharp = f == (-e.0 - e.1, e.0) || f == (e.1, -e.0 - e.1);
            let (t, i, j) = par.vertex(r);
            let main = (0..6).filter_map(|k| {
                let (tt, a, b) = vnbr(t, i, j, k);
                par.vid(tt, a, b)
            })
            .filter(|&k| par.trough.get(k) && par.rec.get(k) == r as i64)
            .all(|k| (par.acc.get(k), k) <= mine);
            if sharp && main { (rq + f.0, rr + f.1) } else { (rq, rr) }
        }
        _ => step(3),
    };
    Some(match bend {
        Some(m) => vec![a, step(1), m, step(2), end],
        None => vec![a, step(1), step(2), end],
    })
}

/// The lakes the level above hands down over `zone`, cells of a level
/// whose parent or a cell within [`super::LAKE_REACH`] of it lies under
/// the level above's lake, joined to every such cell beside them: the
/// zone's vertices flooded together from the ground round it
/// ([`Level::brim`]: a river's channel spills at its water, never its
/// bed), lowest water first, so a lake
/// stands at one surface wherever it is read, as high as the level's own
/// ground holds it and never above the lake handed down over it. Where
/// the level's ground opens a hollow the level above found closed, the
/// lake drains to that opening. Every cell of the zone, and the cells
/// round it, supplied.
pub(super) fn flood_zone(lv: &Level, zone: &HashSet<usize>) {
    let nn = lv.cap();
    let w = lv.w;
    let key = |lv: &Level, v: usize| {
        let (t, i, j) = lv.vertex(v);
        let (q3, r3) = pos3(t, i, j, w);
        (lv.brim.get(v), 2 * r3 + q3, q3)
    };
    let inside = |v: usize| zone.contains(&(v % nn));
    let mut heap = BinaryHeap::new();
    let mut seen: HashSet<usize> = HashSet::new();
    let mut vertices = Vec::new();
    for &c in zone {
        for t in 0..3u8 {
            let v = t as usize * nn + c;
            if lv.brim.get(v).is_nan() {
                continue;
            }
            vertices.push(v);
            let (t, i, j) = lv.vertex(v);
            let mut outlet = lv.ground.get(v) < 0.0;
            for k in 0..6 {
                let (tt, ni, nj) = vnbr(t, i, j, k);
                match lv.idx(ni, nj).filter(|&d| lv.exists[d]) {
                    None => outlet = true,
                    Some(d) => {
                        let u = tt as usize * nn + d;
                        if !inside(u) && !lv.brim.get(u).is_nan() && seen.insert(u) {
                            heap.push(Reverse(Flood { level: lv.brim.get(u), key: key(lv, u), x: u }));
                        }
                    }
                }
            }
            if outlet && seen.insert(v) {
                heap.push(Reverse(Flood { level: lv.brim.get(v), key: key(lv, v), x: v }));
            }
        }
    }
    let mut level: HashMap<usize, f64> = HashMap::new();
    while let Some(Reverse(f)) = heap.pop() {
        if inside(f.x) {
            level.insert(f.x, f.level);
        }
        let (t, i, j) = lv.vertex(f.x);
        for k in 0..6 {
            let (tt, ni, nj) = vnbr(t, i, j, k);
            let Some(d) = lv.idx(ni, nj).filter(|&d| lv.exists[d]) else { continue };
            let u = tt as usize * nn + d;
            if inside(u) && !lv.brim.get(u).is_nan() && seen.insert(u) {
                heap.push(Reverse(Flood { level: f.level.max(lv.brim.get(u)), key: key(lv, u), x: u }));
            }
        }
    }
    for v in vertices {
        let surface = level.get(&v).map_or(f64::NAN, |&s| s.min(lv.lake_above.get(v % nn)));
        lv.lake_zone.set(v, if surface > lv.sunk.get(v) { surface } else { f64::NAN });
    }
    for &c in zone {
        lv.zoned.finish(c);
    }
}

const BEND_SEED: u64 = 0x4265_6e64_5f5f_5f5f; // "Bend____"
