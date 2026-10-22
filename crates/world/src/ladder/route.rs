//! Routing below the top: every vertex's receiver over a route cell and its
//! ring, on the level's carved ground, every trough's catchment, graded
//! profile and floor.
//!
//! Every vertex goes to its lowest neighbour in the strict order (height,
//! the level above's surface, tilt, position), so water never climbs. Where
//! troughs are born, it goes instead to one of its lower neighbours by a
//! draw from its position weighted by how steeply the ground falls to
//! each: on even ground born troughs wander and two that meet are one, as
//! drainage branches, where a lattice's steepest way would run them side
//! by side forever; in a valley its walls leave only the way down. Below,
//! a river follows its valley.
//!
//! The level above hands its rivers down as nodes, its trough vertices,
//! each with its catchment and its graded profile, and no course: levels nest, so each node is one
//! of the level's vertices, and a river's course is the level's own water
//! running downhill from it. Nodes are taken upstream first. The level's
//! water from the node upstream is followed until it comes within half a
//! cell of the level above, or for [`FOLLOW`] steps, and the node's
//! catchment joins it where it stops: the node is that river already, and
//! a river that drifts is never started twice. Where that water ends short
//! of the node, at the sea or the footprint's edge, and where no node lies
//! upstream in reach, the river starts at the node. Whatever a
//! trough drains into is a trough, carrying the larger catchment, so a
//! river runs on to its outlet wherever the level's ground takes it.
//!
//! A hollow holds a lake: the footprint is flooded from its outlets, the
//! sea and its edge, lowest water first, so the flood leaves each hollow
//! over the lowest point of its rim at the level it filled to, and water
//! that would end in a hollow takes the flood's way out from where it
//! reaches the lake. A vertex
//! the flood reached above its own ground stands in the lake at that level.
//! The river leaving a lake cuts its outlet as it cuts any floor: the
//! lake's surface is lowered toward its outlet's graded profile, as far
//! as the plate's age has carried erosion, and a vertex the lowered
//! surface no longer covers is dry. A lake is the base level of the
//! rivers above it, and its floor is its surface; the floors below never
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
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::geom::{facet, parent_of, pos3, vertex_cells, vnbr, wander, xy};
use super::mask::Mask;
use super::paged::Paged;
use super::{drained, graded_slope, head_of, Level, Params, Pass};

pub(super) const WALK_SEED: u64 = 0x5761_6c6b_5f5f_5f5f; // "Walk____"

