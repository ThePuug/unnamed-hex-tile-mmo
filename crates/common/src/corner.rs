//! The corner rule: where three cells meet, each cell's own value at the
//! corner. Cells within a walkable span of each other meet; cells out of
//! reach leave a face between them. The terrain ladder draws every level by
//! it, and the tiles' ground, mesh and physics alike, where the world's
//! heights are the ladder's.

/// Each sharing cell's value at one corner, by the half-walkable rule. Cells
/// all within `w` of each other meet at the middle of their range; otherwise
/// each meets the one it reaches, and one that reaches two which cannot
/// reach each other meets the lower; one that reaches none keeps its height.
/// NaN marks a missing cell.
pub fn meet(h: [f64; 3], w: f64) -> [f64; 3] {
    let present = [!h[0].is_nan(), !h[1].is_nan(), !h[2].is_nan()];
    let (mut hi, mut lo) = (f64::NEG_INFINITY, f64::INFINITY);
    for x in 0..3 {
        if present[x] {
            hi = hi.max(h[x]);
            lo = lo.min(h[x]);
        }
    }
    let together = hi - lo <= w + 1e-9;
    let mid = (hi + lo) / 2.0;
    let mut out = [f64::NAN; 3];
    for x in 0..3 {
        if !present[x] {
            continue;
        }
        if together {
            out[x] = mid;
            continue;
        }
        let others = [(x + 1) % 3, (x + 2) % 3];
        let (y0, y1) = if others[0] < others[1] { (others[0], others[1]) } else { (others[1], others[0]) };
        let reaches = |y: usize| present[y] && (h[x] - h[y]).abs() <= w + 1e-9;
        let (r0, r1) = (reaches(y0), reaches(y1));
        out[x] = match (r0, r1) {
            (false, false) => h[x],
            (true, false) => (h[x] + h[y0]) / 2.0,
            (false, true) => (h[x] + h[y1]) / 2.0,
            (true, true) => (h[x] + h[y0].min(h[y1])) / 2.0,
        };
    }
    out
}

/// Each sharing cell's surface at one corner, where the cells stand at
/// `level` on the map and their surfaces run through `through`: who meets
/// whom is [`meet`]'s rule on the levels, and where cells meet, the surface
/// is the mean of what they run through, so ground lying in a plane is
/// drawn as that plane. A cell that meets none keeps its own. NaN in
/// `level` marks a missing cell.
pub fn meet_through(level: [f64; 3], through: [f64; 3], w: f64) -> [f64; 3] {
    let present = [!level[0].is_nan(), !level[1].is_nan(), !level[2].is_nan()];
    let (mut hi, mut lo, mut sum, mut n) = (f64::NEG_INFINITY, f64::INFINITY, 0.0, 0.0);
    for x in 0..3 {
        if present[x] {
            hi = hi.max(level[x]);
            lo = lo.min(level[x]);
            sum += through[x];
            n += 1.0;
        }
    }
    let together = hi - lo <= w + 1e-9;
    let mut out = [f64::NAN; 3];
    for x in 0..3 {
        if !present[x] {
            continue;
        }
        if together {
            out[x] = sum / n;
            continue;
        }
        let (y0, y1) = ((x + 1) % 3, (x + 2) % 3);
        let reaches = |y: usize| present[y] && (level[x] - level[y]).abs() <= w + 1e-9;
        out[x] = match (reaches(y0), reaches(y1)) {
            (false, false) => through[x],
            (true, false) => (through[x] + through[y0]) / 2.0,
            (false, true) => (through[x] + through[y1]) / 2.0,
            (true, true) => (through[x] + if level[y0] <= level[y1] { through[y0] } else { through[y1] }) / 2.0,
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cells that meet share one height at the corner, and three in a plane
    /// meet on it: the corner is their centroid.
    #[test]
    fn cells_in_reach_meet_on_their_plane() {
        let through = [10.4, 10.7, 11.3];
        let met = meet_through([10.0, 11.0, 11.0], through, 1.0);
        assert!(met.iter().all(|m| (m - 10.8).abs() < 1e-12), "{met:?}");
    }

    /// A wall stands exactly where [`meet`] stands one, whatever the cells
    /// run through: who meets is the levels' alone.
    #[test]
    fn walls_stand_where_the_levels_put_them() {
        let levels = [[10.0, 10.0, 12.0], [10.0, 11.0, 12.0], [10.0, 12.0, 14.0], [10.0, 11.0, 11.0], [10.0, f64::NAN, 12.0], [10.0, f64::NAN, 11.0]];
        for level in levels {
            let by_level = meet(level, 1.0);
            for nudge in [[0.49, -0.5, 0.2], [-0.5, 0.49, -0.3], [0.0, 0.0, 0.0]] {
                let through = [level[0] + nudge[0], level[1] + nudge[1], level[2] + nudge[2]];
                let met = meet_through(level, through, 1.0);
                for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                    if level[a].is_nan() || level[b].is_nan() {
                        continue;
                    }
                    let (walled, walled_through) = ((by_level[a] - by_level[b]).abs() > 1e-9, (met[a] - met[b]).abs() > 1e-9);
                    assert!(walled <= walled_through || !walled, "{level:?}");
                    assert!(walled || !walled_through, "a wall the levels do not stand at {level:?} through {through:?}: {met:?}");
                }
            }
        }
    }

    /// Standing exactly at their levels, cells keep their own height where
    /// they reach none.
    #[test]
    fn a_cell_out_of_reach_keeps_its_own() {
        let met = meet_through([10.0, 13.0, 16.0], [10.2, 12.9, 16.4], 1.0);
        assert_eq!(met, [10.2, 12.9, 16.4]);
    }
}
