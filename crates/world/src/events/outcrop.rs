//! OutcropEvent — where rock shows through the soil, and the boulders it
//! stands in a tile's slots. `design/outcrop.md` in the internal repo is
//! the spec; the claims below are the ones the code binds.
//!
//! # Claims
//!
//! Rock shows where erosion outruns soil. A slope carries its soil away,
//! and past a grade the rock cannot yield it as fast, so the share of bare
//! ground rises with the grade from a threshold to all of it at repose.
//! The threshold is the rock's and the cold's: shale weathers to clay fast
//! and holds a mantle to repose, harder rock bares on gentler ground in
//! proportion to how slowly a river cuts it, and above the treeline nothing
//! roots to hold a soil. Loose rock cannot stand steeper than repose, so
//! ground steeper than that is bedrock, and bedrock fills every slot.
//!
//! Exposure sets the boulders on ground that is not bedrock, each slot by
//! its own hash against a share of the exposure, so even a scree slope at
//! repose leaves most of its slots open and a range is still climbed.
//!
//! A tile reads its grade, its rock and its elevation from the layers
//! beneath, and nothing else. Drought, the rim a hard bed stands as at a
//! scarp, tors and sea cliffs are unbuilt.

use common::{Cover, Rock, TILE_SLOTS};

use crate::hex_to_world;
use crate::noise::hash_channel_f64;
use super::forest::{temperature, TREELINE, TREELINE_BAND};
use super::thrusting::{smoothstep, REPOSE_GRADE};
use super::{CellScope, TileOutput, TileView, WorldEvent};

const BOULDER_SEED: u64 = 0x626f_756c;

/// Where the hardest rock begins to bare, as a share of repose: basement
/// carries a thin soil and shows through it on moderate slopes. Shale holds
/// its mantle to repose, and the rocks between bare in proportion to their
/// erodibility.
pub const BARE_FROM: f64 = 0.5;

/// How far the cold lowers the threshold at and above the treeline, as a
/// share of it.
pub const COLD_BARING: f64 = 0.75;

/// The share of a tile's slots boulders take where the ground is all rock
/// and still loose: scree at repose.
pub const BOULDERS_AT_REPOSE: f64 = 0.3;

/// How much steeper than repose ground must be to read as bedrock: a
/// margin over the grade a scree slope stands at, which a half-tile read
/// at a brink can overstate.
pub const BEDROCK_OVER: f64 = 1.1;

/// The erodibility of the hardest rock, which bares from [`BARE_FROM`].
const HARDEST: f64 = 0.3;

/// The grade, in z-levels per world unit, past which a slope on `rock`
/// begins to show it, at a temperature `t`.
pub fn threshold(rock: Rock, t: f64) -> f64 {
    let hardness = ((1.0 - rock.erodibility()) / (1.0 - HARDEST)).clamp(0.0, 1.0);
    let cold = 1.0 - smoothstep((t - TREELINE) / TREELINE_BAND);
    REPOSE_GRADE * (1.0 - (1.0 - BARE_FROM) * hardness) * (1.0 - COLD_BARING * cold)
}

/// The share of a tile's ground that is rock, 0 to 1, at a grade over a
/// threshold: none below it, all of it at repose.
pub fn exposure(grade: f64, threshold: f64) -> f64 {
    if grade >= REPOSE_GRADE {
        return 1.0;
    }
    smoothstep((grade - threshold) / (REPOSE_GRADE - threshold).max(f64::EPSILON))
}

/// The share of a tile's slots boulders take: every one on bedrock, a share
/// of the exposure on loose ground.
pub fn boulder_share(grade: f64, threshold: f64) -> f64 {
    if grade > REPOSE_GRADE * BEDROCK_OVER {
        1.0
    } else {
        BOULDERS_AT_REPOSE * exposure(grade, threshold)
    }
}

/// A tile's boulders at a share: each slot by its own hash against it.
pub fn boulders_of(q: i32, r: i32, share: f64, rock: Rock, seed: u64) -> Cover {
    let mut cover = Cover::NONE;
    for k in 0..TILE_SLOTS as usize {
        if hash_channel_f64(q as i64, r as i64, seed ^ BOULDER_SEED, k as u64) < share {
            cover = cover.with_boulder(k);
        }
    }
    if cover.is_empty() { cover } else { cover.with_rock(rock) }
}

