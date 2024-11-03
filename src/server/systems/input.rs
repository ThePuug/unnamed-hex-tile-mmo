use bevy::prelude::*;

use crate::common::message::{*, Event};

pub fn try_input(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits, dt } } => {
                writer.send(Do { event: Event::Input { ent, key_bits, dt } });
            }
            _ => {}
        }
    }
}
