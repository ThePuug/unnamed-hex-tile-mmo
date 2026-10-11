//! The weather: what the sky does over a place at a moment of game time,
//! read from the number and the place alone, so every reader of one moment
//! sees one sky. `design/weather.md` in the internal repo is the spec.
//!
//! Fronts are a slow field over the world that the wind carries. Each
//! front period draws a fresh field; a moment between two periods blends
//! the two fields so the sum keeps its spread and never jumps, each
//! carried by the wind from its own period's start. The season a period
//! starts in wets or dries it, and the leap season's periods carry
//! lightning storms. A moment is only ever floated as its difference from
//! a period's start (AGENTS.md anti-pattern 11).

use std::f64::consts::{FRAC_PI_2, PI};

use bevy::math::Vec3;

use crate::systems::{Date, DAY_MS, HOUR_MS, SEASON_MS};

/// The weather over a place, each a share from none to full.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weather {
    /// How much of the sky the cloud deck hides.
    pub cover: f32,
    /// How hard rain falls, or snow above the snow line.
    pub rain: f32,
    /// How thick the mist lies.
    pub mist: f32,
    /// How much of a lightning storm stands here. None outside the leap
    /// season.
    pub storm: f32,
}

/// How long one front field lasts before the next has replaced it, the
/// time a shower takes to come and go.
pub const FRONT_PERIOD_MS: u128 = 2 * HOUR_MS;
/// The size of a front, a rain region a few kilometres across: a player
/// sees one coming within the reach and can walk out of it.
const FRONT_SCALE_WU: f64 = 8_000.0;
/// The size of a mist bank.
const MIST_SCALE_WU: f64 = 3_000.0;

/// The wind carries anything `WIND_LATTICE_WU` across, along each axis a
/// whole number of times, in `WIND_PERIOD_MS`: anything periodic over that
/// lattice drifts with it and wraps without a seam. West-south-west, about
/// eight units a second.
pub const WIND_LATTICE_WU: f64 = 12_000.0;
pub const WIND_PERIOD_MS: u128 = 3_360_000;
const WIND_STEPS: (f64, f64) = (2.0, 1.0);

/// The wind's velocity in world units per millisecond, x and z.
pub fn wind() -> (f64, f64) {
    let per_ms = WIND_LATTICE_WU / WIND_PERIOD_MS as f64;
    (WIND_STEPS.0 * per_ms, WIND_STEPS.1 * per_ms)
}

/// How far the wind has carried a thing periodic over `WIND_LATTICE_WU`
/// at moment `ms`, within one lattice cell.
pub fn drift(ms: u128) -> (f64, f64) {
    let along = (ms % WIND_PERIOD_MS) as f64 / WIND_PERIOD_MS as f64;
    (
        (WIND_STEPS.0 * along).fract() * WIND_LATTICE_WU,
        (WIND_STEPS.1 * along).fract() * WIND_LATTICE_WU,
    )
}

/// How each season wets its fronts.
const SEASON_WETNESS: [f64; 5] = [0.04, -0.08, 0.04, 0.0, 0.05];
/// How much wider the leap season spreads its fronts: storms stand
/// between clear skies, and the meteor shower shows through the gaps.
const OMEN_SPREAD: f64 = 0.6;

/// One front period's field, carried by the wind from its start.
#[derive(Clone, Copy, Debug)]
struct Front {
    seed: u64,
    /// Where the field's own start lies from a place now, against the wind.
    shift: (f64, f64),
}

/// The sky at one moment: everything of the weather that is the same
/// everywhere, so a place's weather is a few noise reads.
#[derive(Clone, Copy, Debug)]
pub struct Forecast {
    fronts: [Front; 2],
    /// The two fronts' weights: their squares sum to one, so the blend of
    /// two independent fields keeps the spread of one.
    weights: (f64, f64),
    wetness: f64,
    omen: f64,
    morning: f64,
    /// One weather everywhere instead of the fronts, for looking at it.
    steady: Option<Weather>,
}

