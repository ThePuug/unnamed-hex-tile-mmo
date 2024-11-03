use bevy::prelude::*;

use crate::{ *,
    common::{
        message::{*, Event},
        components::keybits::*
    },
};

pub fn generate_input(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    query: Query<(Entity, Option<&AirTime>, &KeyBits), With<Actor>>,
) {
    for (ent, air_time, &key_bits) in query.iter() {
        let dt = (time.delta_seconds() * 1000.) as u16;
        if key_bits.all_pressed([KB_JUMP]) && air_time.is_none() {
            commands.entity(ent).insert(AirTime(500));
        }
        writer.send(Try { event: Event::Input { ent, key_bits, dt } });
    }
}
