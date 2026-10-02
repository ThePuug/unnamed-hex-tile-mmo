//! What every fighter needs beside the fight, which its installer
//! schedules: its `Loc` following its `Position`, and the dead removed.

use bevy::prelude::*;
use common_bevy::{
    components::{position::Position, resources::RespawnTimer, Loc},
    message::{Component, Do, Event, Try},
    resources::map::Map,
};

/// Moves each actor's `Loc` onto the tile its `Position` has reached.
pub fn update(
    mut writer: MessageWriter<Try>,
    mut query: Query<(Entity, &mut Loc, &mut Position), Changed<Position>>,
    map: Res<Map>,
) {
    for (ent, mut loc0, mut position) in &mut query {
        let qrz = position.reached(&map);
        if **loc0 != qrz {
            position.rebase(qrz, &map);
            **loc0 = qrz;

            // Send Loc update to client
            writer.write(Try { event: Event::Incremental { ent, component: Component::Loc(Loc::new(qrz)) } });
        }
    }
}

/// Despawns each entity a `Despawn` names, but a dead player waiting to
/// respawn. The live server runs it after the network send, so the
/// message goes out before the entity is gone.
pub fn cleanup_despawned(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    respawn_query: Query<&RespawnTimer>,
) {
    for message in reader.read() {
        if let Do { event: Event::Despawn { ent } } = message {
            let ent = *ent;
            if respawn_query.get(ent).is_ok() {
                continue;
            }
            commands.entity(ent).despawn();
        }
    }
}
