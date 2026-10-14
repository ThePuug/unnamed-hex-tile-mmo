//! Routing below the top: every vertex's receiver over a route cell and its
//! ring, on the ground the level above handed down and the level's layers
//! raised, every trough's catchment, base level and floor.
//!
//! A vertex on a line goes where the line goes; a line's end that is not sea
//! is a hollow. Every other vertex falls by steepest descent in the strict
//! order (height, the level above's surface, tilt, position). A line keeps
//! its own catchment, or the largest trough flowing into it if that is more;
//! a vertex off the lines gathers what drains into it, capped at the level
//! above's head, since what it could carry past that the level above already
//! carries in its own troughs; where it reaches the level's head a trough is
//! born. Where troughs are born, water short of the head splits between the
//! two neighbours bracketing its steepest downslope, both lower in the
//! strict order, so a slope drains in a spread rather than a comb of
//! parallel streams; a trough goes whole to its receiver and never thins.

use super::geom::{facet, pos3, vertex_cells, vnbr, xy};
use super::{drained, head_of, Level, Params, Pass};

/// Routes every pending route cell's own vertices. Returns the born troughs
/// whose upstream reached the footprint's edge, the own vertices that did,
/// own vertices left undecided, and own vertices in a cycle.
pub(super) fn route_blocks(lv: &mut Level, par: &Level, pend: &[(i64, i64)], p: &Params, births: bool) -> [u64; 4] {
    let nn = lv.cap();
    let w = lv.w;
    let head = if births { head_of(w, p) } else { f64::INFINITY };
    let cap = head_of(par.w, p);
    let lists = lv.block_lists(Pass::Route, pend);
    let mut stamp = super::paged::Paged::new(0u32, nn);
    let mut loc = super::paged::Paged::new(-1i64, 3 * nn);
    let mut stats = [0u64; 4];
    for (b, (own, fpl)) in lists.iter().enumerate() {
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
        let mut inherited = vec![false; nv];
        let mut ends = vec![false; nv];
        let mut key = vec![(0.0f64, 0.0f64, 0i64, 0i64); nv];
        let mut at = vec![(0.0f64, 0.0f64); nv];
        for (x, &v) in vids.iter().enumerate() {
            let (t, i, j) = lv.vertex(v);
            let (cells, m) = vertex_cells(t, i, j);
            known[x] = cells[..m].iter().all(|&(a, bb)| lv.idx(a, bb).is_none_or(|c| stamp[c] == cur && lv.supplied[c]));
            let (q3, r3) = pos3(t, i, j, w);
            key[x] = (lv.gelev[v], lv.psurf[v], 2 * r3 + q3, q3);
            at[x] = xy(q3, r3);
            ends[x] = lv.line_on[v] && lv.line_rec[v] < 0;
            if known[x] && lv.gelev[v] < 0.0 {
                sink[x] = true;
                decided[x] = true;
            }
        }
        let less = |a: usize, b: usize| -> bool {
            let (x, y) = (key[a], key[b]);
            x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)).then(x.2.cmp(&y.2)).then(x.3.cmp(&y.3)).is_lt()
        };
        let mut rec: Vec<Option<usize>> = vec![None; nv];
        let mut recg = vec![-1i64; nv];
        // Where water short of the head goes, and the first's share.
        let mut split: Vec<Option<(usize, Option<usize>, f64)>> = vec![None; nv];
        for x in 0..nv {
            if sink[x] {
                continue;
            }
            let v = vids[x];
            let forced = lv.line_rec[v];
            if forced >= 0 {
                inherited[x] = true;
                recg[x] = forced;
                rec[x] = local(forced as usize);
                decided[x] = true;
                continue;
            }
            if ends[x] {
                decided[x] = true;
                continue;
            }
            if !known[x] {
                continue;
            }
            let (t, i, j) = lv.vertex(v);
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
            if let Some(y) = best {
                rec[x] = Some(y);
                recg[x] = vids[y] as i64;
                if births {
                    if let Some((a, bb, share)) = facet(&mut around) {
                        let ok_a = less(a, x) && share > 1e-12;
                        let ok_b = less(bb, x) && share < 1.0 - 1e-12;
                        split[x] = match (ok_a, ok_b) {
                            (true, true) => Some((a, Some(bb), share)),
                            (true, false) => Some((a, None, 1.0)),
                            (false, true) => Some((bb, None, 1.0)),
                            (false, false) => None,
                        };
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
        let carried: Vec<bool> = (0..nv).map(|x| (inherited[x] || ends[x]) && !sink[x]).collect();
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
        // Catchment over the footprint in flow order.
        let mut indeg = vec![0u32; nv];
        for x in 0..nv {
            for y in succ(x).into_iter().flatten() {
                indeg[y] += 1;
            }
        }
        let mut val = vec![0.0f64; nv];
        let mut inflow = vec![0.0f64; nv];
        let mut inflow_max = vec![0.0f64; nv];
        let mut order = Vec::with_capacity(nv);
        let mut stack: Vec<usize> = (0..nv).filter(|&x| indeg[x] == 0).collect();
        while let Some(x) = stack.pop() {
            order.push(x);
            let v = vids[x];
            if carried[x] {
                val[x] = lv.line_acc[v].max(inflow_max[x]);
            } else {
                let c = v % nn;
                let area = if v < nn && lv.exists[c] { (w * w) as f64 } else { 0.0 };
                val[x] = (area + inflow[x]).min(cap);
            }
            let whole = carried[x] || val[x] >= head;
            for (y, frac) in shares(x, whole) {
                let Some(y) = y else { continue };
                if !carried[y] {
                    inflow[y] += frac * val[x];
                    if trunc[x] && frac > 0.0 {
                        trunc[y] = true;
                    }
                } else if whole && val[x] > inflow_max[y] {
                    inflow_max[y] = val[x];
                }
            }
            for y in succ(x).into_iter().flatten() {
                indeg[y] -= 1;
                if indeg[y] == 0 {
                    stack.push(y);
                }
            }
        }
        // Base levels, outlets first: the sea; where water off the lines joins
        // one, that line's floor at the junction; or the ground handed down
        // at the pit a vertex with nowhere lower to go stands in.
        let mut base = vec![f64::NAN; nv];
        for &x in order.iter().rev() {
            let v = vids[x];
            base[x] = if sink[x] {
                0.0
            } else if carried[x] {
                lv.line_base[v]
            } else if let Some(y) = rec[x] {
                if carried[y] { lv.line_fl[vids[y]] } else { base[y] }
            } else if decided[x] {
                lv.gelev[v]
            } else {
                f64::NAN
            };
        }
        // Each trough's room: how far its hillslopes run to the nearer
        // divide, on each side of its flow the farthest any vertex entering
        // the troughs within two steps lies from where it enters, the nearer
        // side's, read within the footprint.
        let trough_at: Vec<bool> = (0..nv).map(|x| (carried[x] || val[x] >= head) && !sink[x]).collect();
        let mut entry = vec![usize::MAX; nv];
        for &x in order.iter().rev() {
            entry[x] = if trough_at[x] { x } else { rec[x].map_or(usize::MAX, |y| entry[y]) };
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
        // Floors in flow order: a line hands down its own floor; off the
        // lines, the lowest of drainage's floors upstream, each measured on
        // the ground the level above handed down and held to the room its
        // hillslopes give, so a floor never rises and never reads the cut
        // water made.
        let mut flo = vec![f64::INFINITY; nv];
        let mut fl_in = vec![f64::INFINITY; nv];
        for &x in &order {
            let v = vids[x];
            if carried[x] {
                flo[x] = lv.line_fl[v];
            } else {
                let c = v % nn;
                // Never below the level it runs down to, whatever water
                // split from other routes brings: where the ground lies
                // lower, water stands at that level.
                let target = drained(lv.gelev[v], base[x], val[x], lv.age[c], lv.erod[c], room(x), p);
                let low = if target.is_nan() { fl_in[x] } else { fl_in[x].min(target) };
                flo[x] = if base[x].is_finite() { low.max(base[x]) } else { low };
            }
            let whole = carried[x] || val[x] >= head;
            for (y, frac) in shares(x, whole) {
                if let Some(y) = y {
                    if !carried[y] && frac > 0.0 {
                        fl_in[y] = fl_in[y].min(flo[x]);
                    }
                }
            }
        }
        let mut done = vec![false; nv];
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
            let trough = (carried[x] || val[x] >= head) && !sink[x];
            if trough && !carried[x] && trunc[x] {
                stats[0] += 1;
            }
            if trunc[x] && !carried[x] {
                stats[1] += 1;
            }
            lv.rec[v] = if sink[x] { -1 } else { recg[x] };
            lv.trough[v] = trough;
            lv.acc[v] = val[x];
            lv.floor[v] = (flo[x] + 0.5).floor();
            lv.base[v] = base[x];
            lv.born[v] = if carried[x] {
                lv.line_born[v]
            } else if trough {
                w
            } else {
                -1
            };
        }
        for &v in &vids {
            loc[v] = -1;
        }
        for &c in own {
            lv.routed[c] = true;
        }
    }
    stats
}
