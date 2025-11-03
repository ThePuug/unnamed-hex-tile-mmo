use bevy::prelude::*;

use crate::{
    common::{
        components::{ heading::*, * },
        message::{Event, *},
        systems::combat::gcd::*
    },
    *
};

pub fn try_input(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    respawn_query: Query<&crate::common::components::resources::RespawnTimer>,
) {
    for &message in reader.read() {
        let Try { event } = message;
        let Event::Input { ent, .. } = event else { continue };

        // Ignore input from dead players (those with RespawnTimer)
        if respawn_query.get(ent).is_ok() {
            continue;
        }

        writer.write(Do { event });
    }
}

pub fn send_input(
    lobby: Res<Lobby>,
    mut conn: ResMut<RenetServer>,
    mut buffers: ResMut<InputQueues>,
) {
    let entities_to_send: Vec<Entity> = buffers.entities().copied().collect();

    for ent in entities_to_send {
        let Some(buffer) = buffers.get_mut(&ent) else { continue };

        // Queue invariant: all queues must have at least 1 input
        assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");

        while buffer.queue.len() > 1 {
            let event = buffer.queue.pop_back().unwrap();
            let message = bincode::serde::encode_to_vec(
                Do { event },
                bincode::config::legacy()).unwrap();
            conn.send_message(*lobby.get_by_right(&ent).unwrap(), DefaultChannel::ReliableOrdered, message);
        }

        // Queue invariant maintained: exactly 1 input remaining (the accumulating one)
        assert_eq!(buffer.queue.len(), 1, "Queue must have exactly 1 input after sending confirmations");
    }
}

/// try_gcd is now vestigial - Event::Gcd only contains GcdType::Attack
/// which is handled by the ability systems, not here.
///
/// This function exists to satisfy the event pipeline but does nothing.
/// Event::Gcd { typ: GcdType::Attack } is sent but not processed here.
pub fn try_gcd(
    mut _reader: EventReader<Try>,
) {
    // GcdType::Attack is handled by ability systems (auto_attack, lunge, etc.)
    // PlaceSpawner and Spawn were removed - spawners are placed during terrain generation

    // This system could be removed entirely if Event::Gcd is not used elsewhere
}
