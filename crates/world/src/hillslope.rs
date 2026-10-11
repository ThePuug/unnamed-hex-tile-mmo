//! Hillslopes as published fits make them: how fast ground erodes from the
//! relief it stands in, how sharp a crest stands at that rate, and how deep
//! a soil lies where bedrock makes soil against it. The ranges round their
//! crests by it and the forest reads the soil it leaves.
//!
//! The fits are Earth's, in metres and years. A z-level is 0.8 metres and
//! tiles lie √3 metres apart, the renderer's unit a metre, and a crest and
//! a soil are what the player stands on, so they go in uncompressed.
//!
//! Three steady-state relations, each where hillslopes lower as fast as the
//! ground beneath them is raised, and the decay that follows once the
//! raising stops ([`standing_after`]):
//!
//! - Erosion rises with relief, slowly over low ground and without bound
//!   toward the relief rock strength allows (Montgomery & Brandon 2002, the
//!   composite fit of their Eq. 2 to rates from cratons to the Himalaya).
//! - A soil-mantled crest is as curved as erosion is fast over how readily
//!   soil creeps: C = (ρr/ρs)·E/D (Roering et al. 2007), D calibrated on
//!   granitic hilltops by Hurst et al. (2012).
//! - Bedrock turns to soil fastest bare and slower under the soil it has
//!   made, exponentially with depth (Heimsath et al. 1997), so a soil
//!   stands where production meets erosion, h₀·ln(P₀/E) deep, and is gone
//!   where erosion outpaces the bare rate.

use crate::{RISE, SQRT_3};

/// Metres in a z-level.
const Z_METRES: f64 = RISE * SQRT_3;

/// Metres between neighbouring tiles.
const TILE_METRES: f64 = SQRT_3;

/// Erosion with no relief, in millimetres a year: chemical denudation
/// alone, the mean of the world's thirty-five largest drainage basins.
const BACKGROUND_EROSION: f64 = 0.01;

/// Erosion's rise per metre of relief over low ground, in millimetres a
/// year: Ahnert's line.
const RELIEF_EROSION: f64 = 2.5e-4;

/// The relief rock strength allows, in metres, which erosion climbs
/// without bound toward.
const RELIEF_LIMIT: f64 = 1_500.0;

/// The share of what erosion takes off a range that its surface falls by:
/// crust floats on the mantle, so the root rises back by the crust's
/// density over the mantle's of whatever is taken (Airy).
const ISOSTATIC_LOWERING: f64 = 1.0 - 2_700.0 / 3_300.0;

/// How long a range's relief takes to fall to a share 1/e of itself once
/// its convergence stops, in millions of years: relief erodes at what its
/// height drives over what level ground erodes at, and the surface falls
/// by [`ISOSTATIC_LOWERING`] of that. Some twenty-two, within the 2.5 to 69
/// fitted to the world's rivers' loads by orogen age (Pinet & Souriau 1988,
/// Pazzaglia & Brandon 1996).
pub const RELIEF_DECAY: f64 = 1.0 / (ISOSTATIC_LOWERING * RELIEF_EROSION * 1_000.0);

/// How readily soil creeps downslope, in square metres a year.
const TRANSPORT: f64 = 0.0086;

/// A soil's density as a share of the rock's it was made from.
const SOIL_DENSITY: f64 = 0.5;

/// The rate bare bedrock turns to soil, in millimetres a year: within the
/// 0.05 to 2 measured from southeastern Australia to northern California,
/// set by climate and rock and held one constant here. Tuning, to be judged
/// by eye.
pub const SOIL_PRODUCTION: f64 = 0.12;

/// How fast ground standing in `relief` z-levels of relief erodes, in
/// millimetres a year. The relief is under [`RELIEF_LIMIT`], where the
/// rate has no bound.
pub fn erosion_rate(relief: f64) -> f64 {
    let metres = relief.max(0.0) * Z_METRES;
    BACKGROUND_EROSION + RELIEF_EROSION * metres / (1.0 - (metres / RELIEF_LIMIT).powi(2))
}

