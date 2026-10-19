//! The top: the coarsest level routed once over its whole domain, flooded
//! from its edge and the sea, its catchment accumulated over the flood's
//! order and kept.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::geom::{facet, pos3, vnbr, xy};
use super::{drained, graded_slope, head_of, Level, Params};

/// A vertex waiting in the flood, lowest first, earliest pushed first among
/// equals.
#[derive(PartialEq)]
struct Waiting(f64, u64, usize);

impl Eq for Waiting {}

impl Ord for Waiting {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}

impl PartialOrd for Waiting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Every vertex's receiver, the top's catchment, graded profiles and floors. Water drains from
/// the sinks, the sea and the domain's edge, by a priority flood; off a
/// trough a vertex's water splits between the two neighbours bracketing its
/// downslope, both nearer the outlet in the flood's order, so a slope drains
/// in a spread rather than a comb of parallel streams; along a trough it goes
/// whole to the flood's receiver, so a trough never thins.
pub(super) fn route_top(lv: &mut Level, p: &Params) {
    let nn = lv.cap();
    let count = 3 * nn;
    let mut nbr = vec![[-1i64; 6]; count];
    let mut pos = vec![(0.0, 0.0); count];
    let mut tb = vec![0.0; count];
    for v in 0..count {
        let (t, i, j) = lv.vertex(v);
        if !lv.exists[v % nn] {
            continue;
        }
        for k in 0..6 {
            let (tt, ni, nj) = vnbr(t, i, j, k);
            if let Some(c) = lv.idx(ni, nj) {
                if lv.exists[c] {
                    nbr[v][k] = (tt as usize * nn + c) as i64;
                }
            }
        }
        let (q3, r3) = pos3(t, i, j, lv.w);
        pos[v] = xy(q3, r3);
        // Flats break by the ground's own height: a centre's, a corner's the
        // mean of the cells sharing it.
        let (cells, m) = super::geom::vertex_cells(t, i, j);
        let hs: Vec<f64> = cells[..m].iter().filter_map(|&(a, b)| lv.idx(a, b)).filter(|&c| lv.exists[c]).map(|c| lv.ground[c]).collect();
        tb[v] = hs.iter().sum::<f64>() / hs.len().max(1) as f64;
    }
    let e: Vec<f64> = (0..count)
        .map(|v| {
            let (t, i, j) = lv.vertex(v);
            let (q3, r3) = pos3(t, i, j, lv.w);
            lv.elev[v] + 1e-4 * tb[v] + 1e-7 * (r3 as f64 / 3.0 + q3 as f64 / 6.0)
        })
        .collect();
    let live: Vec<bool> = (0..count).map(|v| lv.exists[v % nn] && !lv.elev[v].is_nan()).collect();
    let sink: Vec<bool> = (0..count).map(|v| live[v] && (nbr[v].iter().any(|&u| u < 0) || lv.elev[v] < 0.0)).collect();

    // The flood.
    let mut rec = vec![-1i64; count];
    let mut closed = vec![false; count];
    let mut filled = e.clone();
    let mut order = Vec::with_capacity(count);
    let mut heap = BinaryHeap::new();
    let mut counter = 0u64;
    for v in 0..count {
        if sink[v] {
            closed[v] = true;
            heap.push(Waiting(e[v], counter, v));
            counter += 1;
        }
    }
    while let Some(Waiting(level, _, v)) = heap.pop() {
        order.push(v);
        for &u in &nbr[v] {
            if u < 0 || closed[u as usize] || !live[u as usize] {
                continue;
            }
            let u = u as usize;
            closed[u] = true;
            rec[u] = v as i64;
            let f = e[u].max(level);
            filled[u] = f;
            heap.push(Waiting(f, counter, u));
            counter += 1;
        }
    }
    let mut at = vec![usize::MAX; count];
    for (k, &v) in order.iter().enumerate() {
        at[v] = k;
    }

    // Each vertex's split, between two neighbours both nearer the outlet in
    // the flood's order.
    let mut r1 = rec.clone();
    let mut r2 = vec![-1i64; count];
    let mut w1 = vec![1.0f64; count];
    // How steep each vertex's downslope is, in steps per tile.
    let mut slope = vec![0.0f64; count];
    for v in 0..count {
        if sink[v] || rec[v] < 0 {
            continue;
        }
        let mut around: Vec<(f64, f64, f64, usize)> = nbr[v]
            .iter()
            .filter(|&&u| u >= 0 && live[u as usize])
            .map(|&u| {
                let u = u as usize;
                (pos[u].0 - pos[v].0, pos[u].1 - pos[v].1, filled[u] - filled[v], u)
            })
            .collect();
        let Some((a, b, share_a, fall)) = facet(&mut around) else { continue };
        slope[v] = fall * crate::SQRT_3;
        let ok_a = at[a] < at[v] && share_a > 1e-12;
        let ok_b = at[b] < at[v] && share_a < 1.0 - 1e-12;
        if ok_a && ok_b {
            r1[v] = a as i64;
            r2[v] = b as i64;
            w1[v] = share_a;
        } else if ok_a {
            r1[v] = a as i64;
        } else if ok_b {
            r1[v] = b as i64;
        }
    }

    // Catchment, flow and troughs over the flood's order, upstream first: a
    // trough is born where the catchment reaches the head, and whatever a
    // trough drains into is a trough.
    let head = |v: usize| head_of(lv.w, lv.erod[v % nn], slope[v], p);
    let mut acc: Vec<f64> = (0..count).map(|v| if v < nn && live[v] { (lv.w * lv.w) as f64 } else { 0.0 }).collect();
    let mut flow: Vec<f64> = (0..count).map(|v| if v < nn && live[v] { acc[v] * lv.rain[v] } else { 0.0 }).collect();
    let mut trough = vec![false; count];
    for &v in order.iter().rev() {
        if rec[v] < 0 {
            continue;
        }
        trough[v] = trough[v] || acc[v] >= head(v);
        if trough[v] {
            let r = rec[v] as usize;
            acc[r] += acc[v];
            flow[r] += flow[v];
            trough[r] = true;
        } else if r2[v] >= 0 {
            acc[r1[v] as usize] += w1[v] * acc[v];
            acc[r2[v] as usize] += (1.0 - w1[v]) * acc[v];
            flow[r1[v] as usize] += w1[v] * flow[v];
            flow[r2[v] as usize] += (1.0 - w1[v]) * flow[v];
        } else {
            acc[r1[v] as usize] += acc[v];
            flow[r1[v] as usize] += flow[v];
        }
    }
    // A vertex in closed ground stands under the water filling it.
    let flooded = |v: usize| filled[v] > e[v];
    // Each trough's room: how far its hillslopes run to the nearer divide,
    // on each side of its flow the farthest any vertex entering the troughs
    // within two steps lies from where it enters, the nearer side's.
    let trough: Vec<bool> = (0..count).map(|v| trough[v] && rec[v] >= 0).collect();
    let mut entry = vec![usize::MAX; count];
    for &v in &order {
        entry[v] = if trough[v] { v } else if rec[v] >= 0 { entry[rec[v] as usize] } else { usize::MAX };
    }
    let mut slope_len = vec![[0.0f64; 2]; count];
    for v in 0..count {
        let e = entry[v];
        if e == usize::MAX || e == v {
            continue;
        }
        let (gx, gy) = (pos[v].0 - pos[e].0, pos[v].1 - pos[e].1);
        let side = if rec[e] >= 0 {
            let y = rec[e] as usize;
            let (fx, fy) = (pos[y].0 - pos[e].0, pos[y].1 - pos[e].1);
            usize::from(fx * gy - fy * gx < 0.0)
        } else {
            0
        };
        let d = gx.hypot(gy) / crate::SQRT_3;
        slope_len[e][side] = slope_len[e][side].max(d);
    }
    let room = |v: usize| -> f64 {
        let mut best = slope_len[v];
        let mut take = |u: usize| {
            best[0] = best[0].max(slope_len[u][0]);
            best[1] = best[1].max(slope_len[u][1]);
        };
        for &u in &nbr[v] {
            if u < 0 {
                continue;
            }
            take(u as usize);
            for &u2 in &nbr[u as usize] {
                if u2 >= 0 {
                    take(u2 as usize);
                }
            }
        }
        if best[0] > 0.0 && best[1] > 0.0 { best[0].min(best[1]) } else { best[0].max(best[1]) }
    };
    // Each vertex's graded profile, outlets first: the sea's level at the
    // sea, a lake's surface over closed ground, and up from its receiver by
    // its channel's graded slope over the way between. Land at the domain's
    // edge drains where the top cannot see: its profile is not known.
    let mut graded = vec![f64::NAN; count];
    for &v in &order {
        let c = v % nn;
        graded[v] = if rec[v] < 0 {
            if lv.elev[v] < 0.0 { 0.0 } else { f64::NAN }
        } else if flooded(v) {
            filled[v]
        } else {
            let r = rec[v] as usize;
            let tiles = (pos[v].0 - pos[r].0).hypot(pos[v].1 - pos[r].1) / crate::SQRT_3;
            graded[r] + graded_slope(acc[v], lv.erod[c], p) * tiles
        };
    }
    // Floors: drainage's under each vertex, toward its graded profile and
    // held to its room; along a trough never above the floors draining into
    // it, upstream first.
    let mut floor: Vec<f64> = (0..count)
        .map(|v| {
            let c = v % nn;
            drained(lv.elev[v], graded[v], lv.age[c], lv.erod[c], room(v), p)
        })
        .collect();
    let mut low = vec![f64::INFINITY; count];
    for &v in order.iter().rev() {
        if !trough[v] {
            continue;
        }
        floor[v] = floor[v].min(low[v]);
        let r = rec[v] as usize;
        low[r] = low[r].min(floor[v]);
    }
    for v in 0..count {
        lv.rec[v] = rec[v];
        lv.acc[v] = acc[v];
        lv.graded[v] = graded[v];
        lv.flow[v] = flow[v];
        lv.trough[v] = trough[v];
        lv.floor[v] = (floor[v] + 0.5).floor();
        lv.born[v] = if lv.trough[v] { lv.w } else { -1 };
        lv.psurf[v] = tb[v];
    }
}
