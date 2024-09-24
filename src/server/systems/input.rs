use bevy::prelude::*;
use keybits::KB_JUMP;

use crate::common::{
    components::{ *,
        keybits::*,
        hx::*,
    },
    message::{*, Event},
};

pub fn try_input(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut query: Query<(&mut Heading, &mut Offset, &mut KeyBits, Option<&mut AirTime>)>
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits } } => {
                let (mut heading0, mut offset0, mut key_bits0, air_time0) = query.get_mut(ent).unwrap();
                let key_bits = *key_bits0;
                let heading = Heading(
                    if key_bits & (KB_HEADING_Q | KB_HEADING_R | KB_HEADING_NEG) { Hx { q: 1, r: -1, z: 0 } }
                    else if key_bits & (KB_HEADING_Q | KB_HEADING_R) { Hx { q: -1, r: 1, z: 0 } }
                    else if key_bits & (KB_HEADING_Q | KB_HEADING_NEG) { Hx { q: -1, r: 0, z: 0 } }
                    else if key_bits & (KB_HEADING_R | KB_HEADING_NEG) { Hx { q: 0, r: -1, z: 0 } }
                    else if key_bits & KB_HEADING_Q { Hx { q: 1, r: 0, z: 0 } }
                    else if key_bits & KB_HEADING_R { Hx { q: 0, r: 1, z: 0 } }
                    else { Hx::default() });
                if *heading0 != heading { *heading0 = heading; }
                if key_bits & KB_JUMP {
                    if air_time0.is_none() {
                        commands.entity(ent).insert(AirTime(0.5));
                        offset0.0 += Vec3{ x: 0., y: 0., z: 1.2 };
                    }
                }
                if *key_bits0 != key_bits { *key_bits0 = key_bits; }
            }
            _ => {}
        }
    }
}