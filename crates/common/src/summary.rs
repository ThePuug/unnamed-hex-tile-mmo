//! The summary sampling rule: what one height, one water surface and one
//! outcrop stand for a group of tiles at a distance, read from seven of
//! them, and what canopy each of its nine parts wears, read at the part's
//! own tile. Every producer of a summary — the client's map, the server,
//! the flyover, the world viewer — reads this one rule, or their
//! silhouettes differ where they meet.

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

/// One summary: the height, which of its [`part_offsets`] parts stand
/// under water, the canopy of each part, and the outcrop. What every cache
/// holds and the wire carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryCell {
    pub z: i32,
    /// A bit to each part in [`part_offsets`] order, set where the part's
    /// tile stands under water above the sea.
    pub wet: u16,
    pub canopy: [Canopy; PARTS],
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

/// The growth sites one part of a summary of radius `r` holds: a finer
/// summary's tiles, whose ground is the width's third squared, at a tile's
/// sites each.
pub fn part_sites(r: u32) -> u32 {
    (scale(r) as u32 / 3).pow(2) * crate::cover::SITES.len() as u32
}

/// What stands on each part of a summary players have changed, counted
/// from its tiles: its pine, deciduous and brush. A part is counted the
/// first time a change lands in it, from its tiles as generated, which a
/// change has had built around the player by then; each change after moves
/// the count by what it took or left, so a part felled whole reads bare. A
/// part no change has reached is not counted, and wears its tile's reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartStats([Option<[u32; 3]>; PARTS]);

impl PartStats {
    /// Moves part `part` of a summary of radius `r` from what `before`
    /// holds to what `after` holds, counting it first by `count` — its
    /// trees as generated — where it is not counted yet, and answers the
    /// part's canopy now.
    pub fn change(&mut self, r: u32, part: usize, before: Cover, after: Cover, count: impl FnOnce() -> [u32; 3]) -> Canopy {
        let counts = self.0[part].get_or_insert_with(count);
        let (took, left) = (Canopy::tally(before), Canopy::tally(after));
        for kind in 0..3 {
            counts[kind] = (counts[kind] + left[kind]).saturating_sub(took[kind]);
        }
        Canopy::of_counts(*counts, part_sites(r))
    }
}

