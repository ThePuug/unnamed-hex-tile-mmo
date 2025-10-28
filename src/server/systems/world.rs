use bevy::prelude::*;
use chrono::{
    offset::Local, Datelike, Timelike
};
use qrz::*;

use crate::{
    common::{
        components::{ *, 
            entity_type::*,
            heading::Heading, 
            keybits::KeyBits, 
            offset::Offset, 
        }, 
        message::{ Event, * },
        systems::*
    }, 
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

pub fn try_spawn(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    query: Query<(&Loc, &EntityType, Option<&ActorAttributes>)>,
) {
    for message in reader.read() {
        let Try { event: Event::Spawn { ent, .. }} = message else { continue };
        let Ok((loc, typ, attrs)) = query.get(*ent) else { continue; };
        writer.write(Do { event: Event::Spawn { ent: *ent, typ: *typ, qrz: **loc, attrs: attrs.copied() }});
    }
}

pub fn do_spawn(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    mut map: ResMut<crate::Map>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Spawn { qrz, typ, ent, .. } } = message {
            match typ {
                EntityType::Decorator(_) => {
                    if map.get(qrz).is_none() { map.insert(qrz, typ) }
                },
                EntityType::Actor(_) => {
                    commands.entity(ent).insert((
                        Actor,
                        AirTime { state: Some(125), step: None },
                        KeyBits::default(),
                        Heading::default(),
                        Offset::default(),
                        Transform {
                            translation: map.convert(qrz),
                            ..default()},
                    ));
                },
                _ => {}
            }
        }
    }
}