impl Forecast {
    /// The sky at moment `ms`.
    pub fn at(ms: u128) -> Self {
        let period = ms / FRONT_PERIOD_MS;
        let since = (ms - period * FRONT_PERIOD_MS) as f64;
        let until = FRONT_PERIOD_MS as f64 - since;
        let (vx, vz) = wind();
        let fronts = [
            Front { seed: period as u64, shift: (-vx * since, -vz * since) },
            Front { seed: period as u64 + 1, shift: (vx * until, vz * until) },
        ];
        let s = smoothstep(0.0, 1.0, since / FRONT_PERIOD_MS as f64);
        let weights = ((s * FRAC_PI_2).cos(), (s * FRAC_PI_2).sin());
        let start = |k: u128| Date::of(k * FRONT_PERIOD_MS).season;
        let (a, b) = (start(period), start(period + 1));
        let lerp = |x: f64, y: f64| x + (y - x) * s;
        Self {
            fronts,
            weights,
            wetness: lerp(SEASON_WETNESS[a], SEASON_WETNESS[b]),
            omen: lerp(f64::from(a == 4), f64::from(b == 4)),
            morning: morning(ms),
            steady: None,
        }
    }

    /// One weather everywhere, with `omen` of the leap season's sky over it,
    /// at moment `ms`.
    pub fn steady(ms: u128, weather: Weather, omen: f32) -> Self {
        Self { omen: omen as f64, steady: Some(weather), ..Self::at(ms) }
    }

    /// The weather over world position `x`, `z`.
    pub fn over(&self, x: f64, z: f64) -> Weather {
        if let Some(weather) = self.steady {
            return weather;
        }
        let field = |scale: f64, channel: u64| {
            let [a, b] = self.fronts;
            let read = |f: Front| fbm((x + f.shift.0) / scale, (z + f.shift.1) / scale, f.seed.wrapping_mul(2).wrapping_add(channel));
            self.weights.0 * read(a) + self.weights.1 * read(b)
        };
        let wet = 0.5 + field(FRONT_SCALE_WU, 0) * (1.0 + OMEN_SPREAD * self.omen) + self.wetness;
        let mist = 0.5 + field(MIST_SCALE_WU, 1);
        Weather {
            cover: smoothstep(0.4, 0.78, wet) as f32,
            rain: smoothstep(0.74, 0.95, wet) as f32,
            mist: (self.morning * smoothstep(0.65, 0.85, mist)) as f32,
            storm: (self.omen * smoothstep(0.6, 0.85, wet)) as f32,
        }
    }

    /// How much of the leap season holds, the same everywhere: its storms
    /// and its meteor shower.
    pub fn omen(&self) -> f32 {
        self.omen as f32
    }
}

/// Mist lies in the hours about dawn: from three, thickest from five to
/// seven, gone by ten.
fn morning(ms: u128) -> f64 {
    let hour = (ms % DAY_MS) as f64 / HOUR_MS as f64;
    smoothstep(3.0, 5.0, hour) * (1.0 - smoothstep(7.0, 10.0, hour))
}

/// The height precipitation turns to snow at, in z-levels: at the sea in
/// the depth of Freeze, on the high peaks alone at the height of Blaze.
/// The warmth turns once over the four seasons, warmest in the middle of
/// Blaze; a leap season holds the cold the year turns at.
pub fn snow_line(ms: u128) -> f32 {
    let (start, _) = Date::year(ms);
    let seasons = ((ms - start) as f64 / SEASON_MS as f64).min(4.0);
    let warmth = (FRAC_PI_2 * (seasons - 1.5)).cos();
    (SNOW_LINE_MID + SNOW_LINE_SWING * warmth) as f32
}
const SNOW_LINE_MID: f64 = 450.0;
const SNOW_LINE_SWING: f64 = 520.0;

/// A lightning strike: when, where it meets the ground, and the seed its
/// bolt's path is drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub at: u128,
    pub x: f64,
    pub z: f64,
    pub seed: u32,
}

/// Strikes are drawn on a lattice of cells and slots: each cell may strike
/// once a slot, with a chance that rises with the storm over it.
pub const STRIKE_SLOT_MS: u128 = 500;
const STRIKE_CELL_WU: f64 = 2_000.0;
/// A cell under a full storm strikes about once in seven and a half
/// minutes, so a storm standing over the view flashes every few seconds.
const STRIKE_CHANCE: f64 = STRIKE_SLOT_MS as f64 / 450_000.0;

