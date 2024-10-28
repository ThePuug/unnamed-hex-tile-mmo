use bevy::prelude::*;
// use bevy_easings::*;
use keybits::KB_JUMP;

use crate::common::{
    components::{ *,
        keybits::*,
    },
    message::{*, Event},
};

pub fn try_input(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut query: Query<(&mut KeyBits, Option<&AirTime>)>
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits, dt } } => {
                let (mut key_bits0, air_time0) = query.get_mut(ent).unwrap();
                if key_bits.all_pressed([KB_JUMP]) && air_time0.is_none() {
                    commands.entity(ent).insert(AirTime(500));
                }
                if *key_bits0 != key_bits { *key_bits0 = key_bits; }
                writer.send(Do { event: Event::Input { ent, key_bits, dt } });
            }
            _ => {}
        }
    }
}