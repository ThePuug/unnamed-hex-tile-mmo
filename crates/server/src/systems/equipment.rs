use bevy::prelude::*;

use common_bevy::{
    components::equipment::{Equipment, Inventory},
    message::{Component, Event, *},
};

/// Wears or takes off the item a client asked about, when the item is in
/// that player's bag, and tells every client that sees the actor.
pub fn try_wear(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut query: Query<(&Inventory, &mut Equipment)>,
) {
    for message in reader.read() {
        let Try { event: Event::Wear { ent, item, on } } = message else { continue };
        let (ent, item, on) = (*ent, *item, *on);
        let Ok((bag, mut equipment)) = query.get_mut(ent) else { continue };
        if !bag.contains(item) {
            continue;
        }
        let changed = if on {
            equipment.wear(item) != Some(item)
        } else {
            equipment.take_off(item)
        };
        if changed {
            writer.write(Do { event: Event::Incremental { ent, component: Component::Equipment(*equipment) }});
        }
    }
}
