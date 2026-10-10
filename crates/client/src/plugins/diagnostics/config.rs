use bevy::prelude::*;
use common_bevy::systems::{Date, DAY_MS, HOUR_MS, MINUTE_MS, SEASON_MS, WEEK_MS};

#[derive(Resource, Default)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    pub lighting: LightingClock,
    /// Every terrain mesh is hidden.
    pub terrain_hidden: bool,
    /// Every stand of models and cards is hidden. A stand is an entity of
    /// its own, gathering every region's instances, so hiding the terrain
    /// leaves it drawn; this hides the cover alone, leaving the ground to
    /// be measured by itself.
    pub cover_hidden: bool,
    /// The camera holds its lowest pose — the boom at its shortest, looking
    /// up — instead of following the ground: an actor seen close.
    pub camera_closeup: bool,
    /// The camera goes straight to its wanted pose, showing whatever the
    /// envelope would have hidden.
    pub camera_envelope_off: bool,
    /// The ground wears the one canopy each summary's vertices carry
    /// instead of reading its parts, the two drawn in the same frame for a
    /// measurement of what the parts cost.
    pub canopy_parts_off: bool,
}

/// The earliest moment the clock is scrubbed to, 1 January 1971: the
/// calendar reads no moment whose week or year began before the epoch.
const EARLIEST: u128 = 365 * 86_400_000;

/// The clock the sun and moon keep, in wall-clock time (`Server::wall`):
/// the wall clock itself, or a moment the console holds it at, so the sky
/// can be looked at by the hour and the date while threats keep game time.
#[derive(Clone, Copy, Debug)]
pub struct LightingClock {
    /// Lighting time held, or none to read the wall clock.
    held: Option<u128>,
}

impl Default for LightingClock {
    /// Held at nine in the morning of the day the client starts on. Never
    /// a day of 1970: the calendar reads no moment whose season began
    /// before the epoch.
    fn default() -> Self {
        let now = common_bevy::systems::wall_now();
        Self { held: Some(now - now % DAY_MS + 9 * HOUR_MS) }
    }
}

impl LightingClock {
    /// Lighting time at wall-clock time `wall`.
    pub fn at(&self, wall: u128) -> u128 {
        self.held.unwrap_or(wall)
    }

    /// The hour of the day the clock is held at, as `HH:MM`, if held.
    pub fn held_at(&self) -> Option<String> {
        let day = self.held? % DAY_MS;
        Some(format!("{:02}:{:02}", day / HOUR_MS, day % HOUR_MS / MINUTE_MS))
    }

    /// Holds the clock at `ms_of_day` on the day it reads at wall time
    /// `wall`, so the season stays.
    pub fn hold(&mut self, wall: u128, ms_of_day: u128) {
        let now = self.at(wall);
        self.held = Some(now - now % DAY_MS + ms_of_day % DAY_MS);
    }

    /// Holds the clock at game time `ms`, the date with the hour.
    pub fn hold_at(&mut self, ms: u128) {
        self.held = Some(ms);
    }

    /// Reads game time again.
    pub fn sync(&mut self) {
        self.held = None;
    }

    /// Moves the clock `delta` ms either way, never before `EARLIEST`,
    /// holding it first where it read game time `game` if it was not held.
    pub fn scrub(&mut self, wall: u128, delta: i128) {
        let now = self.at(wall) as i128;
        self.held = Some((now + delta).max(EARLIEST as i128) as u128);
    }

    /// Moves the clock `steps` of `field` on, or back, wrapping within the
    /// span above it — a day within its week, a week within its season, a
    /// season within its year of four or five — so nothing coarser or finer
    /// moves. Holds the clock first where it read game time `game` if it
    /// was not held.
    pub fn step(&mut self, wall: u128, field: DateField, steps: i32) {
        let now = self.at(wall);
        let (unit, start, span) = match field {
            DateField::Day => (DAY_MS, now - now % WEEK_MS, WEEK_MS),
            DateField::Week => (WEEK_MS, Date::season_start(now), SEASON_MS),
            DateField::Season => {
                let (start, span) = Date::year(now);
                (SEASON_MS, start, span)
            }
        };
        let count = (span / unit) as i128;
        let index = (((now - start) / unit) as i128 + steps as i128).rem_euclid(count) as u128;
        self.held = Some(start + index * unit + (now - start) % unit);
    }