/// The curvature a soil-mantled crest stands at where ground erodes at
/// `erosion` millimetres a year, in z-levels per tile squared: how fast its
/// slope falls away from level per tile across it.
pub fn crest_curvature(erosion: f64) -> f64 {
    let per_metre = erosion * 1e-3 / (SOIL_DENSITY * TRANSPORT);
    per_metre * TILE_METRES * TILE_METRES / Z_METRES
}

/// The share of a range's relief standing `myr` million years after its
/// convergence stopped: falling by e every [`RELIEF_DECAY`]. The relief
/// limit's term, under a twentieth at a range's height, is left out, so a
/// range falls that much slower at first. What survives longest is not
/// height but the contrast between rocks, which lithology and the
/// scarps carry.
pub fn standing_after(myr: f64) -> f64 {
    (-myr.max(0.0) / RELIEF_DECAY).exp()
}

/// How deep a soil lies where ground erodes at `erosion` millimetres a
/// year, as a share of the soil on ground with no relief: one there,
/// thinning as erosion climbs, nothing where it reaches
/// [`SOIL_PRODUCTION`]. The depth scale of the production law divides out.
pub fn soil_share(erosion: f64) -> f64 {
    ((SOIL_PRODUCTION / erosion).ln() / (SOIL_PRODUCTION / BACKGROUND_EROSION).ln()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The units: a z-level is 0.8 metres.
    #[test]
    fn a_z_level_is_eight_tenths_of_a_metre() {
        assert!((Z_METRES - 0.8).abs() < 1e-12);
    }

    /// Erosion is the background rate with no relief and rises with
    /// relief, faster the higher it stands.
    #[test]
    fn erosion_rises_with_relief() {
        assert_eq!(erosion_rate(0.0), BACKGROUND_EROSION);
        let rates: Vec<f64> = (0..10).map(|k| erosion_rate(k as f64 * 100.0)).collect();
        for w in rates.windows(3) {
            assert!(w[1] > w[0] && w[2] - w[1] > w[1] - w[0], "erosion does not climb faster with relief: {w:?}");
        }
    }

    /// A crest is sharper the faster its ground erodes.
    #[test]
    fn crests_sharpen_with_erosion() {
        let c: Vec<f64> = [0.01, 0.05, 0.1, 0.2].iter().map(|e| crest_curvature(*e)).collect();
        assert!(c.windows(2).all(|w| w[1] > w[0]) && c[0] > 0.0, "{c:?}");
    }

    /// A stopped range keeps all its relief at once and loses it over time,
    /// on the order the world's rivers measure.
    #[test]
    fn a_stopped_range_falls_on_the_measured_timescale() {
        assert_eq!(standing_after(0.0), 1.0);
        let s: Vec<f64> = (0..6).map(|k| standing_after(k as f64 * 20.0)).collect();
        assert!(s.windows(2).all(|w| w[1] < w[0]) && s[5] > 0.0, "{s:?}");
        assert!((2.5..=69.0).contains(&RELIEF_DECAY), "{RELIEF_DECAY} Myr");
    }

    /// The soil is whole with no relief, thins as erosion climbs, and is
    /// gone where erosion outpaces production.
    #[test]
    fn soil_thins_to_nothing_as_erosion_climbs() {
        assert!((soil_share(BACKGROUND_EROSION) - 1.0).abs() < 1e-12);
        let s: Vec<f64> = (0..8).map(|k| soil_share(erosion_rate(k as f64 * 80.0))).collect();
        assert!(s.windows(2).all(|w| w[1] <= w[0]), "{s:?}");
        assert_eq!(soil_share(SOIL_PRODUCTION), 0.0);
        assert_eq!(soil_share(2.0 * SOIL_PRODUCTION), 0.0);
    }
}