/// Each part's canopy of the summary at `(sq, sr)` on the lattice of
/// radius `r`, read at its own tile: what [`summarize`] reads, from a
/// source that may differ from the one the rest of the summary is read
/// from. None unless the source has all nine.
pub fn canopy_parts(r: u32, sq: i32, sr: i32, source: &impl SummarySource) -> Option<[Canopy; PARTS]> {
    let (cq, cr) = center_tile(r, sq, sr);
    let mut canopy = [Canopy::NONE; PARTS];
    for (part, (dq, dr)) in part_offsets(r).into_iter().enumerate() {
        canopy[part] = Canopy::of(source.sample(cq + dq, cr + dr)?.cover);
    }
    Some(canopy)
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
/// [`part_offsets`] tiles: the water by [`wet_parts`] and each part's
/// canopy by [`Canopy::of`] from its own tile, and the height and the
/// outcrop by [`select_center_z`] and [`Outcrop::of`] from the seven
/// samples, unless the summary holds water: then it stands at the highest
/// ground its water covers, so the water is drawn on the ground and runs
/// down its valley on the ground's own corners. None unless the source has
/// all nine (the client's map while chunks stream in).
pub fn summarize(r: u32, sq: i32, sr: i32, source: &impl SummarySource) -> Option<SummaryCell> {
    let (cq, cr) = center_tile(r, sq, sr);
    let mut read = [None; PARTS];
    for (i, (dq, dr)) in part_offsets(r).into_iter().enumerate() {
        read[i] = Some(source.sample(cq + dq, cr + dr)?);
    }
    let parts: [TileSample; PARTS] = read.map(|sample| sample.expect("every part was read"));
    let samples = &parts[..SAMPLES];
    let covers: Vec<Cover> = samples.iter().map(|s| s.cover).collect();
    let (wet, surface) = wet_parts(&parts);
    let z = match surface {
        Some(surface) => surface - 1,
        None => select_center_z(&samples.iter().map(|s| s.z).collect::<Vec<_>>()),
    };
    Some(SummaryCell {
        z,
        wet,
        canopy: parts.map(|part| Canopy::of(part.cover)),
        outcrop: Outcrop::of(&covers),
    })
}

/// Which of a summary's parts stand under water above the sea, a bit to
/// each, and the lowest surface over them, or None where none does. A
/// river is drawn wherever it crosses a part, however narrow the summary
/// would make it; a surface stepping down the river inside one summary
/// gives the summary its lowest step. The sea is one plane drawn to the
/// horizon, so a part under it alone is not wet here.
pub fn wet_parts(parts: &[TileSample; PARTS]) -> (u16, Option<i32>) {
    let mut wet = 0;
    let mut lowest: Option<i32> = None;
    for (i, surface) in parts.iter().enumerate().filter_map(|(i, p)| p.water.filter(|&w| w > 0).map(|w| (i, w))) {
        wet |= 1 << i;
        lowest = Some(lowest.map_or(surface, |l| l.min(surface)));
    }
    (wet, lowest)
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
    /// rules' answers from the seven samples, and a canopy at every part.
    #[test]
    fn a_summary_needs_every_part() {
        /// Ground at 5 with a peak at 25 in the middle, missing one tile,
        /// and water at 9 over one tile off the peak where it is wet.
        struct Flat(Option<(i32, i32)>, bool);
        impl SummarySource for Flat {
            fn sample(&self, q: i32, r: i32) -> Option<TileSample> {
                if self.0 == Some((q, r)) {
                    return None;
                }
                let cover = Cover::NONE.with(0, Content::Pine);
                let water = (self.1 && (q, r) == (1, 0)).then_some(9);
                Some(TileSample { z: 5 + (q == 0 && r == 0) as i32 * 20, water, cover })
            }
        }
        for (dq, dr) in [sample_offsets(1)[3], part_offsets(1)[PARTS - 1]] {
            assert_eq!(summarize(1, 0, 0, &Flat(Some((dq, dr)), false)), None);
        }
        let cell = summarize(1, 0, 0, &Flat(None, false)).expect("every part is there");
        assert_eq!((cell.z, cell.wet), (25, 0), "the peak survives");
        assert_eq!(cell.canopy, [Canopy::of(Cover::NONE.with(0, Content::Pine)); PARTS]);
        let cell = summarize(1, 0, 0, &Flat(None, true)).expect("every part is there");
        assert_eq!((cell.z, cell.wet), (8, 1 << 1), "the summary stands under its water");
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

    /// Every part under water above the sea is wet, however few, and the
    /// lowest of their surfaces is the summary's; the sea alone wets none.
    #[test]
    fn a_part_under_water_is_wet() {
        let tile = |water| TileSample { z: 3, water, cover: Cover::NONE };
        let mut parts = [tile(None); PARTS];
        assert_eq!(wet_parts(&parts), (0, None));
        parts[8] = tile(Some(6));
        assert_eq!(wet_parts(&parts), (1 << 8, Some(6)));
        parts[2] = tile(Some(5));
        assert_eq!(wet_parts(&parts), (1 << 8 | 1 << 2, Some(5)));
        parts[0] = tile(Some(0));
        assert_eq!(wet_parts(&parts), (1 << 8 | 1 << 2, Some(5)), "the sea is the plane's");
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

    /// A part is counted from its tiles on the first change and moved by
    /// exactly what each change takes: one of a part's nine pines takes its
    /// share from ten steps to nine, and the part felled whole reads bare
    /// whatever its tile read. The count is taken once.
    #[test]
    fn a_change_moves_its_part_by_what_it_took() {
        let r = 4;
        let pine = Cover::NONE.with(0, Content::Pine);
        let mut stats = PartStats::default();
        let mut counted = 0;
        let mut count = || {
            counted += 1;
            [9, 0, 0]
        };
        assert_eq!(stats.change(r, 3, pine, Cover::NONE, &mut count).share(Content::Pine), 9, "one pine of nine");
        for _ in 1..9 {
            stats.change(r, 3, pine, Cover::NONE, &mut count);
        }
        assert!(stats.change(r, 3, pine, pine, &mut count).is_empty(), "the part felled whole");
        assert_eq!(counted, 1);
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
