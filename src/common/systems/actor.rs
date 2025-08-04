use bevy::prelude::*;

use crate::common::{
    components::{heading::Heading, keybits::*}, 
    message::{Component, Event, *}
};

pub fn update(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &KeyBits, &mut Heading), Changed<KeyBits>>,
) {
    for (ent, &key_bits, mut heading0) in &mut query {
        let heading = if key_bits.any_pressed([KB_HEADING_Q, KB_HEADING_R]) { Heading::from(key_bits) } else { *heading0 };
        if *heading0 != heading { 
            *heading0 = heading;
            let component = Component::Heading(heading);
            writer.write(Try { event: Event::Incremental { ent, component } });
        }
    }
}
