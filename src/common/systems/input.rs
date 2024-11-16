use bevy::prelude::*;

use crate::{ *,
    common::{
        message::{*, Event},
        components::keybits::*
    },
};

pub fn generate_input(
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    query: Query<(Entity, &KeyBits), With<Actor>>,
) {
    for (ent, &key_bits) in query.iter() {
        let dt = (time.delta_seconds() * 1000.) as u16;
        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq: 0 } });
    }
}
