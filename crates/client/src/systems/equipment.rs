use bevy::prelude::*;

use common_bevy::{
    components::equipment::Inventory,
    message::{Event, *},
};

/// Keeps the local player's bag as the server sends it.
pub fn do_inventory(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
) {
    for message in reader.read() {
        let Do { event: Event::Inventory { ent, items } } = message else { continue };
        if let Ok(mut entity) = commands.get_entity(*ent) {
            entity.insert(Inventory { items: items.clone() });
        }
    }
}
