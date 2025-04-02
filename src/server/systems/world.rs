use bevy::prelude::*;
use chrono::{
    offset::Local, Datelike, Timelike
};

use crate::{
    common::systems::*, 
    server::resources::*
};

pub fn setup(
    mut runtime: ResMut<RunTime>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed().as_millis();
    let secs_since_midnight = Local::now().time().num_seconds_from_midnight();
    let days_since_monday = Local::now().weekday().number_from_monday() - 1;
    let weeks_since_year = Local::now().iso_week().week();
    debug!("weeks_since_year: {weeks_since_year},  days_since_monday: {days_since_monday}, secs_since_midnight: {secs_since_midnight}");
    runtime.elapsed_offset = weeks_since_year as u128 * SEASON_MS
        + days_since_monday as u128 * WEEK_MS 
        + secs_since_midnight as u128 * 1000 
        - elapsed;
}