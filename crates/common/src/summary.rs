//! The summary sampling rule: what one height and one water surface stand
//! for a group of tiles at a distance, read from seven of them. Every
//! producer of a summary — the client's map, the server, the flyover, the
//! world viewer — reads this one rule, or their silhouettes differ where
//! they meet.

/// Nested LoD levels: summary scales triple per level.
///
/// scale = 2r+1 ∈ {1, 3, 9, 27, 81}. Tripling makes the levels nest:
/// every coarse summary center is also a fine summary center, and
/// [`sample_offsets`]' 7 sample points at d = scale/3 land exactly on the
/// child level's summary centers (INV-006). Arbitrary integer radii do not
/// nest — adjacent-band lattices would share no structure at all. The
/// ladder ends where its outer edge reaches about one plate.
pub const LOD_LEVELS: [u32; 5] = [0, 1, 4, 13, 40];

/// The width of a summary of radius `r`, in tiles: the step of its lattice.
pub fn scale(r: u32) -> i32 {
    2 * r as i32 + 1
}

/// The center tile of the summary at lattice coordinates `(sq, sr)` on the
/// lattice of radius `r`: the tile grid scaled by the summary's width.
pub fn center_tile(r: u32, sq: i32, sr: i32) -> (i32, i32) {
    (sq * scale(r), sr * scale(r))
}

/// The seven tiles a summary of radius `r` is read from, as offsets from
/// its center: the center and one a third of the width out along each hex
/// axis. For nested levels (scale divisible by 3) the six land exactly on
/// the child level's centers (INV-006), so refinement keeps the
/// silhouette. At r=1 the seven are the whole hexball.
pub fn sample_offsets(r: u32) -> [(i32, i32); 7] {
    let d = scale(r) / 3;
    [(0, 0), (d, 0), (-d, 0), (0, d), (0, -d), (d, -d), (-d, d)]
}

/// Sample 7 elevations (center + 6 hex-axis points) and select center_z.
pub fn sample_center_z(r: u32, sq: i32, sr: i32, mut elevation_at: impl FnMut(i32, i32) -> i32) -> i32 {
    sample_center_z_opt(r, sq, sr, |q, rr| Some(elevation_at(q, rr))).expect("infallible elevation source")
}

/// Fallible variant of [`sample_center_z`] for tile sources with holes
/// (the client map while chunks stream in). None unless all 7 samples are
/// available.
pub fn sample_center_z_opt(r: u32, sq: i32, sr: i32, mut elevation_at: impl FnMut(i32, i32) -> Option<i32>) -> Option<i32> {
    let (cq, cr) = center_tile(r, sq, sr);
    let mut zs = [0i32; 7];
    for (i, (dq, dr)) in sample_offsets(r).into_iter().enumerate() {
        zs[i] = elevation_at(cq + dq, cr + dr)?;
    }
    Some(select_center_z(&zs))
}

/// The water surface a summary carries: the surface a majority of its seven
/// samples share, or None. A lake wider than the summary keeps its surface;
/// a river narrower than it vanishes into its valley, the way a valley
/// narrower than a summary vanishes under the height rule. `water_at` is a
/// tile's surface, or None where it is dry.
pub fn sample_center_water(r: u32, sq: i32, sr: i32, mut water_at: impl FnMut(i32, i32) -> Option<i32>) -> Option<i32> {
    let (cq, cr) = center_tile(r, sq, sr);
    let mut ws = [None; 7];
    for (i, (dq, dr)) in sample_offsets(r).into_iter().enumerate() {
        ws[i] = water_at(cq + dq, cr + dr);
    }
    select_center_water(&ws)
}

/// The surface more than half of the samples share, or None.
pub fn select_center_water(ws: &[Option<i32>]) -> Option<i32> {
    let mut best: Option<(i32, usize)> = None;
    for &w in ws.iter().flatten() {
        let n = ws.iter().filter(|&&x| x == Some(w)).count();
        if best.map_or(true, |(_, bn)| n > bn) {
            best = Some((w, n));
        }
    }
    best.filter(|&(_, n)| 2 * n > ws.len()).map(|(w, _)| w)
}

/// Select center_z using extremal deviation from the mean: the tile
/// furthest from the mean, and the higher on a tie, so a peak survives
/// the summary and a valley does. 0 for no samples.
pub fn select_center_z(tile_zs: &[i32]) -> i32 {
    if tile_zs.is_empty() {
        return 0;
    }
    if tile_zs.len() == 1 {
        return tile_zs[0];
    }

    let sum: f64 = tile_zs.iter().map(|&z| z as f64).sum();
    let mean = sum / tile_zs.len() as f64;

    let mut best_z = tile_zs[0];
    let mut best_dev = (tile_zs[0] as f64 - mean).abs();

    for &z in &tile_zs[1..] {
        let dev = (z as f64 - mean).abs();
        if dev > best_dev || (dev == best_dev && z > best_z) {
            best_z = z;
            best_dev = dev;
        }
    }

    best_z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_center_z_single_tile() {
        assert_eq!(select_center_z(&[42]), 42);
    }

    /// Water needs a majority of the samples at one surface: four of seven
    /// carry it, three do not, and four split between two surfaces do not.
    #[test]
    fn select_center_water_needs_a_majority_at_one_surface() {
        let s = Some(5);
        assert_eq!(select_center_water(&[s, s, s, s, None, None, None]), Some(5));
        assert_eq!(select_center_water(&[s, s, s, None, None, None, None]), None);
        assert_eq!(select_center_water(&[s, s, Some(6), Some(6), None, None, None]), None);
        assert_eq!(select_center_water(&[s, s, s, s, Some(6), Some(6), Some(6)]), Some(5));
        assert_eq!(select_center_water(&[None; 7]), None);
    }

    #[test]
    fn select_center_z_empty() {
        assert_eq!(select_center_z(&[]), 0);
    }

    #[test]
    fn select_center_z_uniform() {
        assert_eq!(select_center_z(&[5, 5, 5, 5]), 5);
    }

    #[test]
    fn select_center_z_peak() {
        assert_eq!(select_center_z(&[1, 2, 3, 2, 10]), 10);
    }

    #[test]
    fn select_center_z_valley() {
        assert_eq!(select_center_z(&[10, 10, 10, 10, 0]), 0);
    }

    #[test]
    fn select_center_z_tie_prefers_higher() {
        assert_eq!(select_center_z(&[5, 0, 10, 5]), 10);
    }

    #[test]
    fn select_center_z_symmetric_tie() {
        assert_eq!(select_center_z(&[0, 10]), 10);
    }

    /// INV-006: each LoD level's 7 sample points land exactly on the child
    /// level's summary centers — the sampling rule is hierarchical.
    #[test]
    fn lod_levels_sample_points_are_child_centers() {
        for w in LOD_LEVELS.windows(2) {
            let (child_r, parent_r) = (w[0], w[1]);
            assert_eq!(scale(parent_r), 3 * scale(child_r), "levels must triple");
            for &(sq, sr) in &[(0, 0), (1, 0), (-2, 3), (5, -4)] {
                let (cq, cr) = center_tile(parent_r, sq, sr);
                for (dq, dr) in sample_offsets(parent_r) {
                    let (q, r) = (cq + dq, cr + dr);
                    assert_eq!(q % scale(child_r), 0, "sample q={q} not a child center");
                    assert_eq!(r % scale(child_r), 0, "sample r={r} not a child center");
                }
            }
        }
    }
}
