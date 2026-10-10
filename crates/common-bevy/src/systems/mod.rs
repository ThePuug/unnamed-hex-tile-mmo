pub mod combat;
pub mod movement;
pub mod physics;
pub mod targeting;
pub mod world;

use chrono::Datelike;

// Game time is the server's wall clock in milliseconds since the epoch,
// under game names: a day of four real hours, a week of six days (a real
// day, midnight to midnight), a season of seven weeks (a real week, Monday
// to Sunday), and a year from a month's first Monday to the next month's,
// four seasons or five (`design/calendar.md`). Every reading below takes
// the number alone, so the server and every client read one date.
pub const MINUTE_MS: u128 = HOUR_MS / 60;   // 10 secs  real time = 60-sec   min    game time
pub const HOUR_MS: u128 = DAY_MS / 24;      // 10 mins  real time = 60-min   hour   game time
pub const DAY_MS: u128 = 14_400_000;        //  4 hour  real time = 24-hour  day    game time
pub const WEEK_MS: u128 = DAY_MS*6;         //  1 day   real time = 6-day    week   game time
pub const SEASON_MS: u128 = WEEK_MS*7;      //  1 week  real time = 7-week   season game time

/// The seasons of the year in order, the fifth the leap season a month
/// with a fifth Monday ends on; and the weeks of a season, named for the
/// weekday each is.
pub const SEASONS: [&str; 5] = ["Thaw", "Blaze", "Ash", "Freeze", "Omen"];
pub const WEEKS: [&str; 7] = ["Mot", "Tus", "Wen", "Tur", "Fid", "Sar", "Sud"];

/// A day of the year by its season, week of the season and day of the
/// week, shown as `day.week.season`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Date {
    pub season: usize,
    pub week: usize,
    pub day: usize,
}

impl Date {
    /// The date at game time `ms`: the season is which Monday of its month
    /// the week's is, so a fifth Monday's week is the leap season.
    pub fn of(ms: u128) -> Self {
        let at = wall(ms);
        let week = at.weekday().num_days_from_monday() as usize;
        let monday = at.date() - chrono::Days::new(week as u64);
        Self { season: (monday.day0() / 7) as usize, week, day: (ms % WEEK_MS / DAY_MS) as usize }
    }

    /// The game time the season holding `ms` began at: its Monday's midnight.
    pub fn season_start(ms: u128) -> u128 {
        let at = wall(ms);
        game_ms(at.date() - chrono::Days::new(at.weekday().num_days_from_monday() as u64))
    }

    /// The year holding `ms`: the game time its first Monday began at, and
    /// its length, four seasons or five.
    pub fn year(ms: u128) -> (u128, u128) {
        let monday = wall(Self::season_start(ms)).date();
        let start = first_monday(monday.year(), monday.month());
        let next = monday.checked_add_months(chrono::Months::new(1)).expect("a month on");
        let end = first_monday(next.year(), next.month());
        (game_ms(start), game_ms(end) - game_ms(start))
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.day, WEEKS[self.week], SEASONS[self.season])
    }
}

/// The calendar's zone: Eastern European Time, UTC+2 without summer time,
/// whatever zone the server runs in, so every server keeps one calendar
/// and no clock jumps an hour at a summer-time change.
pub const CALENDAR_OFFSET_SECS: i32 = 2 * 3600;

/// The wall clock now, in the calendar's zone, as game time's anchor.
pub fn wall_now() -> u128 {
    let zone = chrono::FixedOffset::east_opt(CALENDAR_OFFSET_SECS).expect("a zone");
    chrono::Utc::now().with_timezone(&zone).naive_local().and_utc().timestamp_millis() as u128
}

/// The wall-clock moment game time `ms` is.
fn wall(ms: u128) -> chrono::NaiveDateTime {
    chrono::DateTime::from_timestamp_millis(ms as i64).expect("game time is a wall-clock moment").naive_utc()
}

/// The game time midnight of `date` is.
fn game_ms(date: chrono::NaiveDate) -> u128 {
    date.and_hms_opt(0, 0, 0).expect("midnight").and_utc().timestamp_millis() as u128
}

/// The first Monday of a month.
fn first_monday(year: i32, month: u32) -> chrono::NaiveDate {
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("a month");
    first + chrono::Days::new((7 - first.weekday().num_days_from_monday() as u64) % 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monday(year: i32, month: u32, day: u32) -> u128 {
        game_ms(chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap())
    }

    /// March 2026 has five Mondays, the 2nd its first and the 30th its
    /// fifth; its Sunday the 1st belongs to February's year.
    #[test]
    fn a_date_reads_day_week_and_season() {
        let march = monday(2026, 3, 2);
        let ms = march + 3 * WEEK_MS + 4 * DAY_MS + 5 * HOUR_MS;
        assert_eq!(Date::of(ms), Date { season: 0, week: 3, day: 4 });
        assert_eq!(Date::of(ms).to_string(), "4.Tur.Thaw");
        assert_eq!(Date::of(ms + 4 * SEASON_MS).to_string(), "4.Tur.Omen", "the fifth Monday's week is the leap season");
        assert_eq!(Date::of(ms + 5 * SEASON_MS), Date::of(ms), "April's first Monday begins the year again");
        assert_eq!(Date::of(march - 1), Date { season: 3, week: 6, day: 5 }, "the Sunday before is February's last");
    }

    #[test]
    fn a_year_runs_from_one_first_monday_to_the_next() {
        let (march, april, may) = (monday(2026, 3, 2), monday(2026, 4, 6), monday(2026, 5, 4));
        assert_eq!(Date::year(march + 2 * SEASON_MS + DAY_MS), (march, 5 * SEASON_MS), "five Mondays, five seasons");
        assert_eq!(Date::year(april + HOUR_MS), (april, 4 * SEASON_MS));
        assert_eq!(april - march, 5 * SEASON_MS);
        assert_eq!(may - april, 4 * SEASON_MS);
        assert_eq!(Date::season_start(march + 3 * WEEK_MS + HOUR_MS), march);
    }
}
