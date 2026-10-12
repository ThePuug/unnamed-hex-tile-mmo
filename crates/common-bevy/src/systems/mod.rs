pub mod combat;
pub mod movement;
pub mod physics;
pub mod targeting;
pub mod world;

use chrono::Datelike;
use serde::{Deserialize, Serialize};

use crate::Moment;

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

/// The world's calendar: the wall-clock moment the server's clock read 0
/// at, as an admin has moved it, and the moment it stands held at, if it
/// is held. The server keeps it and sends it to every client, and every
/// reading of the date, the sky and the weather goes through it, so every
/// player reads one moment and a held calendar holds all of it. The game
/// clock every timer runs on is the server's uptime and never moves with
/// it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub wall_at_zero: u128,
    pub held: Option<u128>,
}

/// An admin's change to the calendar, as `Calendar::change` takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarChange {
    /// Move it this many milliseconds on, or back, held or running.
    Shift(i64),
    /// Hold it at this moment.
    HoldAt(u128),
    /// Let it run on from where it is held.
    Run,
    /// Run it at the wall clock again.
    Reset,
}

/// The earliest moment the calendar is moved to, 1 January 1971: it reads
/// no moment whose week or year began before the epoch.
pub const EARLIEST: u128 = 365 * 86_400_000;

impl Calendar {
    /// Running at the wall clock, the server's clock having read 0 at
    /// `wall_at_zero`.
    pub fn running(wall_at_zero: u128) -> Self {
        Self { wall_at_zero, held: None }
    }

    /// The calendar's moment at game time `game`.
    pub fn at(&self, game: Moment) -> u128 {
        self.held.unwrap_or(self.wall_at_zero.saturating_add(game.since(Moment::ZERO).as_millis()))
    }

    /// Takes `change` at game time `game`, the wall clock reading
    /// `wall_at_zero` at the server's 0 for a reset. Never moves before
    /// `EARLIEST`.
    pub fn change(&mut self, change: CalendarChange, game: Moment, wall_at_zero: u128) {
        let elapsed = game.since(Moment::ZERO).as_millis();
        match change {
            CalendarChange::Shift(delta) => {
                let to = (self.at(game) as i128 + delta as i128).max(EARLIEST as i128) as u128;
                match self.held {
                    Some(_) => self.held = Some(to),
                    None => self.wall_at_zero = to.saturating_sub(elapsed),
                }
            }
            CalendarChange::HoldAt(moment) => self.held = Some(moment.max(EARLIEST)),
            CalendarChange::Run => {
                if let Some(held) = self.held.take() {
                    self.wall_at_zero = held.saturating_sub(elapsed);
                }
            }
            CalendarChange::Reset => *self = Self::running(wall_at_zero),
        }
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

    /// A running calendar keeps the wall clock, shifted where it was moved;
    /// a held one stands still; run again, it goes on from where it stood;
    /// reset, it keeps the wall clock; and nothing moves it before 1971.
    #[test]
    fn the_calendar_holds_runs_and_shifts() {
        let wall = monday(2026, 3, 2);
        let at = |ms: u64| Moment::from_millis(ms);
        let mut calendar = Calendar::running(wall);
        assert_eq!(calendar.at(at(5_000)), wall + 5_000);

        calendar.change(CalendarChange::Shift(DAY_MS as i64), at(5_000), wall);
        assert_eq!(calendar.at(at(6_000)), wall + DAY_MS + 6_000, "a shift runs on");

        calendar.change(CalendarChange::HoldAt(wall + HOUR_MS), at(6_000), wall);
        assert_eq!(calendar.at(at(60_000)), wall + HOUR_MS, "held, it stands still");
        calendar.change(CalendarChange::Shift(-(MINUTE_MS as i64)), at(60_000), wall);
        assert_eq!(calendar.at(at(90_000)), wall + HOUR_MS - MINUTE_MS, "a shift moves the held moment");

        calendar.change(CalendarChange::Run, at(90_000), wall);
        assert_eq!(calendar.at(at(91_000)), wall + HOUR_MS - MINUTE_MS + 1_000, "it runs on from where it stood");

        calendar.change(CalendarChange::Reset, at(91_000), wall);
        assert_eq!(calendar, Calendar::running(wall));

        calendar.change(CalendarChange::Shift(-(wall as i64)), at(0), wall);
        assert_eq!(calendar.at(at(0)), EARLIEST, "never before 1971");
        assert!(Date::year(calendar.at(at(0))).1 > 0, "the calendar reads it");
    }
}
