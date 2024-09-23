use bevy::prelude::*;

use crate::common::{
    components::*,
    message::{*, Event},
};

pub fn try_input(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut query: Query<(&mut Offset, Option<&mut AirTime>)>
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits } } => {
                if key_bits.0 & (1 << 0) != 0 {
                    let (mut offset0, air_time) = query.get_mut(ent).unwrap();
                    if air_time.is_none() {
                        commands.entity(ent).insert(AirTime(0.5));
                        offset0.0 += Vec3{ x: 0., y: 0., z: 1.2 };
                    }
                }
            }
            _ => {}
        }
    }
}