//! Hillslopes as published fits make them: how fast ground erodes from the
//! relief it stands in, and how sharp a crest stands at that rate. The
//! ranges round their crests by it.
//!
//! The fits are Earth's, in metres and years. A z-level is 0.8 metres and
//! tiles lie √3 metres apart, the renderer's unit a metre, and a crest is
//! what the player stands on, so it goes in uncompressed.
//!
//! Two steady-state relations, each where hillslopes lower as fast as the
//! ground beneath them is raised:
//!
//! - Erosion rises with relief, slowly over low ground and without bound
//!   toward the relief rock strength allows (Montgomery & Brandon 2002, the
//!   composite fit of their Eq. 2 to rates from cratons to the Himalaya).
//! - A soil-mantled crest is as curved as erosion is fast over how readily
//!   soil creeps: C = (ρr/ρs)·E/D (Roering et al. 2007), D calibrated on
//!   granitic hilltops by Hurst et al. (2012).

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

/// How readily soil creeps downslope, in square metres a year.
const TRANSPORT: f64 = 0.0086;

/// A soil's density as a share of the rock's it was made from.
const SOIL_DENSITY: f64 = 0.5;

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
}
