use bevy::prelude::*;

use crate::{
    components::heading::*,
    message::{Component, Event, *},
};

pub fn try_incremental(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
) {
    for message in reader.read() {
        if let Try { event: Event::Incremental { ent, component } } = message {
            writer.write(Do { event: Event::Incremental { ent: *ent, component: component.clone() }});
        }
    }
}

/// Apply incremental component updates. Updates an existing component or
/// inserts it (late-binding for NPCs), except `Loc`, which the server set
/// before it sent the update and which the client re-anchors in its own
/// movement system because the position and the visual depend on it.
pub fn do_incremental(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut headings: Query<&mut Heading>,
) {
    for message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue };
        let ent = *ent;
        match component.clone() {
            Component::Loc(_) => {}
            Component::Heading(heading) => {
                if let Ok(mut heading0) = headings.get_mut(ent) {
                    if *heading0 != heading {
                        *heading0 = heading;
                    }
                }
            }
            other => {
                if let Ok(mut e) = commands.get_entity(ent) {
                    other.insert_into(&mut e);
                }
            }
        }
    }
}
