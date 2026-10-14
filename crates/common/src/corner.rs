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
