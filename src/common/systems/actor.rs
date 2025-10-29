use bevy::prelude::*;

use crate::common::{
    components::{heading::Heading, keybits::*, behaviour::*},
    message::{Component, Event, *}
};

pub fn update(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &KeyBits, &Behaviour, &mut Heading), Changed<KeyBits>>,
) {
    for (ent, &key_bits, behaviour, mut heading0) in &mut query {
        // Only update heading for player-controlled entities
        if *behaviour != Behaviour::Controlled {
            continue;
        }

        let heading = if key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { Heading::from(key_bits) } else { *heading0 };
        if *heading0 != heading {
            *heading0 = heading;
            let component = Component::Heading(heading);
            writer.write(Try { event: Event::Incremental { ent, component } });
        }
    }
}
