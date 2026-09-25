//! The summary sampling rule: what one height, one water surface and one
//! canopy stand for a group of tiles at a distance, read from seven of
//! them. Every producer of a summary — the client's map, the server, the
//! flyover, the world viewer — reads this one rule, or their silhouettes
//! differ where they meet.

use serde::{Deserialize, Serialize};

use crate::cover::{Canopy, Cover, Outcrop};

/// What a summary is read from: tiles, each giving its height, the water
/// over it and its cover at once, so a source that materialises a tile
/// serves all three from one. None where the source has no tile yet.
pub trait SummarySource {
    fn sample(&self, q: i32, r: i32) -> Option<TileSample>;
}

/// One tile as a summary reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileSample {
    pub z: i32,
    /// The surface water stands at over the tile, or None where it is dry.
    pub water: Option<i32>,
    pub cover: Cover,
}

/// One summary: the height, the water surface over it or None where it is
/// dry, the canopy and the outcrop. What every cache holds and the wire
/// carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryCell {
    pub z: i32,
    pub water: Option<i32>,
    pub canopy: Canopy,
    pub outcrop: Outcrop,
}

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

/// How many tiles a summary is read from.
pub const SAMPLES: usize = 7;

/// The seven tiles a summary of radius `r` is read from, as offsets from
/// its center: the center and one a third of the width out along each hex
/// axis. For nested levels (scale divisible by 3) the six land exactly on
/// the child level's centers (INV-006), so refinement keeps the
/// silhouette. At r=1 the seven are the whole hexball.
pub fn sample_offsets(r: u32) -> [(i32, i32); SAMPLES] {
    let d = scale(r) / 3;
    [(0, 0), (d, 0), (-d, 0), (0, d), (0, -d), (d, -d), (-d, d)]
}

/// How many parts a summary's ground divides into.
pub const PARTS: usize = 9;

/// The tiles a summary's parts are read at, as offsets from its center:
/// the seven samples, then the two corners where it meets two neighbours
/// that the tie between the three gives it. Levels triple, so a summary's
/// ground is exactly nine of the next finer level's, and each offset is the
/// center of one: the nine are distinct modulo the width, and every finer
/// center is a part of exactly one summary. At r=1 they are every tile the
/// summary holds.
pub fn part_offsets(r: u32) -> [(i32, i32); PARTS] {
    let d = scale(r) / 3;
    let [a, b, c, e, f, g, h] = sample_offsets(r);
    [a, b, c, e, f, g, h, (d, d), (2 * d, -d)]
}

/// The summary on the lattice of radius `r` that reads tile `(q, rr)` as
/// one of its samples, at lattice coordinates, or None where none does.
/// The offsets are distinct modulo the width, so at most one summary a
/// level reads any tile.
pub fn sampled_by(r: u32, q: i32, rr: i32) -> Option<(i32, i32)> {
    let s = scale(r);
    sample_offsets(r).into_iter().find_map(|(dq, dr)| {
        let (cq, cr) = (q - dq, rr - dr);
        (cq.rem_euclid(s) == 0 && cr.rem_euclid(s) == 0).then(|| (cq.div_euclid(s), cr.div_euclid(s)))
    })
}

/// The summary at `(sq, sr)` on the lattice of radius `r`, read from its
/// seven samples: the height by [`select_center_z`], the water by
/// [`select_center_water`], the canopy by [`Canopy::of`], the outcrop by
/// [`Outcrop::of`]. None unless the
/// source has all seven (the client's map while chunks stream in).
pub fn summarize(r: u32, sq: i32, sr: i32, source: &impl SummarySource) -> Option<SummaryCell> {
    let (cq, cr) = center_tile(r, sq, sr);
    let mut zs = [0i32; SAMPLES];
    let mut ws = [None; SAMPLES];
    let mut covers = [Cover::NONE; SAMPLES];
    for (i, (dq, dr)) in sample_offsets(r).into_iter().enumerate() {
        let sample = source.sample(cq + dq, cr + dr)?;
        zs[i] = sample.z;
        ws[i] = sample.water;
        covers[i] = sample.cover;
    }
    Some(SummaryCell {
        z: select_center_z(&zs),
        water: select_center_water(&ws),
        canopy: Canopy::of(&covers),
        outcrop: Outcrop::of(&covers),
    })
}

/// The water surface a summary carries: the surface more than half of
/// the samples share, or None. A lake wider than the summary keeps its
/// surface; a river narrower than it vanishes into its valley, the way a
/// valley narrower than a summary vanishes under the height rule.
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
    use crate::cover::Content;

    /// A source with a hole gives no summary; a whole one gives the three
    /// rules' answers from the same seven tiles.
    #[test]
    fn a_summary_needs_all_seven_samples() {
        struct Flat(Option<(i32, i32)>);
        impl SummarySource for Flat {
            fn sample(&self, q: i32, r: i32) -> Option<TileSample> {
                if self.0 == Some((q, r)) {
                    return None;
                }
                let cover = Cover::NONE.with(0, Content::Pine);
                Some(TileSample { z: 5 + (q == 0 && r == 0) as i32 * 20, water: Some(9), cover })
            }
        }
        let (dq, dr) = sample_offsets(1)[3];
        assert_eq!(summarize(1, 0, 0, &Flat(Some((dq, dr)))), None);
        let cell = summarize(1, 0, 0, &Flat(None)).expect("every sample is there");
        assert_eq!(cell.z, 25, "the peak survives");
        assert_eq!(cell.water, Some(9));
        assert_eq!(cell.canopy.count(Content::Pine), SAMPLES as u16);
    }

    /// Every tile a summary samples names that summary, and no other tile
    /// names one: at each level, over a patch wider than its lattice.
    #[test]
    fn a_sample_names_the_summary_that_reads_it() {
        for &r in &LOD_LEVELS {
            let s = scale(r);
            let mut named = std::collections::HashMap::new();
            for sq in -3..=3 {
                for sr in -3..=3 {
                    let (cq, cr) = center_tile(r, sq, sr);
                    for (dq, dr) in sample_offsets(r) {
                        named.insert((cq + dq, cr + dr), (sq, sr));
                    }
                }
            }
            for q in -2 * s..=2 * s {
                for rr in -2 * s..=2 * s {
                    assert_eq!(sampled_by(r, q, rr), named.get(&(q, rr)).copied(), "r={r} tile ({q}, {rr})");
                }
            }
        }
    }

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

    /// The nine parts fall on every residue of the finer lattice modulo the
    /// width, so each finer center is a part of exactly one summary.
    #[test]
    fn parts_cover_every_finer_center_once() {
        for &r in &LOD_LEVELS[1..] {
            let d = scale(r) / 3;
            let residues: std::collections::HashSet<(i32, i32)> =
                part_offsets(r).into_iter().map(|(q, rr)| ((q / d).rem_euclid(3), (rr / d).rem_euclid(3))).collect();
            assert_eq!(residues.len(), PARTS, "r={r}");
            assert_eq!(part_offsets(r)[..SAMPLES], sample_offsets(r), "the samples lead, r={r}");
        }
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
