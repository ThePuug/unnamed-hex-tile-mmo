use bevy::prelude::*;
use common_bevy::systems::{DAY_MS, HOUR_MS, MINUTE_MS, YEAR_MS};

#[derive(Resource)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    pub lighting: LightingClock,
    pub metrics_overlay_visible: bool,
    /// Every camera renders without MSAA.
    pub msaa_off: bool,
    /// The sun's shadows are filtered by the hardware's 2×2 tap instead of
    /// the Gaussian.
    pub hard_shadows: bool,
    /// The camera goes straight to its wanted pose, showing whatever the
    /// envelope would have hidden.
    pub camera_envelope_off: bool,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            lighting: LightingClock::default(),
            metrics_overlay_visible: false,
            msaa_off: false,
            hard_shadows: false,
            camera_envelope_off: false,
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
}
