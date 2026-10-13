//! Each level's own trough lines, routed on its ground inside corridors
//! around the level above's.
//!
//! The level above's troughs are cut at their pins: a source, a junction, an
//! end, and both vertices of an edge whose straight path, spoke, rim and
//! spoke, crosses from one route cell into another. A segment runs between
//! two pins inside one route cell, or is one edge crossing between cells.
//! Every vertex within [`CORRIDOR`] cells of a segment's polyline goes to
//! the nearest segment whose cells hold it, so corridors share only pins and
//! lines never cross or touch. A line walks from its start pin to its end
//! pin: nearer the end at any height, or sideways only strictly lower; the
//! lowest such step by the strict order (height, the level above's surface,
//! tilt, position). Distance, then height, falls at every step. A line's
//! floor runs straight between its pins' floors by distance along it: set
//! once, where its trough was born.

use std::collections::HashMap;

use super::geom::{pos3, project, vertex_cells, vnbr, xy};
use super::{Level, Pass, CORRIDOR};

struct Segment {
    q3: Vec<i64>,
    r3: Vec<i64>,
    acc: Vec<f64>,
    fl: Vec<f64>,
    base: Vec<f64>,
    born: Vec<i64>,
    start: usize,
    end: usize,
    cells: Vec<(i64, i64)>,
}

/// A trough's end: the vertex its last edge reaches, with no trough edge
/// out: its catchment, floor, base level, and whether it ends in a hollow.
struct End {
    vid: usize,
    acc: f64,
    fl: f64,
    base: f64,
    born: i64,
    hollow: bool,
}

fn segments(lv: &Level, par: &Level, pmask: &[bool]) -> (Vec<Segment>, Vec<End>) {
    let pnn = par.n * par.n;
    let mut nxt: HashMap<usize, usize> = HashMap::new();
    for u in 0..3 * pnn {
        if par.trough[u] && par.rec[u] >= 0 && pmask[u % pnn] {
            nxt.insert(u, par.rec[u] as usize);
        }
    }
    let mut preds: HashMap<usize, Vec<usize>> = HashMap::new();
    for (&x, &y) in &nxt {
        preds.entry(y).or_default().push(x);
    }
    let blocks = &lv.blocks[Pass::Route as usize];
    let place = |p: usize| -> Option<((i64, i64), usize, (i64, i64))> {
        let (t, i, j) = par.vertex(p);
        let (q3, r3) = pos3(t, i, j, par.w);
        let v = lv.vid_at(q3, r3)?;
        Some(((q3, r3), v, blocks[v % (lv.n * lv.n)]))
    };
    let path_cells = |p: usize| -> Option<Vec<(i64, i64)>> {
        let ((qu, ru), _, _) = place(p)?;
        let ((qv, rv), _, _) = place(nxt[&p])?;
        let mut out = Vec::new();
        for f in 0..4 {
            let v = lv.vid_at(qu + f * (qv - qu) / 3, ru + f * (rv - ru) / 3)?;
            let c = blocks[v % (lv.n * lv.n)];
            if !out.contains(&c) {
                out.push(c);
            }
        }
        Some(out)
    };
    let crossing: HashMap<usize, Vec<(i64, i64)>> = nxt.keys().filter_map(|&p| Some((p, path_cells(p)?))).collect();
    let crosses = |p: usize| crossing.get(&p).is_none_or(|c| c.len() > 1);
    let fixed = |p: usize| -> bool {
        let ps = preds.get(&p).map_or(&[][..], |v| v.as_slice());
        ps.len() != 1 || !nxt.contains_key(&p) || crosses(p) || ps.iter().any(|&x| crosses(x))
    };
    let value = |p: usize| -> (f64, f64, f64) { (par.acc[p], par.floor[p], par.base[p]) };
    let mut starts: Vec<usize> = nxt.keys().copied().filter(|&p| fixed(p)).collect();
    starts.sort_unstable();
    let mut segs = Vec::new();
    for p in starts {
        let mut chain = vec![p];
        let mut x = nxt[&p];
        loop {
            chain.push(x);
            if fixed(x) || !nxt.contains_key(&x) || chain.len() > 4096 {
                break;
            }
            x = nxt[&x];
        }
        let places: Option<Vec<_>> = chain.iter().map(|&c| place(c)).collect();
        let Some(places) = places else { continue };
        let Some(cells) = crossing.get(&p) else { continue };
        let cells = if cells.len() > 1 { cells.clone() } else { vec![places[0].2] };
        let vals: Vec<(f64, f64, f64)> = chain.iter().map(|&c| value(c)).collect();
        segs.push(Segment {
            q3: places.iter().map(|p| p.0 .0).collect(),
            r3: places.iter().map(|p| p.0 .1).collect(),
            acc: vals.iter().map(|v| v.0).collect(),
            fl: vals.iter().map(|v| v.1).collect(),
            base: vals.iter().map(|v| v.2).collect(),
            born: chain.iter().map(|&c| par.born[c]).collect(),
            start: places[0].1,
            end: places[places.len() - 1].1,
            cells,
        });
    }
    let mut ends = Vec::new();
    let mut targets: Vec<usize> = nxt.values().copied().collect();
    targets.sort_unstable();
    targets.dedup();
    for y in targets {
        if nxt.contains_key(&y) {
            continue;
        }
        let Some((_, vid, _)) = place(y) else { continue };
        let (acc, fl, base) = value(y);
        // A line's catchment never falls: its end carries what runs into it.
        let acc = preds[&y].iter().map(|&x| par.acc[x]).fold(acc, f64::max);
        ends.push(End { vid, acc, fl, base, born: par.born[y], hollow: par.rec[y] < 0 && par.elev[y] >= 0.0 });
    }
    (segs, ends)
}

