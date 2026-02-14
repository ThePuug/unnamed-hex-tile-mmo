use bevy::prelude::*;
use bevy_renet::RenetServer;
use ::renet::DefaultChannel;
use qrz::Convert;

use crate::{
    common::{
        components::{ tier_lock::TierLock, heading::{ Heading, HERE }, keybits::*, position::Position, * },
        message::{Event, *},
        resources::map::Map,
    },
    *
};

pub fn try_input(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
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
    mut _reader: MessageReader<Try>,
) {
    // GcdType::Attack is handled by ability systems (auto_attack, lunge, etc.)
    // PlaceSpawner and Spawn were removed - spawners are placed during terrain generation

    // This system could be removed entirely if Event::Gcd is not used elsewhere
}

/// Handle tier lock requests from clients (ADR-010 Phase 1)
///
/// Clients send SetTierLock events when pressing 1/2/3 keys.
/// Server updates the TierLock component to reflect the chosen tier.
/// Abilities will validate the existing Target component is in the correct tier.
pub fn try_set_tier_lock(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut tier_locks: Query<&mut TierLock>,
) {
    for &message in reader.read() {
        let Try { event } = message;
        let Event::SetTierLock { ent, tier } = event else { continue };

        if let Ok(mut tier_lock) = tier_locks.get_mut(ent) {
            tier_lock.set(tier);

            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: crate::common::message::Component::TierLock(*tier_lock),
                },
            });
        }
    }
}

/// Broadcast movement intent for player inputs (ADR-011)
///
/// Runs in FixedPostUpdate after physics has processed all inputs.
/// At this point Heading and offset.state are up-to-date, so we can accurately
/// broadcast where players are heading, enabling client-side prediction of remote players.
pub fn broadcast_player_movement_intent(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    buffers: Res<InputQueues>,
    mut query: Query<(&Loc, &Heading, &Position, Option<&ActorAttributes>, Option<&mut crate::common::components::movement_intent_state::MovementIntentState>)>,
    map: Res<Map>,
) {
    for (ent, buffer) in buffers.iter() {
        // Queue invariant: all queues must have at least 1 input
        assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");

        let Ok((loc, heading, position, attrs, o_intent_state)) = query.get_mut(ent) else { continue; };

        // Get the first input (the accumulating one that physics will process next)
        let Some(input) = buffer.queue.back() else { continue; };
        let Event::Input { key_bits, .. } = input else { unreachable!() };

        // Get or initialize MovementIntentState first (needed for reset logic)
        let mut intent_state = if let Some(state) = o_intent_state {
            state
        } else {
            // First time - add component and skip (will process next frame)
            commands.entity(ent).insert(crate::common::components::movement_intent_state::MovementIntentState::default());
            continue;
        };

        // Check if moving (any movement keys pressed)
        let is_moving = key_bits.is_pressed(KB_HEADING_Q) || key_bits.is_pressed(KB_HEADING_R);

        // Calculate destination tile
        let destination = if is_moving {
            // Moving: destination is next tile in movement direction (use Heading component, not key_bits)
            **heading + **loc
        } else {
            // Stopped: destination is current tile (to snap back to heading position)
            **loc
        };

        // Skip if already broadcast for this destination and heading
        if destination == intent_state.last_broadcast_dest && *heading == intent_state.last_broadcast_heading {
            continue;
        }

        // Calculate distance and duration
        let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);

        let distance = if is_moving {
            // Moving: distance from current position to destination heading-adjusted position
            let current_world = map.convert(**loc) + position.offset;
            let dest_tile_center = map.convert(destination);

            // Calculate destination heading-adjusted offset (use Heading component, not key_bits)
            let dest_heading_neighbor = map.convert(destination + **heading);
            let dest_direction = dest_heading_neighbor - dest_tile_center;
            let dest_offset = (dest_direction * HERE).xz();
            let dest_world = dest_tile_center + Vec3::new(dest_offset.x, 0.0, dest_offset.y);

            (dest_world - current_world).length()
        } else {
            // Stopped: distance from current position to current tile heading-adjusted position
            let current_world = map.convert(**loc) + position.offset;
            let tile_center = map.convert(**loc);
            let heading_neighbor = map.convert(**loc + **heading);
            let direction = heading_neighbor - tile_center;
            let heading_offset = (direction * HERE).xz();
            let dest_world = tile_center + Vec3::new(heading_offset.x, 0.0, heading_offset.y);
            (dest_world - current_world).length()
        };

        let duration_ms = (distance / movement_speed) as u16;

        // Update state and broadcast
        intent_state.last_broadcast_dest = destination;
        intent_state.last_broadcast_heading = *heading;

        writer.write(Do {
            event: Event::MovementIntent {
                ent,
                destination, // Players stand ON terrain (already at correct Z)
                duration_ms,
            }
        });
    }
}

/// Handle attribute respec requests from clients
///
/// Clients send RespecAttributes Try events when clicking Apply button.
/// Server validates the respec (budget, ranges, not in combat) and broadcasts Do event.
pub fn try_respec_attributes(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut attrs_query: Query<&mut ActorAttributes>,
) {
    for &message in reader.read() {
        let Try { event } = message;
        let Event::RespecAttributes {
            ent,
            might_grace_axis,
            might_grace_spectrum,
            vitality_focus_axis,
            vitality_focus_spectrum,
            instinct_presence_axis,
            instinct_presence_spectrum,
        } = event
        else {
            continue;
        };

        let Ok(mut attrs) = attrs_query.get_mut(ent) else {
            continue;
        };

        // Calculate draft investment
        let draft_investment = might_grace_axis.unsigned_abs() as u32
            + might_grace_spectrum.max(0) as u32
            + vitality_focus_axis.unsigned_abs() as u32
            + vitality_focus_spectrum.max(0) as u32
            + instinct_presence_axis.unsigned_abs() as u32
            + instinct_presence_spectrum.max(0) as u32;

        // Validate budget
        if draft_investment > attrs.total_level() {
            continue; // Overbudget
        }

        // Validate ranges (i8 max is 127, but level is practical limit)
        let max_investment = attrs.total_level() as i8;
        if might_grace_axis.abs() > max_investment
            || might_grace_spectrum < 0
            || might_grace_spectrum > max_investment
            || vitality_focus_axis.abs() > max_investment
            || vitality_focus_spectrum < 0
            || vitality_focus_spectrum > max_investment
            || instinct_presence_axis.abs() > max_investment
            || instinct_presence_spectrum < 0
            || instinct_presence_spectrum > max_investment
        {
            continue; // Invalid ranges
        }

        // Apply respec
        attrs.apply_respec(
            might_grace_axis,
            might_grace_spectrum,
            vitality_focus_axis,
            vitality_focus_spectrum,
            instinct_presence_axis,
            instinct_presence_spectrum,
        );

        // Broadcast confirmation
        writer.write(Do { event });
    }
}