/// Every strike from `from` to `to` within `radius` of `x`, `z`, under the
/// storms `forecast` stands, the same for every reader of one forecast. The
/// strikes keep their own clock: a sky held at one moment still flashes.
pub fn strikes(forecast: &Forecast, from: u128, to: u128, x: f64, z: f64, radius: f64) -> Vec<Strike> {
    let mut found = Vec::new();
    let reach = (radius / STRIKE_CELL_WU).ceil() as i64;
    let (ci, cj) = ((x / STRIKE_CELL_WU).floor() as i64, (z / STRIKE_CELL_WU).floor() as i64);
    for slot in from / STRIKE_SLOT_MS..=to / STRIKE_SLOT_MS {
        for i in ci - reach..=ci + reach {
            for j in cj - reach..=cj + reach {
                let draw = draws(hash(i, j, slot as u64 ^ STRIKE_SEED));
                if draw(0) >= STRIKE_CHANCE {
                    continue;
                }
                let sx = (i as f64 + draw(1)) * STRIKE_CELL_WU;
                let sz = (j as f64 + draw(2)) * STRIKE_CELL_WU;
                if (sx - x).hypot(sz - z) > radius {
                    continue;
                }
                let storm = forecast.over(sx, sz).storm as f64;
                if draw(3) >= storm * storm {
                    continue;
                }
                let at = slot * STRIKE_SLOT_MS + (draw(4) * STRIKE_SLOT_MS as f64) as u128;
                if (from..=to).contains(&at) {
                    found.push(Strike { at, x: sx, z: sz, seed: hash(i, j, slot as u64) as u32 });
                }
            }
        }
    }
    found
}
const STRIKE_SEED: u64 = 0x6c69_6768_746e;

/// A meteor: when it lights, how long it burns, and the directions it runs
/// between across the sky, along the great circle through both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meteor {
    pub at: u128,
    pub burns_ms: u128,
    pub from: Vec3,
    pub to: Vec3,
}

pub const METEOR_SLOT_MS: u128 = 1_000;
/// About one fireball every ten seconds over the whole sky in the leap
/// season.
const METEOR_CHANCE: f64 = 0.1;
/// The longest a fireball burns: a reader looking for the ones alight
/// looks this far back.
pub const METEOR_BURNS_MOST_MS: u128 = 4_000;

/// Every meteor lit from `from` to `to` in `forecast`'s leap season: a
/// fireball, the same over the whole world, burning for seconds on a long
/// arc that comes down across the sky, from high to the horizon over a
/// third of the sky's round, so it crosses the low sky the camera sees.
/// Day or night. The meteors keep their own clock, as the strikes do.
pub fn meteors(forecast: &Forecast, from: u128, to: u128) -> Vec<Meteor> {
    let mut found = Vec::new();
    for slot in from / METEOR_SLOT_MS..=to / METEOR_SLOT_MS {
        let draw = draws(hash(slot as i64, 0, METEOR_SEED));
        let start = slot * METEOR_SLOT_MS;
        if draw(0) >= METEOR_CHANCE * forecast.omen {
            continue;
        }
        let at = start + (draw(1) * METEOR_SLOT_MS as f64) as u128;
        if !(from..=to).contains(&at) {
            continue;
        }
        let bearing = 2.0 * PI * draw(4);
        let across = (60.0 + 60.0 * draw(2)) * if draw(6) < 0.5 { -1.0 } else { 1.0 };
        found.push(Meteor {
            at,
            burns_ms: 1_500 + (draw(5) * (METEOR_BURNS_MOST_MS - 1_500) as f64) as u128,
            from: direction(bearing, (25.0 + 20.0 * draw(3)).to_radians()),
            to: direction(bearing + across.to_radians(), (8.0 * draw(7)).to_radians()),
        });
    }
    found
}
const METEOR_SEED: u64 = 0x6d65_7465_6f72;

/// Unit direction at bearing `azimuth` from +x toward +z and `elevation`
/// above the horizon.
fn direction(azimuth: f64, elevation: f64) -> Vec3 {
    Vec3::new((elevation.cos() * azimuth.cos()) as f32, elevation.sin() as f32, (elevation.cos() * azimuth.sin()) as f32)
}