// ── The event ───────────────────────────────────────────────────────────────

/// Cell scale for the layer's own grid. It publishes nothing and reads no
/// index, so the scale only sets how often `prepare` runs.
pub const OUTCROP_CELL_SCALE: u32 = 1800;

pub struct OutcropEvent;

impl OutcropEvent {
    pub fn new() -> Self { OutcropEvent }
}

impl Default for OutcropEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for OutcropEvent {
    fn name(&self) -> &str { "outcrop" }
    fn scale(&self) -> u32 { OUTCROP_CELL_SCALE }

    /// Nothing to place: a tile's rock is a function of what lies beneath it.
    fn deform(&self, _scope: &CellScope) {}

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        _cell: &(dyn std::any::Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        if below.water.is_some() {
            return None;
        }
        let rock = below.rock?;
        let (wx, wy) = hex_to_world(q, r);
        let t = temperature(wx, wy, below.elevation, seed);
        let share = boulder_share(below.grade(), threshold(rock, t));
        if share <= 0.0 {
            return None;
        }
        let cover = boulders_of(q, r, share, rock, seed);
        if cover.is_empty() {
            return None;
        }
        Some(TileOutput { cover, ..TileOutput::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 0x9E3779B97F4A7C15;
    const WARM: f64 = 20.0;
    const ROCKS: [Rock; 4] = [Rock::Shale, Rock::Sandstone, Rock::Limestone, Rock::Basement];

    /// Harder rock bares on gentler ground, shale holds to repose, and the
    /// cold lowers every threshold.
    #[test]
    fn harder_and_colder_bare_sooner() {
        assert_eq!(threshold(Rock::Shale, WARM), REPOSE_GRADE);
        assert!((threshold(Rock::Basement, WARM) - BARE_FROM * REPOSE_GRADE).abs() < 1e-9);
        let mut by_hardness = ROCKS;
        by_hardness.sort_by(|a, b| b.erodibility().total_cmp(&a.erodibility()));
        for w in by_hardness.windows(2) {
            assert!(threshold(w[1], WARM) <= threshold(w[0], WARM), "{:?} bares after {:?}", w[1], w[0]);
        }
        for rock in ROCKS {
            assert!(threshold(rock, TREELINE) < threshold(rock, WARM));
        }
    }

    /// Exposure is none on flat ground, all of it at repose, and never falls
    /// as the grade rises; boulders fill every slot only past repose.
    #[test]
    fn exposure_rises_with_grade() {
        for rock in ROCKS {
            for t in [WARM, TREELINE] {
                let thr = threshold(rock, t);
                assert_eq!(exposure(0.0, thr), 0.0);
                assert_eq!(exposure(REPOSE_GRADE, thr), 1.0);
                let mut last = 0.0;
                for i in 0..=40 {
                    let g = REPOSE_GRADE * 1.5 * i as f64 / 40.0;
                    let s = boulder_share(g, thr);
                    assert!(exposure(g, thr) >= exposure(g * 0.99, thr));
                    assert!(s >= last, "{rock:?} at {t}: share fell at grade {g}");
                    assert!(s <= BOULDERS_AT_REPOSE || g > REPOSE_GRADE * BEDROCK_OVER);
                    last = s;
                }
                assert_eq!(boulder_share(REPOSE_GRADE * 2.0, thr), 1.0);
            }
        }
    }

    /// Boulders fill with their share and carry the rock they are.
    #[test]
    fn boulders_fill_with_their_share() {
        for (q, r) in [(0, 0), (104_289, -4_677), (-3_000_000, 1_000_000)] {
            assert!(boulders_of(q, r, 0.0, Rock::Sandstone, S).is_empty());
            let all = boulders_of(q, r, 1.0, Rock::Limestone, S);
            assert_eq!(all.fullness(), TILE_SLOTS);
            assert_eq!(all.rock(), Rock::Limestone);
            let mut last = 0;
            for i in 0..=20 {
                let n = boulders_of(q, r, i as f64 / 20.0, Rock::Basement, S).boulders().count();
                assert!(n >= last);
                last = n;
            }
        }
    }
}
