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

use crate::systems::{Date, DAY_MS, HOUR_MS};

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
/// How much dust each season's air carries and how damp it is, which a
/// low sun's fire leans by: Thaw's clean and pale, Blaze's dry, Ash's
/// the dustiest and reddest, Freeze's damp and pink.
const SEASON_DUST: [f64; 5] = [0.15, 0.7, 1.0, 0.3, 0.5];
const SEASON_DAMP: [f64; 5] = [0.3, 0.0, 0.1, 0.7, 0.4];
/// How far toward the low sun its light comes in under the deck: cloud
/// standing there, or twice as far, shuts the fire out.
const SUNWARD_WU: f64 = 10_000.0;
/// The size of a dust haze, and how long rain leaves the air washed.
const DUST_SCALE_WU: f64 = 20_000.0;
const WASHED_MS: [u128; 2] = [HOUR_MS, 2 * HOUR_MS];

/// The air a low sun's light comes through to a place, each a share
/// from none to full: what makes one sunrise unlike the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Air {
    /// How clear the far sky toward the sun is, where its light comes in.
    pub open: f32,
    /// How much dust hangs in it, which reddens and deepens the fire.
    pub dust: f32,
    /// How damp it is, which softens the fire to rose.
    pub damp: f32,
    /// How lately rain washed it, which pales and shortens the fire.
    pub washed: f32,
}
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
    dust: f64,
    damp: f64,
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
        // Square roots round alike on every platform, where a cosine's last
        // bit may not, so every reader blends to the same bit.
        let norm = ((1.0 - s) * (1.0 - s) + s * s).sqrt();
        let weights = ((1.0 - s) / norm, s / norm);
        let start = |k: u128| Date::of(k * FRONT_PERIOD_MS).season;
        let (a, b) = (start(period), start(period + 1));
        let lerp = |x: f64, y: f64| x + (y - x) * s;
        Self {
            fronts,
            weights,
            wetness: lerp(SEASON_WETNESS[a], SEASON_WETNESS[b]),
            dust: lerp(SEASON_DUST[a], SEASON_DUST[b]),
            damp: lerp(SEASON_DAMP[a], SEASON_DAMP[b]),
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
        let wet = 0.5 + self.field(x, z, FRONT_SCALE_WU, 0) * (1.0 + OMEN_SPREAD * self.omen) + self.wetness;
        let mist = 0.5 + self.field(x, z, MIST_SCALE_WU, 1);
        Weather {
            cover: smoothstep(0.4, 0.78, wet) as f32,
            rain: smoothstep(0.74, 0.95, wet) as f32,
            mist: (self.morning * smoothstep(0.65, 0.85, mist)) as f32,
            storm: (self.omen * smoothstep(0.6, 0.85, wet)) as f32,
        }
    }

    /// One of the fronts' fields at a place, about zero: the two periods'
    /// own, blended. Channels 0 and 1 are a seed's two fields; a channel
    /// past them reads a seed no period's own two reach.
    fn field(&self, x: f64, z: f64, scale: f64, channel: u64) -> f64 {
        let [a, b] = self.fronts;
        let read = |f: Front| fbm((x + f.shift.0) / scale, (z + f.shift.1) / scale, f.seed.wrapping_mul(2).wrapping_add(channel));
        self.weights.0 * read(a) + self.weights.1 * read(b)
    }

    /// The air a low sun's light comes through to `x`, `z` at moment
    /// `ms`, the forecast's own, with the sun on the bearing `sunward`, a
    /// unit vector in x and z. The same for every reader of one moment.
    pub fn air(&self, ms: u128, x: f64, z: f64, sunward: (f64, f64)) -> Air {
        let far = |times: f64| self.over(x + sunward.0 * SUNWARD_WU * times, z + sunward.1 * SUNWARD_WU * times).cover;
        let open = 1.0 - smoothstep(0.25, 0.85, far(1.0).max(far(2.0)) as f64);
        let (washed, damp, haze) = match self.steady {
            Some(weather) => (0.0, weather.mist as f64, 0.0),
            None => {
                let rained = WASHED_MS.iter().map(|&ago| Self::at(ms.saturating_sub(ago)).over(x, z).rain).fold(0.0, f32::max);
                // Past the two channels a period's seed holds, and the next's.
                (smoothstep(0.0, 0.4, rained as f64), self.field(x, z, DUST_SCALE_WU, 1_001), self.field(x, z, DUST_SCALE_WU, 1_003))
            }
        };
        Air {
            open: open as f32,
            dust: ((self.dust + 0.8 * haze).clamp(0.0, 1.0) * (1.0 - 0.6 * washed)) as f32,
            damp: (self.damp + if self.steady.is_some() { damp } else { 0.8 * damp }).clamp(0.0, 1.0) as f32,
            washed: washed as f32,
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
    let seasons = 4.0 * Date::turned(ms);
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
/// storms `forecast` stands, the same for every reader of one forecast. A
/// held calendar holds them, a stroke stopped as it was.
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

/// A meteor: when it lights, how long it burns, and its arc across the
/// sky: the direction it lights at, which may lie under the horizon, the
/// pole of the great circle it runs on, and how far round it runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meteor {
    pub at: u128,
    pub burns_ms: u128,
    pub lights: Vec3,
    pub pole: Vec3,
    pub span: f32,
}

impl Meteor {
    /// Where it stands after `share` of its run.
    pub fn along(&self, share: f32) -> Vec3 {
        let angle = self.span * share;
        self.lights * angle.cos() + self.pole.cross(self.lights) * angle.sin()
    }
}

pub const METEOR_SLOT_MS: u128 = 1_000;
/// About one fireball a minute in the leap season.
const METEOR_CHANCE: f64 = 1.0 / 60.0;
/// The longest a fireball burns: a reader looking for the ones alight
/// looks this far back.
pub const METEOR_BURNS_MOST_MS: u128 = 7_000;

/// Every meteor lit from `from` to `to` in `forecast`'s leap season: a
/// fireball on a great circle that rises over the horizon, climbs a
/// quarter of the way round to its top and sets on the far side. It
/// lights under the horizon and comes up over it burning, or lights in
/// the sky; it burns out in the sky, or goes down past the far horizon
/// still alight; now and then one does both and crosses the whole sky.
/// The ground hides what is under the horizon. It is in the sky, as the
/// sun is, so every player sees the one arc wherever they stand, and
/// every arc runs long enough in the sky to cross the low sky the camera
/// sees whichever way it faces. Day or night. A held calendar holds them
/// where they burn.
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
        let turn = if draw(6) < 0.5 { -FRAC_PI_2 } else { FRAC_PI_2 };
        let rises = direction(bearing, 0.0);
        let top = direction(bearing + turn, (20.0 + 30.0 * draw(3)).to_radians());
        let pole = rises.cross(top).normalize();
        // Where it lights and where it is out, in degrees round the
        // circle from where that rises: under 0 and past 180 are under
        // the horizon.
        let (near, far) = (draw(8), draw(2));
        let (lights, out) = match draw(7) {
            kind if kind < 0.3 => (-2.0 - 13.0 * near, 80.0 + 80.0 * far),
            kind if kind < 0.6 => (5.0 + 45.0 * near, 85.0 + 45.0 * near + 30.0 * far),
            kind if kind < 0.9 => (20.0 + 80.0 * near, 182.0 + 13.0 * far),
            _ => (-2.0 - 10.0 * near, 182.0 + 10.0 * far),
        };
        let from = (lights as f32).to_radians();
        found.push(Meteor {
            at,
            // Thirty to forty degrees a second, however far it runs.
            burns_ms: ((out - lights) / (30.0 + 10.0 * draw(5)) * 1_000.0) as u128,
            lights: rises * from.cos() + pole.cross(rises) * from.sin(),
            pole,
            span: ((out - lights) as f32).to_radians(),
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
    use crate::systems::SEASON_MS;

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
        assert!(!meteors(&omen_sky, omen, omen + 600_000).is_empty());
    }

    /// A low sun's fire is shut out by cloud toward the sun and by none
    /// behind; it is the moment's and the place's; and dust follows the
    /// seasons, most in Ash.
    #[test]
    fn the_air_is_the_far_sky_and_the_season() {
        let ms = MARCH + 2 * SEASON_MS + 3 * DAY_MS + 21 * HOUR_MS;
        let forecast = Forecast::at(ms);
        let (mut shut, mut open) = (0, 0);
        for k in 0..400 {
            let (x, z) = (k as f64 * 1_700.0, k as f64 * -900.0);
            let air = forecast.air(ms, x, z, (-1.0, 0.0));
            assert_eq!(air, forecast.air(ms, x, z, (-1.0, 0.0)));
            assert!([air.open, air.dust, air.damp, air.washed].iter().all(|share| (0.0..=1.0).contains(share)), "{air:?}");
            let far = forecast.over(x - SUNWARD_WU, z).cover.max(forecast.over(x - 2.0 * SUNWARD_WU, z).cover);
            if far > 0.9 { shut += 1; assert!(air.open < 0.05, "{air:?}"); }
            if far < 0.2 { open += 1; assert!(air.open > 0.95, "{air:?}"); }
        }
        assert!(shut > 0 && open > 0, "some shut, some open: {shut} {open}");
        let clear = Weather::default();
        let dust = |season: u128| Forecast::steady(MARCH + season * SEASON_MS + SEASON_MS / 2, clear, 0.0).air(0, 0.0, 0.0, (1.0, 0.0)).dust;
        assert!(dust(2) > dust(1) && dust(1) > dust(3) && dust(3) > dust(0), "Ash, Blaze, Freeze, Thaw");
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

    /// Every fireball runs on a great circle, most of its run in the sky
    /// and climbing high there, burning no longer than a reader looks
    /// back for. Some light under the horizon and some in the sky, some
    /// burn out in the sky and some go down past the horizon, in every
    /// pairing; they cross on every side.
    #[test]
    fn fireballs_arc_across_the_sky_on_every_side() {
        let omen = MARCH + 4 * SEASON_MS + DAY_MS;
        let seen = meteors(&Forecast::at(omen), omen, omen + 180 * 60_000);
        assert!(seen.len() > 50);
        let mut kinds = std::collections::HashSet::new();
        for m in &seen {
            assert!(m.pole.dot(m.lights).abs() < 1e-5, "on a great circle through where it lights");
            let run: Vec<f32> = (0..=40).map(|k| m.along(k as f32 / 40.0).y).collect();
            assert!(run.iter().filter(|&&y| y > 0.0).count() > run.len() / 2, "most of its run in the sky");
            assert!(run.iter().any(|&y| y > 0.3), "climbing high");
            assert!(m.burns_ms > 0 && m.burns_ms <= METEOR_BURNS_MOST_MS);
            kinds.insert((m.lights.y < 0.0, m.along(1.0).y < 0.0));
        }
        assert_eq!(kinds.len(), 4, "lit under the horizon or in the sky, out in the sky or past the horizon");
        for (x, z) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            assert!(seen.iter().any(|m| { let mid = m.along(0.5); mid.x * x + mid.z * z > 0.5 }), "fireballs cross toward ({x}, {z})");
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