fn smoothstep(from: f64, to: f64, x: f64) -> f64 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(i: i64, j: i64, seed: u64) -> u64 {
    let mut h = seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (j as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

/// A hash's low bits as a share in [0, 1).
fn unit(h: u64) -> f64 {
    (h & 0xff_ffff) as f64 / (1u64 << 24) as f64
}

/// Independent shares drawn from one hash, the `n`th by rehashing it.
fn draws(h: u64) -> impl Fn(i64) -> f64 {
    move |n| unit(hash(n, 0, h))
}

/// Value noise at `x`, `y` in lattice units, about zero, within ±1.
fn value(x: f64, y: f64, seed: u64) -> f64 {
    let (i, j) = (x.floor(), y.floor());
    let (fx, fy) = (x - i, y - j);
    let ease = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (ux, uy) = (ease(fx), ease(fy));
    let at = |di: i64, dj: i64| 2.0 * unit(hash(i as i64 + di, j as i64 + dj, seed)) - 1.0;
    let bottom = at(0, 0) + (at(1, 0) - at(0, 0)) * ux;
    let top = at(0, 1) + (at(1, 1) - at(0, 1)) * ux;
    bottom + (top - bottom) * uy
}

/// Two octaves of value noise, with a spread of about a quarter.
fn fbm(x: f64, y: f64, seed: u64) -> f64 {
    FBM_GAIN * (value(x, y, seed) + 0.5 * value(2.0 * x + 17.3, 2.0 * y - 5.1, seed ^ 0x5bd1_e995))
}
const FBM_GAIN: f64 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    /// A Monday's midnight, the first of March 2026's five seasons.
    const MARCH: u128 = 1_772_409_600_000;

    fn grid(forecast: &Forecast) -> Vec<Weather> {
        (0..60).flat_map(|i| (0..60).map(move |j| (i as f64 * 1_500.0, j as f64 * 1_500.0)))
            .map(|(x, z)| forecast.over(x, z))
            .collect()
    }

    /// One moment and place read one weather, however often it is read.
    #[test]
    fn the_weather_is_the_moment_and_the_place() {
        let ms = MARCH + 2 * DAY_MS + 7 * HOUR_MS;
        assert_eq!(Forecast::at(ms).over(1_234.5, -9_876.0), Forecast::at(ms).over(1_234.5, -9_876.0));
        let omen = MARCH + 4 * SEASON_MS;
        let forecast = Forecast::at(omen);
        assert_eq!(strikes(&forecast, omen, omen + 60_000, 0.0, 0.0, 16_000.0), strikes(&Forecast::at(omen), omen, omen + 60_000, 0.0, 0.0, 16_000.0));
    }

    /// The sky never jumps: across a front period's turn, a millisecond
    /// moves the weather by a hair, and the season's turn likewise.
    #[test]
    fn the_weather_never_jumps() {
        for turn in [MARCH + 7 * FRONT_PERIOD_MS, MARCH + 4 * SEASON_MS, MARCH + SEASON_MS] {
            let (before, after) = (Forecast::at(turn - 1), Forecast::at(turn));
            for (x, z) in [(0.0, 0.0), (5_000.0, -3_000.0), (-40_000.0, 22_000.0)] {
                let (a, b) = (before.over(x, z), after.over(x, z));
                for (p, q) in [(a.cover, b.cover), (a.rain, b.rain), (a.mist, b.mist), (a.storm, b.storm)] {
                    assert!((p - q).abs() < 1e-3, "{a:?} → {b:?} at the turn {turn}");
                }
            }
        }
    }

    /// Weather is regional: over a stretch of country some of the sky is
    /// clear and some rains, and the clouds stand wider than the rain.
    #[test]
    fn some_places_are_clear_while_others_rain() {
        let weathers = grid(&Forecast::at(MARCH + 10 * DAY_MS + 13 * HOUR_MS));
        let share = |f: &dyn Fn(&Weather) -> bool| weathers.iter().filter(|w| f(w)).count() as f64 / weathers.len() as f64;
        let (clear, raining, clouded) = (share(&|w| w.cover == 0.0), share(&|w| w.rain > 0.0), share(&|w| w.cover > 0.0));
        assert!(clear > 0.0 && raining > 0.0, "clear {clear}, raining {raining}");
        assert!(clouded > raining, "rain falls from clouds");
    }

    /// Storms stand in the leap season and nowhere else in time.
    #[test]
    fn storms_belong_to_the_leap_season() {
        let ordinary = MARCH + 2 * SEASON_MS + 3 * DAY_MS;
        let omen = MARCH + 4 * SEASON_MS + 3 * DAY_MS;
        assert!(grid(&Forecast::at(ordinary)).iter().all(|w| w.storm == 0.0));
        assert!(grid(&Forecast::at(omen)).iter().any(|w| w.storm > 0.0));
        assert_eq!(Forecast::at(ordinary).omen(), 0.0);
        assert_eq!(Forecast::at(omen).omen(), 1.0);
        let (ordinary_sky, omen_sky) = (Forecast::at(ordinary), Forecast::at(omen));
        assert!(strikes(&ordinary_sky, ordinary, ordinary + 600_000, 0.0, 0.0, 16_000.0).is_empty());
        assert!(meteors(&ordinary_sky, ordinary, ordinary + 600_000).is_empty());
        assert!(!meteors(&omen_sky, omen, omen + 60_000).is_empty());
    }

    /// A strike falls where a storm stands: never under a clear sky.
    #[test]
    fn lightning_strikes_under_the_storm() {
        let omen = MARCH + 4 * SEASON_MS + 2 * DAY_MS;
        let forecast = Forecast::at(omen);
        let mut struck = 0;
        for (x, z) in [(0.0, 0.0), (30_000.0, -10_000.0), (-25_000.0, 45_000.0)] {
            for strike in strikes(&forecast, omen, omen + 10 * 60_000, x, z, 16_000.0) {
                struck += 1;
                let storm = forecast.over(strike.x, strike.z).storm;
                assert!(storm > 0.0, "a strike at {strike:?} under no storm");
                assert!((strike.x - x).hypot(strike.z - z) <= 16_000.0);
            }
        }
        assert!(struck > 0, "the leap season's storms strike");
    }

    /// Every fireball comes down across the sky to the horizon, burning no
    /// longer than a reader looks back for, and they cross every side.
    #[test]
    fn fireballs_come_down_across_every_side() {
        let omen = MARCH + 4 * SEASON_MS + DAY_MS;
        let seen = meteors(&Forecast::at(omen), omen, omen + 30 * 60_000);
        assert!(seen.len() > 50);
        for m in &seen {
            assert!(m.to.y < m.from.y && m.to.y >= 0.0, "it comes down to the horizon");
            assert!(m.from.dot(m.to) < 0.6, "across a long arc");
            assert!(m.burns_ms <= METEOR_BURNS_MOST_MS);
        }
        for (x, z) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            assert!(seen.iter().any(|m| m.from.x * x + m.from.z * z > 0.7), "fireballs light toward ({x}, {z})");
        }
    }

    /// Snow lies lowest in Freeze and highest in Blaze.
    #[test]
    fn the_snow_line_follows_the_year() {
        let line = |season: u128| snow_line(MARCH + season * SEASON_MS + SEASON_MS / 2);
        assert!(line(1) > line(0) && line(0) > line(3), "Blaze {}, Thaw {}, Freeze {}", line(1), line(0), line(3));
        assert!(line(3) < 0.0, "snow to the sea in Freeze");
        let april = MARCH + 5 * SEASON_MS;
        assert!((snow_line(april - 1) - snow_line(april)).abs() < 1e-3, "the year turns without a step, a leap season or none");
        assert!((snow_line(april - SEASON_MS - 1) - snow_line(april - SEASON_MS)).abs() < 1e-3);
    }

    /// The wind wraps whole lattice cells: the drift a period carries is
    /// none at the period's turn.
    #[test]
    fn the_drift_wraps_on_its_lattice() {
        let (x, z) = drift(MARCH - MARCH % WIND_PERIOD_MS + WIND_PERIOD_MS);
        assert_eq!((x, z), (0.0, 0.0));
        let (vx, vz) = wind();
        let (x, z) = drift(MARCH - MARCH % WIND_PERIOD_MS + 1_000);
        assert!((x - vx * 1_000.0).abs() < 1e-6 && (z - vz * 1_000.0).abs() < 1e-6);
    }
}