    /// An hour of the day typed as `HHMM` or `HH`, in ms of the day.
    pub fn parse_time(text: &str) -> Option<u128> {
        let digits: Vec<u128> = text.chars().map(|c| c.to_digit(10).map(u128::from)).collect::<Option<_>>()?;
        let (hours, minutes) = match digits[..] {
            [h] => (h, 0),
            [h1, h2] => (h1 * 10 + h2, 0),
            [h1, h2, m1, m2] => (h1 * 10 + h2, m1 * 10 + m2),
            _ => return None,
        };
        (hours < 24 && minutes < 60).then(|| hours * HOUR_MS + minutes * MINUTE_MS)
    }
}

/// A field of the date the console picks for the arrows to step.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DateField {
    #[default]
    Day,
    Week,
    Season,
}

impl DateField {
    /// The field after this one, finer to coarser and round again.
    pub fn next(self) -> Self {
        match self {
            Self::Day => Self::Week,
            Self::Week => Self::Season,
            Self::Season => Self::Day,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Held, the clock stands at the hour typed on the day it was on;
    /// synced, it reads game time.
    #[test]
    fn the_lighting_clock_holds_an_hour_or_reads_game_time() {
        let mut clock = LightingClock::default();
        assert_eq!(clock.held_at().as_deref(), Some("09:00"));
        assert_eq!(clock.at(1_000), clock.at(500_000));
        let held_day = clock.at(0) / DAY_MS;

        let wall = EARLIEST + 3 * DAY_MS + 5 * HOUR_MS;
        clock.hold(wall, LightingClock::parse_time("1830").unwrap());
        assert_eq!(clock.held_at().as_deref(), Some("18:30"));
        assert_eq!(clock.at(wall) / DAY_MS, held_day, "the held day is the clock's, not wall time's");

        clock.sync();
        assert_eq!(clock.held_at(), None);
        assert_eq!(clock.at(wall), wall);
        clock.hold(wall, LightingClock::parse_time("7").unwrap());
        assert_eq!(clock.at(wall), EARLIEST + 3 * DAY_MS + 7 * HOUR_MS);

        clock.sync();
        clock.scrub(wall, -(HOUR_MS as i128));
        assert_eq!(clock.at(wall), wall - HOUR_MS, "a scrub from wall time holds an hour behind it");
        clock.scrub(wall, -(wall as i128) - 1);
        assert_eq!(clock.at(wall), EARLIEST, "rewinding stops at 1971");
        assert!(Date::year(clock.at(wall)).1 > 0, "the calendar reads it");

        assert_eq!(LightingClock::parse_time("2460"), None);
        assert_eq!(LightingClock::parse_time("123"), None);
        assert_eq!(LightingClock::parse_time(""), None);
    }

    /// A stepped field wraps within the span above it and moves nothing
    /// else — a season within its year of four or five — and from game
    /// time the step holds the clock where game time was.
    #[test]
    fn a_date_field_steps_within_its_span() {
        let midnight = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis() as u128;
        // March 2026 has five Mondays, the 2nd its first; April four
        let (march, april) = (midnight(2026, 3, 2), midnight(2026, 4, 6));

        let mut clock = LightingClock::default();
        clock.hold_at(march + 9 * HOUR_MS);
        clock.step(0, DateField::Day, -1);
        assert_eq!(Date::of(clock.at(0)), Date { season: 0, week: 0, day: 5 });
        assert_eq!(clock.held_at().as_deref(), Some("09:00"));
        clock.step(0, DateField::Week, 8);
        assert_eq!(Date::of(clock.at(0)), Date { season: 0, week: 1, day: 5 });
        clock.step(0, DateField::Season, -1);
        assert_eq!(Date::of(clock.at(0)), Date { season: 4, week: 1, day: 5 }, "March wraps to its leap season");
        clock.step(0, DateField::Season, 1);
        assert_eq!(Date::of(clock.at(0)), Date { season: 0, week: 1, day: 5 });

        clock.hold_at(april + 9 * HOUR_MS);
        clock.step(0, DateField::Season, -1);
        assert_eq!(Date::of(clock.at(0)), Date { season: 3, week: 0, day: 0 }, "April has four");

        clock.sync();
        let game = march + 2 * SEASON_MS + 3 * WEEK_MS + 4 * DAY_MS + 5 * HOUR_MS;
        clock.step(game, DateField::Day, 1);
        assert_eq!(clock.at(game), game + DAY_MS);
        assert_eq!(clock.at(0), game + DAY_MS);
    }
}
