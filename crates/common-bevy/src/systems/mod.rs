pub mod combat;
pub mod movement;
pub mod physics;
pub mod targeting;
pub mod world;

// TODO: add "leap" season once per quarter
// TODO: shift day/night cycle by 12 minutes every day
pub const MINUTE_MS: u128 = HOUR_MS / 60;   // 10 secs  real time = 60-sec   min    game time
pub const HOUR_MS: u128 = DAY_MS / 24;      // 10 mins  real time = 60-min   hour   game time
pub const DAY_MS: u128 = 14_400_000;        //  4 hour  real time = 24-hour  day    game time
pub const WEEK_MS: u128 = DAY_MS*6;         //  1 day   real time = 6-day    week   game time
pub const SEASON_MS: u128 = WEEK_MS*7;      //  1 week  real time = 7-week   season game time
pub const YEAR_MS: u128 = SEASON_MS*4;      // ~1 month real time = 4-season year   game time, 

/// The seasons of the year and the weeks of a season, in order.
pub const SEASONS: [&str; 4] = ["Thaw", "Blaze", "Ash", "Freeze"];
pub const WEEKS: [&str; 7] = ["Mot", "Tus", "Wad", "Tur", "Fid", "Sut", "Sud"];

/// A day of the year by its season, week of the season and day of the
/// week, shown as `day.week.season`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Date {
    pub season: usize,
    pub week: usize,
    pub day: usize,
}

impl Date {
    /// The date at game time `ms`.
    pub fn of(ms: u128) -> Self {
        Self {
            season: (ms % YEAR_MS / SEASON_MS) as usize,
            week: (ms % SEASON_MS / WEEK_MS) as usize,
            day: (ms % WEEK_MS / DAY_MS) as usize,
        }
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.day, WEEKS[self.week], SEASONS[self.season])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_reads_day_week_and_season() {
        let ms = 2 * SEASON_MS + 3 * WEEK_MS + 4 * DAY_MS + 5 * HOUR_MS;
        assert_eq!(Date::of(ms), Date { season: 2, week: 3, day: 4 });
        assert_eq!(Date::of(ms).to_string(), "4.Tur.Ash");
        assert_eq!(Date::of(ms + YEAR_MS), Date::of(ms), "the year wraps");
    }
}
