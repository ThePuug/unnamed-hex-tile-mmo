pub mod actor;
pub mod behaviour;
pub mod combat_state;
pub mod gcd;
pub mod physics;
pub mod resources;
pub mod world;

// TODO: add "leap" season once per quarter
// TODO: shift day/night cycle by 12 minutes every day
pub const MINUTE_MS: u128 = HOUR_MS / 60;   // 10 secs  real time = 60-sec   min    game time
pub const HOUR_MS: u128 = DAY_MS / 24;      // 10 mins  real time = 60-min   hour   game time
pub const DAY_MS: u128 = 14_400_000;        //  4 hour  real time = 24-hour  day    game time
pub const WEEK_MS: u128 = DAY_MS*6;         //  1 day   real time = 6-day    week   game time
pub const SEASON_MS: u128 = WEEK_MS*7;      //  1 week  real time = 7-week   season game time
pub const YEAR_MS: u128 = SEASON_MS*4;      // ~1 month real time = 4-season year   game time, 