/// Lines for every pending route cell: each cell draws every segment it
/// holds part of and writes its own vertices. Returns the lines with no way
/// through, the lines stalled, and the segments whose pins lay outside the
/// footprint.
pub(super) fn line_blocks(lv: &mut Level, par: &Level, pend: &[(i64, i64)], fp: &[bool]) -> [u64; 3] {
    let nn = lv.n * lv.n;
    let mut pmask = vec![false; par.n * par.n];
    for c in 0..fp.len() {
        if fp[c] {
            let (i, j) = lv.ij(c);
            let (pi, pj, _, _) = super::geom::parent_of(i, j);
            if let Some(pc) = par.idx(pi, pj) {
                pmask[pc] = true;
            }
        }
    }
    let pmask: Vec<bool> = par.dilate(&pmask, 1).iter().zip(&par.exists).map(|(a, b)| *a && *b).collect();
    let (segs, ends) = segments(lv, par, &pmask);
    // Corridors are shared out in one order everywhere: the larger catchment
    // first, then position.
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_by(|&a, &b| segs[b].acc[0].total_cmp(&segs[a].acc[0]).then((segs[a].q3[0], segs[a].r3[0]).cmp(&(segs[b].q3[0], segs[b].r3[0]))));
    let lists = lv.block_lists(Pass::Route, pend);
    let mut stats = [0u64; 3];
    let mut loc: Vec<i64> = vec![-1; 3 * nn];
    let mut stamp: Vec<u32> = vec![0; nn];
    let half = CORRIDOR * lv.w as f64;
    let w = lv.w;
    for (b, &key) in pend.iter().enumerate() {
        let cur = b as u32 + 1;
        let (own, fpl) = &lists[b];
        for &c in fpl {
            stamp[c] = cur;
        }
        let vids: Vec<usize> = fpl.iter().flat_map(|&c| (0..3).map(move |t| t * nn + c)).collect();
        for (x, &v) in vids.iter().enumerate() {
            loc[v] = x as i64;
        }
        let nv = vids.len();
        let local = |v: usize, loc: &[i64]| -> Option<usize> { (loc[v] >= 0).then(|| loc[v] as usize) };
        let mut known = vec![false; nv];
        let mut key_of = vec![(0.0f64, 0.0f64, 0i64, 0i64); nv];
        let mut at = vec![(0.0f64, 0.0f64); nv];
        for (x, &v) in vids.iter().enumerate() {
            let (t, i, j) = lv.vertex(v);
            let (cells, m) = vertex_cells(t, i, j);
            known[x] = cells[..m].iter().all(|&(a, bb)| lv.idx(a, bb).is_none_or(|c| !lv.exists[c] || (stamp[c] == cur && lv.supplied[c])))
                && !lv.gelev[v].is_nan();
            let (q3, r3) = pos3(t, i, j, w);
            key_of[x] = (lv.gelev[v], lv.psurf[v], 2 * r3 + q3, q3);
            at[x] = xy(q3, r3);
        }
        let less = |a: usize, b: usize, key_of: &[(f64, f64, i64, i64)]| -> bool {
            let (x, y) = (key_of[a], key_of[b]);
            x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.2.cmp(&y.2)).then(x.3.cmp(&y.3)).is_lt()
        };
        let ring = |cell: (i64, i64)| super::geom::in_ring(cell, key);
        let comp: Vec<usize> = order.iter().copied().filter(|&s| segs[s].cells.iter().any(|&c| ring(c))).collect();
        let draw: Vec<usize> = order.iter().copied().filter(|&s| segs[s].cells.contains(&key)).collect();
        let mut pin = vec![false; nv];
        for &s in &comp {
            for v in [segs[s].start, segs[s].end] {
                if let Some(x) = local(v, &loc) {
                    pin[x] = true;
                }
            }
        }
        // Each vertex near a trough goes to the nearest segment whose cells
        // hold it, ties to the earlier segment in the order.
        let mut vbest = vec![f64::INFINITY; nv];
        let mut vseg = vec![usize::MAX; nv];
        let reach = CORRIDOR as i64 + 2;
        for &s in &comp {
            let sg = &segs[s];
            for m in 0..sg.q3.len() - 1 {
                let a = xy(sg.q3[m], sg.r3[m]);
                let bxy = xy(sg.q3[m + 1], sg.r3[m + 1]);
                let i0 = sg.q3[m].min(sg.q3[m + 1]).div_euclid(3 * w) - reach;
                let i1 = sg.q3[m].max(sg.q3[m + 1]).div_euclid(3 * w) + reach;
                let j0 = sg.r3[m].min(sg.r3[m + 1]).div_euclid(3 * w) - reach;
                let j1 = sg.r3[m].max(sg.r3[m + 1]).div_euclid(3 * w) + reach;
                for i in i0..=i1 {
                    for j in j0..=j1 {
                        let Some(c) = lv.idx(i, j) else { continue };
                        if stamp[c] != cur || !sg.cells.contains(&lv.blocks[Pass::Route as usize][c]) {
                            continue;
                        }
                        for t in 0..3 {
                            let Some(x) = local(t * nn + c, &loc) else { continue };
                            if !known[x] || pin[x] {
                                continue;
                            }
                            let (_, d) = project(at[x], a, bxy);
                            if d <= half && d < vbest[x] {
                                vbest[x] = d;
                                vseg[x] = s;
                            }
                        }
                    }
                }
            }
        }
        let mut dist = vec![-1i64; nv];
        let mut corr = vec![false; nv];
        for &s in &draw {
            let sg = &segs[s];
            let (Some(sx), Some(ex)) = (local(sg.start, &loc), local(sg.end, &loc)) else {
                stats[2] += 1;
                continue;
            };
            let members: Vec<usize> = (0..nv).filter(|&x| vseg[x] == s).chain([sx, ex]).collect();
            for &x in &members {
                corr[x] = true;
                dist[x] = -1;
            }
            // Distance to the end inside the corridor.
            let mut queue = std::collections::VecDeque::from([ex]);
            dist[ex] = 0;
            while let Some(x) = queue.pop_front() {
                let (t, i, j) = lv.vertex(vids[x]);
                for k in 0..6 {
                    let (tt, ni, nj) = vnbr(t, i, j, k);
                    let Some(gv) = lv.vid(tt, ni, nj) else { continue };
                    let Some(y) = local(gv, &loc) else { continue };
                    if !corr[y] || dist[y] >= 0 {
                        continue;
                    }
                    dist[y] = dist[x] + 1;
                    queue.push_back(y);
                }
            }
            let mut path = vec![sx];
            let mut x = sx;
            if dist[sx] < 0 {
                stats[0] += 1;
            } else {
                while x != ex && path.len() < nv {
                    let (t, i, j) = lv.vertex(vids[x]);
                    let mut best: Option<usize> = None;
                    for k in 0..6 {
                        let (tt, ni, nj) = vnbr(t, i, j, k);
                        let Some(gv) = lv.vid(tt, ni, nj) else { continue };
                        let Some(y) = local(gv, &loc) else { continue };
                        if !corr[y] || dist[y] < 0 || dist[y] > dist[x] {
                            continue;
                        }
                        if dist[y] == dist[x] && !less(y, x, &key_of) {
                            continue;
                        }
                        if best.is_none_or(|bb| less(y, bb, &key_of)) {
                            best = Some(y);
                        }
                    }
                    let Some(y) = best else {
                        stats[1] += 1;
                        break;
                    };
                    path.push(y);
                    x = y;
                }
            }
            if dist[sx] >= 0 && x == ex {
                // Catchment the level above's by distance along the corridor,
                // never falling, and the base level of the nearer end of the
                // edge it lies along. The floor runs straight between the two
                // pins' floors by distance along the line itself: a line's
                // floor is never drawn again below the level it was born at.
                let mut along = vec![0.0f64; path.len()];
                for m in 1..path.len() {
                    let (a0, a1) = (at[path[m - 1]], at[path[m]]);
                    along[m] = along[m - 1] + (a1.0 - a0.0).hypot(a1.1 - a0.1);
                }
                let length = along[path.len() - 1].max(1e-9);
                let (fl_up, fl_dn) = (sg.fl[0], sg.fl[sg.fl.len() - 1]);
                let mut acc = sg.acc[0];
                for m_ in 0..path.len() - 1 {
                    let x = path[m_];
                    let (mut bestd, mut ta, mut base, mut born) = (f64::INFINITY, acc, sg.base[0], sg.born[0]);
                    let mut anchor = [f64::NAN; 2];
                    for m in 0..sg.q3.len() - 1 {
                        let (a0, a1) = (xy(sg.q3[m], sg.r3[m]), xy(sg.q3[m + 1], sg.r3[m + 1]));
                        let (t_, d) = project(at[x], a0, a1);
                        if d < bestd - 1e-12 {
                            bestd = d;
                            anchor = [(a0.0 + t_ * (a1.0 - a0.0)) / crate::SQRT_3, (a0.1 + t_ * (a1.1 - a0.1)) / crate::SQRT_3];
                            ta = sg.acc[m] + t_ * (sg.acc[m + 1] - sg.acc[m]);
                            let end = if t_ < 0.5 { m } else { m + 1 };
                            base = sg.base[end];
                            born = sg.born[end];
                        }
                    }
                    acc = acc.max(ta);
                    let v = vids[x];
                    let fl = fl_up + (fl_dn - fl_up) * along[m_] / length;
                    let c = v % nn;
                    if lv.blocks[Pass::Route as usize][c] != key {
                        continue;
                    }
                    let (t, i, j) = lv.vertex(v);
                    let wall = (0..6).any(|k| {
                        let (tt, ni, nj) = vnbr(t, i, j, k);
                        lv.vid(tt, ni, nj).and_then(|gv| local(gv, &loc)).is_some_and(|y| !corr[y])
                    });
                    let pin_d = |p: usize| (at[x].0 - at[p].0).hypot(at[x].1 - at[p].1) / crate::SQRT_3;
                    lv.line_on[v] = true;
                    lv.line_rec[v] = vids[path[m_ + 1]] as i64;
                    lv.line_acc[v] = acc;
                    lv.line_fl[v] = (fl + 0.5).floor();
                    lv.line_base[v] = base;
                    lv.line_born[v] = born;
                    lv.line_anchor[v] = anchor;
                    lv.line_dev[v] = bestd;
                    lv.line_wall[v] = wall;
                    lv.line_pin[v] = pin_d(sx).min(pin_d(ex));
                }
            }
            for &x in &members {
                let v = vids[x];
                if vseg[x] == s && lv.blocks[Pass::Route as usize][v % nn] == key {
                    lv.line_corr[v] = sg.start as i64;
                }
            }
            for &x in &members {
                corr[x] = false;
            }
        }
        for &v in &vids {
            loc[v] = -1;
        }
        for &c in own {
            lv.lined[c] = true;
        }
    }
    // Where a trough ends, on cells this call lined.
    let mine: std::collections::HashSet<(i64, i64)> = pend.iter().copied().collect();
    for e in ends {
        if mine.contains(&lv.blocks[Pass::Route as usize][e.vid % nn]) {
            lv.line_on[e.vid] = true;
            lv.line_rec[e.vid] = -1;
            lv.line_acc[e.vid] = e.acc;
            lv.line_fl[e.vid] = (e.fl + 0.5).floor();
            lv.line_base[e.vid] = e.base;
            lv.line_born[e.vid] = e.born;
            let (t, i, j) = lv.vertex(e.vid);
            let (q3, r3) = super::geom::pos3(t, i, j, lv.w);
            let (x, y) = xy(q3, r3);
            lv.line_anchor[e.vid] = [x / crate::SQRT_3, y / crate::SQRT_3];
            lv.line_dev[e.vid] = 0.0;
            lv.line_pin[e.vid] = 0.0;
            lv.line_hollow[e.vid] = e.hollow;
        }
    }
    stats
}