/// How many of the level's steps the water from the node upstream is
/// followed looking for a node: four steps of the level above.
const FOLLOW: usize = 12;

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
pub(super) fn route_blocks(lv: &mut Level, par: &Level, pend: &[(i64, i64)], p: &Params, births: bool) -> [u64; 4] {
    let lists = lv.block_lists(Pass::Route, pend);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(lists.len());
    let next = AtomicUsize::new(0);
    let shown: &Level = lv;
    let blocks: Vec<Block> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let nn = shown.cap();
                    let mut scratch = Scratch { stamp: Paged::new(0, nn), loc: Paged::new(-1, 3 * nn) };
                    let mut out = Vec::new();
                    loop {
                        let b = next.fetch_add(1, Ordering::Relaxed);
                        let Some(block) = lists.get(b) else { break };
                        out.push(route_block(shown, par, p, births, b, block, &mut scratch));
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
            lv.rec[d.v] = d.rec;
            lv.trough[d.v] = d.trough;
            lv.acc[d.v] = d.acc;
            lv.flow[d.v] = d.flow;
            lv.graded[d.v] = d.graded;
            lv.room[d.v] = d.room;
            lv.lake[d.v] = d.lake;
            lv.floor[d.v] = d.floor;
            lv.born[d.v] = d.born;
        }
        for (a, b) in stats.iter_mut().zip(block.stats) {
            *a += b;
        }
    }
    for (own, _) in &lists {
        for &c in own {
            lv.routed[c] = true;
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
        stamp[c] = cur;
    }
    let vids: Vec<usize> = fpl.iter().flat_map(|&c| (0..3).map(move |t| t * nn + c)).collect();
    for (x, &v) in vids.iter().enumerate() {
        loc[v] = x as i64;
    }
    let nv = vids.len();
    let local = |g: usize| -> Option<usize> { (loc[g] >= 0).then(|| loc[g] as usize) };
    let mut known = vec![false; nv];
    let mut sink = vec![false; nv];
    let mut decided = vec![false; nv];
    let mut key = vec![(0.0f64, 0i64, 0i64); nv];
    let mut at = vec![(0.0f64, 0.0f64); nv];
    let mut pos = vec![(0i64, 0i64); nv];
    for (x, &v) in vids.iter().enumerate() {
        let (t, i, j) = lv.vertex(v);
        let (cells, m) = vertex_cells(t, i, j);
        known[x] = cells[..m].iter().all(|&(a, bb)| lv.idx(a, bb).is_none_or(|c| stamp[c] == cur && lv.supplied[c]));
        let (q3, r3) = pos3(t, i, j, w);
        key[x] = (lv.sunk[v], 2 * r3 + q3, q3);
        at[x] = xy(q3, r3);
        pos[x] = (q3, r3);
        if known[x] && lv.ground[v] < 0.0 {
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
    // the sea, and the vertices at its edge it cannot route.
    let mut came = vec![usize::MAX; nv];
    let mut lake = vec![f64::NAN; nv];
    let mut seen = vec![false; nv];
    let mut heap = BinaryHeap::new();
    for x in 0..nv {
        if known[x] && (sink[x] || !decided[x]) {
            seen[x] = true;
            heap.push(Reverse(Flood { level: key[x].0, key: key[x], x }));
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
            heap.push(Reverse(Flood { level: f.level.max(key[y].0), key: key[y], x: y }));
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
    // The level above's nodes over the footprint, upstream first: where
    // each one's river is on the level, its catchment and its birth.
    let under = Mask::new(fpl.iter().filter_map(|&c| {
        let (i, j) = lv.ij(c);
        let (pi, pj, _, _) = parent_of(i, j);
        par.idx(pi, pj)
    }));
    let mut nodes: Vec<(usize, usize)> = Vec::new();
    for pc in under.iter() {
        for t in 0..3 {
            let u = t * pnn + pc;
            if !par.trough[u] {
                continue;
            }
            let (ut, ui, uj) = par.vertex(u);
            let (q3, r3) = pos3(ut, ui, uj, par.w);
            if let Some(x) = lv.vid_at(q3, r3).and_then(local) {
                nodes.push((u, x));
            }
        }
    }
    let at_node: HashMap<usize, usize> = nodes.iter().copied().collect();
    let mut upstream: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut indeg_n: HashMap<usize, u32> = nodes.iter().map(|&(u, _)| (u, 0)).collect();
    for &(u, _) in &nodes {
        let r = par.rec[u];
        if r >= 0 && at_node.contains_key(&(r as usize)) {
            upstream.entry(r as usize).or_default().push(u);
            *indeg_n.get_mut(&(r as usize)).expect("a node") += 1;
        }
    }
    let mut ready: Vec<usize> = indeg_n.iter().filter(|&(_, &d)| d == 0).map(|(&u, _)| u).collect();
    ready.sort_unstable();
    let reach2 = {
        let r = 0.5 * par.w as f64 * crate::SQRT_3;
        r * r
    };
    let mut placed: HashMap<usize, usize> = HashMap::new();
    let mut src_acc = vec![f64::NAN; nv];
    let mut src_flow = vec![f64::NAN; nv];
    let mut src_graded = vec![f64::NAN; nv];
    let mut src_born = vec![-1i64; nv];
    while let Some(u) = ready.pop() {
        let xu = at_node[&u];
        let mut ups = upstream.get(&u).cloned().unwrap_or_default();
        ups.sort_unstable();
        let near = |y: usize| {
            let (dx, dy) = (at[y].0 - at[xu].0, at[y].1 - at[xu].1);
            dx * dx + dy * dy <= reach2
        };
        // Where the water from upstream comes near the node; else where
        // following it stopped while it still runs; else, where it ran
        // out short of the node, at the node.
        let (mut hit, mut stop) = (None, None);
        for q in &ups {
            let mut y = placed[q];
            let mut ended = false;
            for _ in 0..FOLLOW {
                if near(y) {
                    hit = Some(y);
                    break;
                }
                match rec[y] {
                    Some(z) => y = z,
                    None => {
                        ended = true;
                        break;
                    }
                }
            }
            if hit.is_none() && near(y) {
                hit = Some(y);
            }
            if hit.is_some() {
                break;
            }
            if !ended {
                stop = stop.or(Some(y));
            }
        }
        let x = hit.or(stop).unwrap_or(xu);
        placed.insert(u, x);
        if !sink[x] {
            src_acc[x] = src_acc[x].max(par.acc[u]);
            src_flow[x] = src_flow[x].max(par.flow[u]);
            src_graded[x] = src_graded[x].min(par.graded[u]);
            src_born[x] = src_born[x].max(par.born[u]);
        }
        let r = par.rec[u];
        if r >= 0 {
            if let Some(d) = indeg_n.get_mut(&(r as usize)) {
                *d -= 1;
                if *d == 0 {
                    ready.push(r as usize);
                }
            }
        }
    }
    // Where water from outside the footprint may arrive unseen.
    let mut own_flag = vec![false; nv];
    for &c in own {
        for t in 0..3 {
            own_flag[loc[t * nn + c] as usize] = true;
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
        let (head, above) = (head_of(w, lv.erod[c], slope[x], p), head_of(par.w, lv.erod[c], slope[x], p));
        let gathered = area + inflow[x];
        val[x] = gathered.min(above);
        let kept = if gathered > 0.0 { val[x] / gathered } else { 0.0 };
        wet[x] = (if area > 0.0 { area * lv.rain[c] } else { 0.0 } + wet_in[x]) * kept;
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
                let up = graded[y] + graded_slope(acc[x], lv.erod[v % nn], p) * tiles;
                above_sea(src_graded[x].min(up).max(graded[y]))
            } else if sink[x] && lv.ground[v] < 0.0 {
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
    let mut lowered = lake.clone();
    for x in 0..nv {
        let Some(o) = outlet[x] else { continue };
        let c = vids[o] % nn;
        let level = above_sea(drained(lake[x], graded[o], lv.age[c], lv.erod[c], room(o), p));
        lowered[x] = if level > key[x].0 { level } else { f64::NAN };
    }
    let lake = lowered;
    graded = grade(&lake);
    // Floors: drainage's under the level's own dry ground at each
    // vertex, toward its graded profile and held to the room its
    // hillslopes give; along a trough never above the floors draining
    // into it, upstream first. A lake's floor is its surface.
    let mut flo: Vec<f64> = (0..nv)
        .map(|x| {
            if !lake[x].is_nan() {
                return above_sea(lake[x]);
            }
            let v = vids[x];
            let c = v % nn;
            drained(lv.ground[v], graded[x], lv.age[c], lv.erod[c], room(x), p)
        })
        .collect();
    let mut low = vec![f64::INFINITY; nv];
    for &x in &order {
        if !trough[x] {
            continue;
        }
        if lake[x].is_nan() {
            flo[x] = flo[x].min(low[x]);
        }
        if let Some(y) = rec[x] {
            low[y] = low[y].min(flo[x]);
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
        loc[v] = -1;
    }
    Block { decided: out, stats }
}
