//! The climate: the regional temperature at sea level, a slow field across
//! the plates, less the lapse rate up the ground. The world has no latitude,
//! so this field is the only climate a region has. The forest reads it for
//! its treeline and its kinds, the outcrops for where the cold bares rock.

use crate::noise::simplex_2d;
use crate::tectonic::PLATE_SPACING;
use super::thrusting::RANGE_RISE;

const CLIMATE_SEED: u64 = 0x636c_696d;

/// The growing-season isotherm trees stop at, in degrees: Körner's, the
/// same on every continent.
pub const TREELINE: f64 = 6.4;

/// The height the mean region's temperature reaches the treeline at, in
/// z-levels: a range and a half's rise, so a full plateau stands just
/// under the line in cold forest and a range's crest above it. The stack's
/// heights are their own scale, so the lapse rate is set against them and
/// never against a kilometre.
pub const TREELINE_RISE: f64 = 1.5 * RANGE_RISE;

/// The lapse rate in degrees per z-level, from [`TREELINE_RISE`].
pub const LAPSE: f64 = (SEA_LEVEL_MEAN - TREELINE) / TREELINE_RISE;

/// How many degrees above the treeline the forest thins to brush, and the
/// cold bares rock.
pub const TREELINE_BAND: f64 = 2.0;

/// Sea-level temperature across the world: a mean, a spread, and the
/// wavelength of the slow field carrying it, several plates.
pub const SEA_LEVEL_MEAN: f64 = 18.0;
pub const SEA_LEVEL_SPREAD: f64 = 10.0;
pub const CLIMATE_WAVELENGTH: f64 = 4.0 * PLATE_SPACING;

/// The regional temperature at sea level: the world has no latitude, so
/// this slow field is the only climate a region has.
pub fn sea_level_temperature(wx: f64, wy: f64, seed: u64) -> f64 {
    SEA_LEVEL_MEAN + SEA_LEVEL_SPREAD * simplex_2d(wx / CLIMATE_WAVELENGTH, wy / CLIMATE_WAVELENGTH, seed ^ CLIMATE_SEED)
}

/// The temperature at a position `elevation` z-levels up: the regional
/// field less the lapse rate over the ground above the sea.
pub fn temperature(wx: f64, wy: f64, elevation: f64, seed: u64) -> f64 {
    sea_level_temperature(wx, wy, seed) - LAPSE * elevation.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex_to_world;

    const S: u64 = 0x9E3779B97F4A7C15;
    const SPAWN: (i32, i32) = (104_289, -4_677);

    /// The temperature falls with elevation at the lapse rate, so the
    /// treeline is a height, and the sea-level field stays in its spread.
    #[test]
    fn temperature_falls_at_the_lapse_rate() {
        let (wx, wy) = hex_to_world(SPAWN.0, SPAWN.1);
        let t0 = temperature(wx, wy, 0.0, S);
        assert!((t0 - SEA_LEVEL_MEAN).abs() <= SEA_LEVEL_SPREAD);
        assert!((temperature(wx, wy, 100.0, S) - (t0 - 100.0 * LAPSE)).abs() < 1e-9);
        assert_eq!(temperature(wx, wy, -50.0, S), t0, "the sea is at sea level");
    }
}
