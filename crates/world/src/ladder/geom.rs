//! The ladder's lattice: each level's cells on the world's axial tiles, `w`
//! tiles apart, nested aperture-nine in one orientation; a level's vertices,
//! its cells' centres and corners.

use crate::SQRT_3;

/// A cell's nine children by their offset from three times its index: the
/// centre, the six around it, and the two corners it owns of the six it
/// shares.
pub const CHILD_OFFSETS: [(i64, i64); 9] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1), (1, 1), (2, -1)];

/// The centres of a cell's descendants `depth` levels down, as tile
/// offsets from its own centre: `9^depth` of them, each standing for an
/// equal share of the cell.
pub fn descendants(w: i64, depth: u32) -> Vec<(i64, i64)> {
    let mut cells = vec![(0i64, 0i64)];
    let mut width = w;
    for _ in 0..depth {
        cells = cells.iter().flat_map(|&(a, b)| CHILD_OFFSETS.iter().map(move |&(oi, oj)| (3 * a + oi, 3 * b + oj))).collect();
        width /= 3;
    }
    cells.into_iter().map(|(a, b)| (a * width, b * width)).collect()
}

/// The neighbours of a cell.
pub const NB: [(i64, i64); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// An axis child sits at the centroid of the fan between these two of its
/// parent's corners.
pub fn axis_fan(oi: i64, oj: i64) -> Option<(usize, usize)> {
    match (oi, oj) {
        (1, 0) => Some((0, 1)),
        (0, 1) => Some((1, 2)),
        (-1, 1) => Some((2, 3)),
        (-1, 0) => Some((3, 4)),
        (0, -1) => Some((4, 5)),
        (1, -1) => Some((5, 0)),
        _ => None,
    }
}

/// A cell's parent one level up and its offset from three times that parent.
/// The nine offsets fall in the nine residue pairs mod 3, one each.
pub fn parent_of(i: i64, j: i64) -> (i64, i64, i64, i64) {
    let (a, b) = (i.rem_euclid(3), j.rem_euclid(3));
    let (oi, oj) = CHILD_OFFSETS.iter().copied().find(|&(oi, oj)| oi.rem_euclid(3) == a && oj.rem_euclid(3) == b).unwrap();
    ((i - oi).div_euclid(3), (j - oj).div_euclid(3), oi, oj)
}

/// A cell's ancestor `m` levels up.
pub fn ancestor(mut i: i64, mut j: i64, m: u32) -> (i64, i64) {
    for _ in 0..m {
        let (a, b, _, _) = parent_of(i, j);
        i = a;
        j = b;
    }
    (i, j)
}

/// Every cell offset within hex distance `d`, with its distance.
pub fn hex_offsets(d: i64) -> Vec<(i64, i64, i64)> {
    let mut out = Vec::new();
    for di in -d..=d {
        for dj in -d..=d {
            let k = di.abs().max(dj.abs()).max((di + dj).abs());
            if k <= d {
                out.push((di, dj, k));
            }
        }
    }
    out
}

/// Whether two cells of a coarser lattice are the same or neighbours.
pub fn in_ring(a: (i64, i64), b: (i64, i64)) -> bool {
    let (da, db) = (a.0 - b.0, a.1 - b.1);
    da.abs().max(db.abs()).max((da + db).abs()) <= 1
}

/// A vertex's position in thirds of a tile: a cell's centre, its corner A at
/// a third of the way toward (1, 1), its corner B at (2, -1).
pub fn pos3(t: u8, i: i64, j: i64, w: i64) -> (i64, i64) {
    let (tq, tr) = [(0, 0), (1, 1), (2, -1)][t as usize];
    ((3 * i + tq) * w, (3 * j + tr) * w)
}

/// A position in thirds of a tile, as world x and y: x = 1.5 q, y = √3 (r + q/2).
pub fn xy(q3: i64, r3: i64) -> (f64, f64) {
    let (q, r) = (q3 as f64 / 3.0, r3 as f64 / 3.0);
    (1.5 * q, SQRT_3 * (r + q / 2.0))
}

/// The vertex at a position in thirds of a tile, which must be one of the
/// level's: its kind and its cell.
pub fn vertex_at(q3: i64, r3: i64, w: i64) -> (u8, i64, i64) {
    let (a, b) = (q3.div_euclid(w), r3.div_euclid(w));
    let t = a.rem_euclid(3) as u8;
    let tr = [0, 1, -1][t as usize];
    (t, (a - t as i64).div_euclid(3), (b - tr).div_euclid(3))
}

/// A vertex's six neighbours on the fan graph: a centre's six corners, a
/// corner's three centres and three corners.
pub fn vnbr(t: u8, i: i64, j: i64, k: usize) -> (u8, i64, i64) {
    match t {
        0 => [(2, i, j), (1, i, j), (2, i - 1, j + 1), (1, i - 1, j), (2, i - 1, j), (1, i, j - 1)][k],
        1 => [(0, i, j), (0, i + 1, j), (0, i, j + 1), (2, i, j), (2, i - 1, j + 1), (2, i, j + 1)][k],
        _ => [(0, i, j), (0, i + 1, j - 1), (0, i + 1, j), (1, i, j - 1), (1, i, j), (1, i + 1, j - 1)][k],
    }
}

/// The cells sharing a vertex: a centre's own, a corner's three.
pub fn vertex_cells(t: u8, i: i64, j: i64) -> ([(i64, i64); 3], usize) {
    match t {
        0 => ([(i, j), (i, j), (i, j)], 1),
        1 => ([(i, j), (i + 1, j), (i, j + 1)], 3),
        _ => ([(i, j), (i + 1, j - 1), (i + 1, j)], 3),
    }
}


/// The nearest cell to fractional cell coordinates.
pub fn cube_round(u: f64, v: f64) -> (i64, i64) {
    let (x, z) = (u, v);
    let y = -x - z;
    let (mut rx, ry, mut rz) = (x.round(), y.round(), z.round());
    let (dx, dy, dz) = ((rx - x).abs(), (ry - y).abs(), (rz - z).abs());
    if dx > dy && dx > dz {
        rx = -ry - rz;
    } else if dz >= dy {
        rz = -rx - ry;
    }
    (rx as i64, rz as i64)
}

/// What one ring of ladder cells `w` tiles wide clears, in tiles: the gap
/// between a cell and the nearest cell two steps away, each a hexagon with
/// corners at 60k° and circumradius w/√3 tiles.
pub fn ring_clearance(w: i64) -> f64 {
    let rad = w as f64 / SQRT_3;
    let mut best = f64::INFINITY;
    for a in -2i64..=2 {
        for b in -2i64..=2 {
            if a.abs().max(b.abs()).max((a + b).abs()) != 2 {
                continue;
            }
            let (x, y) = xy(3 * a * w, 3 * b * w);
            let (x, y) = (x / SQRT_3, y / SQRT_3);
            let th = y.atan2(x);
            let sixth = std::f64::consts::PI / 3.0;
            let delta = (((th + sixth / 2.0).rem_euclid(sixth)) - sixth / 2.0).abs();
            best = best.min(x.hypot(y) - 2.0 * rad * delta.cos());
        }
    }
    best
}

/// Where water leaving a vertex goes: the two neighbours bracketing its
/// steepest downslope, and the share of it the first takes, or one
/// neighbour wholly where the downslope points straight at it. Each of
/// `around` is a neighbour's offset from the vertex, its height less the
/// vertex's, and its id. None where nothing lies below.
pub fn facet(around: &mut [(f64, f64, f64, usize)]) -> Option<(usize, usize, f64)> {
    around.sort_by(|a, b| a.1.atan2(a.0).total_cmp(&b.1.atan2(b.0)));
    let m = around.len();
    let (mut best, mut pick) = (1e-12, None);
    for k in 0..m {
        let (pax, pay, da, a) = around[k];
        let (pbx, pby, db, b) = around[(k + 1) % m];
        let mut th = pby.atan2(pbx) - pay.atan2(pax);
        if k == m - 1 {
            th += 2.0 * std::f64::consts::PI;
        }
        let (mut cand, mut share) = (-1.0, 1.0);
        if m >= 3 && th > 1e-9 && th < std::f64::consts::PI - 1e-9 {
            let det = pax * pby - pay * pbx;
            let gx = (da * pby - db * pay) / det;
            let gy = (pax * db - pbx * da) / det;
            let s = gx.hypot(gy);
            let rel = (pax * (-gy) - pay * (-gx)).atan2(pax * (-gx) + pay * (-gy));
            if s > 1e-12 && (0.0..=th).contains(&rel) {
                cand = s;
                share = 1.0 - rel / th;
            }
        }
        if cand < 0.0 {
            let (sa, sb) = (-da / pax.hypot(pay), -db / pbx.hypot(pby));
            (cand, share) = if sa >= sb { (sa, 1.0) } else { (sb, 0.0) };
        }
        if cand > best {
            best = cand;
            pick = Some((a, b, share));
        }
    }
    pick
}

/// The nearest point of a segment to a point: its share along the segment and
/// its distance, in tiles.
pub fn project(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
    (t, (p.0 - qx).hypot(p.1 - qy) / SQRT_3)
}

#[cfg(test)]
mod tests {
    /// A cell's descendants are its tiles once the depth reaches them: `w²`
    /// distinct tiles.
    #[test]
    fn descendants_are_the_cells_tiles() {
        for w in [3i64, 9, 27] {
            let mut d = descendants(w, w.ilog(3));
            assert_eq!(d.len() as i64, w * w);
            d.sort_unstable();
            d.dedup();
            assert_eq!(d.len() as i64, w * w);
        }
    }

    /// Neighbouring cells' tiles never overlap: the descendants partition
    /// the plane.
    #[test]
    fn neighbouring_cells_share_no_tile() {
        let w = 27i64;
        let own: std::collections::HashSet<(i64, i64)> = descendants(w, 3).into_iter().collect();
        for (di, dj) in NB {
            for (q, r) in descendants(w, 3) {
                assert!(!own.contains(&(q + di * w, r + dj * w)), "a tile in two cells");
            }
        }
    }

    use super::*;

    /// Every cell has one parent, and its nine children are its own.
    #[test]
    fn children_nest() {
        for a in -4..4 {
            for b in -4..4 {
                for (oi, oj) in CHILD_OFFSETS {
                    let (pi, pj, ri, rj) = parent_of(3 * a + oi, 3 * b + oj);
                    assert_eq!((pi, pj, ri, rj), (a, b, oi, oj));
                }
            }
        }
    }

    /// A vertex's position names it back.
    #[test]
    fn a_vertex_is_where_it_stands() {
        for w in [1, 3, 27] {
            for t in 0..3u8 {
                for (i, j) in [(0, 0), (-3, 5), (7, -2)] {
                    let (q3, r3) = pos3(t, i, j, w);
                    assert_eq!(vertex_at(q3, r3, w), (t, i, j));
                }
            }
        }
    }

    /// One ring of cells `w` tiles wide clears w/√3 tiles.
    #[test]
    fn one_ring_clears_a_cell_over_root_three() {
        for w in [3, 27, 729] {
            assert!((ring_clearance(w) - w as f64 / SQRT_3).abs() < 1e-9);
        }
    }
}
