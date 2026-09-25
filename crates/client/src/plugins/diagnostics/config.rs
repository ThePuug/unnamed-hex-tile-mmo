use bevy::prelude::*;
use common_bevy::systems::{DAY_MS, HOUR_MS, MINUTE_MS, SEASON_MS, WEEK_MS, YEAR_MS};

/// Which set of numbers the overlay shows. The frame's own line stands
/// above them whatever is picked; the rest come one at a time, chosen
/// from the strip under the view, because together they outgrew the
/// window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MetricsTab {
    Terrain,
    #[default]
    Render,
    Passes,
    Network,
    Timings,
}

impl MetricsTab {
    /// Every tab, in the order the strip lays them out.
    pub const ALL: [MetricsTab; 5] = [Self::Terrain, Self::Render, Self::Passes, Self::Network, Self::Timings];

    pub fn label(self) -> &'static str {
        match self {
            Self::Terrain => "TERRAIN",
            Self::Render => "RENDER",
            Self::Passes => "PASSES",
            Self::Network => "NETWORK",
            Self::Timings => "TIMINGS",
        }
    }

    /// The tab `steps` along the strip, wrapping at either end.
    pub fn step(self, steps: i32) -> Self {
        let at = Self::ALL.iter().position(|&tab| tab == self).unwrap_or(0) as i32;
        Self::ALL[(at + steps).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Resource)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    pub lighting: LightingClock,
    pub metrics_overlay_visible: bool,
    /// Which set of numbers the overlay's panel shows.
    pub metrics_tab: MetricsTab,
    /// Every terrain mesh is hidden.
    pub terrain_hidden: bool,
    /// Every stand of models and cards is hidden. The batches hang under
    /// the region meshes, so hiding terrain hides these too; this hides
    /// them alone, leaving the ground to be measured by itself.
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

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            lighting: LightingClock::default(),
            metrics_overlay_visible: false,
            metrics_tab: MetricsTab::default(),
            terrain_hidden: false,
            cover_hidden: false,
            camera_closeup: false,
            camera_envelope_off: false,
            canopy_parts_off: false,
        }
    }
}

/// The clock the sun and moon keep: game time, or an hour the console
/// holds it at, so the sky can be looked at by the hour while threats
/// keep game time.
#[derive(Clone, Copy, Debug)]
pub struct LightingClock {
    /// Lighting time held, or none to read game time.
    held: Option<u128>,
}

impl Default for LightingClock {
    /// Held at nine in the morning.
    fn default() -> Self {
        Self { held: Some(9 * HOUR_MS) }
    }
}

impl LightingClock {
    /// Lighting time at game time `game`.
    pub fn at(&self, game: u128) -> u128 {
        self.held.unwrap_or(game)
    }

    /// The hour of the day the clock is held at, as `HH:MM`, if held.
    pub fn held_at(&self) -> Option<String> {
        let day = self.held? % DAY_MS;
        Some(format!("{:02}:{:02}", day / HOUR_MS, day % HOUR_MS / MINUTE_MS))
    }

    /// Holds the clock at `ms_of_day` on the day it reads at game time
    /// `game`, so the season stays.
    pub fn hold(&mut self, game: u128, ms_of_day: u128) {
        let now = self.at(game);
        self.held = Some(now - now % DAY_MS + ms_of_day % DAY_MS);
    }

    /// Holds the clock at `ms` into the year, the date with the hour.
    pub fn hold_at(&mut self, ms: u128) {
        self.held = Some(ms % YEAR_MS);
    }

    /// Reads game time again.
    pub fn sync(&mut self) {
        self.held = None;
    }

    /// Moves the clock `delta` ms either way within the year, holding it
    /// first where it read game time `game` if it was not held.
    pub fn scrub(&mut self, game: u128, delta: i128) {
        let now = self.at(game) as i128;
        self.held = Some((now + delta).rem_euclid(YEAR_MS as i128) as u128);
    }

    /// Moves the clock `steps` of `field` on, or back, wrapping within the
    /// span above it — a day within its week, a week within its season, a
    /// season within the year — so nothing coarser or finer moves. Holds the
    /// clock first where it read game time `game` if it was not held.
    pub fn step(&mut self, game: u128, field: DateField, steps: i32) {
        let (unit, span) = field.unit_span();
        let now = self.at(game);
        let count = (span / unit) as i128;
        let index = ((now % span / unit) as i128 + steps as i128).rem_euclid(count) as u128;
        self.held = Some(now - now % span + index * unit + now % unit);
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

    /// The field's length and the span it counts within, in ms.
    fn unit_span(self) -> (u128, u128) {
        match self {
            Self::Day => (DAY_MS, WEEK_MS),
            Self::Week => (WEEK_MS, SEASON_MS),
            Self::Season => (SEASON_MS, YEAR_MS),
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

        let game = 3 * DAY_MS + 5 * HOUR_MS;
        clock.hold(game, LightingClock::parse_time("1830").unwrap());
        assert_eq!(clock.held_at().as_deref(), Some("18:30"));
        assert_eq!(clock.at(game) / DAY_MS, 0, "the held day is the clock's, not game time's");

        clock.sync();
        assert_eq!(clock.held_at(), None);
        assert_eq!(clock.at(game), game);
        clock.hold(game, LightingClock::parse_time("7").unwrap());
        assert_eq!(clock.at(game), 3 * DAY_MS + 7 * HOUR_MS);

        clock.sync();
        clock.scrub(game, -(HOUR_MS as i128));
        assert_eq!(clock.at(game), game - HOUR_MS, "a scrub from game time holds an hour behind it");
        clock.scrub(game, -(game as i128) - 1);
        assert_eq!(clock.at(game), YEAR_MS - HOUR_MS - 1, "rewinding past the start wraps to the year's end");

        assert_eq!(LightingClock::parse_time("2460"), None);
        assert_eq!(LightingClock::parse_time("123"), None);
        assert_eq!(LightingClock::parse_time(""), None);
    }

    /// A stepped field wraps within the span above it and moves nothing
    /// else; from game time, the step holds the clock where game time was.
    #[test]
    fn a_date_field_steps_within_its_span() {
        use common_bevy::systems::Date;

        let mut clock = LightingClock::default();
        clock.step(0, DateField::Day, -1);
        assert_eq!(Date::of(clock.at(0)), Date { season: 0, week: 0, day: 5 });
        assert_eq!(clock.held_at().as_deref(), Some("09:00"));
        clock.step(0, DateField::Week, 8);
        assert_eq!(Date::of(clock.at(0)), Date { season: 0, week: 1, day: 5 });
        clock.step(0, DateField::Season, -1);
        assert_eq!(Date::of(clock.at(0)), Date { season: 3, week: 1, day: 5 });

        clock.sync();
        let game = 2 * SEASON_MS + 3 * WEEK_MS + 4 * DAY_MS + 5 * HOUR_MS;
        clock.step(game, DateField::Day, 1);
        assert_eq!(clock.at(game), game + DAY_MS);
        assert_eq!(clock.at(0), game + DAY_MS);
    }
}
