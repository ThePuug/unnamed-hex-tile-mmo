use bevy::prelude::*;
use common_bevy::systems::{DAY_MS, HOUR_MS, MINUTE_MS, SEASON_MS, WEEK_MS};

#[derive(Resource, Default)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    /// The weather the sky is held in, or the forecast.
    pub weather: WeatherHold,
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
    /// The ground wears the one canopy each summary's vertices carry
    /// instead of reading its parts, the two drawn in the same frame for a
    /// measurement of what the parts cost.
    pub canopy_parts_off: bool,
    /// The last summary level wears no wood: its parts unread, its tint
    /// and its relief off, the two drawn in the same frame for a
    /// measurement of what the horizon's wood costs.
    pub canopy_last_off: bool,
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

/// The weather the console holds the sky in, the same everywhere, so each
/// can be looked at without waiting for it; or the forecast. Storm and
/// Meteors hold the leap season's sky, its lightning and its fireballs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub enum WeatherHold {
    #[default]
    Forecast,
    Clear,
    Overcast,
    Rain,
    Snow,
    Mist,
    Storm,
    Meteors,
}

impl WeatherHold {
    const ORDER: [Self; 8] = [Self::Forecast, Self::Clear, Self::Overcast, Self::Rain, Self::Snow, Self::Mist, Self::Storm, Self::Meteors];

    /// The hold after this one, round again to the forecast.
    pub fn next(self) -> Self {
        let at = Self::ORDER.iter().position(|&hold| hold == self).expect("every hold is in the order");
        Self::ORDER[(at + 1) % Self::ORDER.len()]
    }
}

/// A field of the date the console picks for the arrows to step the
/// world's calendar by.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DateField {
    #[default]
    Day,
    Week,
    Season,
}

impl DateField {
    /// How long one of it is: every season is the same length, so a step
    /// keeps the place in it, and past the last of its span it carries
    /// into the next, a season through each year's leap season.
    pub fn unit(self) -> u128 {
        match self {
            Self::Day => DAY_MS,
            Self::Week => WEEK_MS,
            Self::Season => SEASON_MS,
        }
    }

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
    use common_bevy::systems::{Calendar, CalendarChange, Date};
    use common_bevy::Moment;

    #[test]
    fn an_hour_is_typed_as_hhmm_or_hh() {
        assert_eq!(parse_time("1830"), Some(18 * HOUR_MS + 30 * MINUTE_MS));
        assert_eq!(parse_time("7"), Some(7 * HOUR_MS));
        assert_eq!(parse_time("2460"), None);
        assert_eq!(parse_time("123"), None);
        assert_eq!(parse_time(""), None);
    }

    /// A stepped field carries into the span above it, a season through
    /// the leap season into the next year, and moves nothing finer.
    #[test]
    fn a_date_field_steps_on_through_the_year() {
        let midnight = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis() as u128;
        // March 2026 has five Mondays, the 2nd its first; April four
        let (march, april) = (midnight(2026, 3, 2), midnight(2026, 4, 6));
        let now = Moment::ZERO;
        let mut calendar = Calendar::running(0);
        calendar.change(CalendarChange::HoldAt(march + 9 * HOUR_MS), now, 0);
        let mut step = |field: DateField, steps: i64| calendar.change(CalendarChange::Shift(steps * field.unit() as i64), now, 0);
        step(DateField::Day, -1);
        step(DateField::Day, 1);
        step(DateField::Season, 4);
        assert_eq!(Date::of(calendar.at(now)), Date { season: 4, week: 0, day: 0 }, "March's leap season");
        let mut step = |field: DateField, steps: i64| calendar.change(CalendarChange::Shift(steps * field.unit() as i64), now, 0);
        step(DateField::Season, 1);
        assert_eq!(calendar.at(now), april + 9 * HOUR_MS, "on into April's year");
    }
}
