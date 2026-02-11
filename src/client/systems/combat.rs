use bevy::prelude::*;
use crate::common::{
    components::{reaction_queue::*, resources::*, gcd::Gcd, target::Target, Loc},
    message::{AbilityType, Do, Event as GameEvent, Try},
    systems::combat::{queue as queue_utils, gcd::GcdType},
};

/// Client system to handle InsertThreat events
/// Inserts threats into the visual reaction queue for display
/// No deduplication needed - we don't predict threat insertions
pub fn handle_insert_threat(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut ReactionQueue>,
    time: Res<Time>,
    server: Res<crate::client::resources::Server>,
) {
    for event in reader.read() {
        if let GameEvent::InsertThreat { ent, threat } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                // Calculate current server time
                let client_now = time.elapsed().as_millis();
                let server_now_ms = server.current_time(client_now);
                let server_now = std::time::Duration::from_millis(server_now_ms.min(u64::MAX as u128) as u64);

                // Use insert_threat helper to properly handle queue capacity
                let _overflow = queue_utils::insert_threat(&mut queue, threat, server_now);
                // Note: We ignore overflow - server already handled it
            }
        }
    }
}

/// Client system to handle ApplyDamage events
/// Removes the corresponding threat from the queue and spawns floating damage numbers
/// NOTE: Does NOT update health - server sends authoritative health via Incremental{Health}
/// ADR-025: Only spawns damage numbers over NPCs (outgoing damage), not over player (incoming damage shown in resolved threats)
pub fn handle_apply_damage(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    _health_query: Query<&mut Health>,
    mut queue_query: Query<&mut ReactionQueue>,
    input_queues: Res<crate::common::resources::InputQueues>,
    transform_query: Query<&Transform>,
    time: Res<Time>,
) {
    // Local player is the entity with an InputQueue (only one on client)
    let player_entity = input_queues.entities().next().copied();

    for event in reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, source } = event.event {
            // Remove the resolved threat from the queue
            if let Ok(mut queue) = queue_query.get_mut(ent) {
                if let Some(pos) = queue.threats.iter().position(|t| t.source == source) {
                    queue.threats.remove(pos);
                }
            }

            // Skip player incoming damage - shown via resolved threats stack (ADR-025)
            let is_player_target = player_entity.map_or(false, |p| p == ent);
            if is_player_target {
                continue;
            }

            // Spawn floating damage number over the NPC
            // Entity stays alive for 3s in death pose, so Transform is available
            let Ok(transform) = transform_query.get(ent) else { continue; };
            let world_pos = transform.translation + Vec3::new(0.0, 2.5, 0.0);

            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Text::new(format!("{:.0}", damage)),
                TextFont {
                    font_size: 32.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Center),
                crate::client::components::FloatingText {
                    spawn_time: time.elapsed(),
                    world_position: world_pos,
                    lifetime: 1.5,
                    velocity: 1.0,
                },
            ));
        }
    }
}

/// Client system to handle ClearQueue events from server
/// Confirms queue clears (may be redundant with prediction but ensures sync)
pub fn handle_clear_queue(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut ReactionQueue>,
) {
    for event in reader.read() {
        if let GameEvent::ClearQueue { ent, clear_type } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                // Clear threats using message ClearType directly
                queue_utils::clear_threats(&mut queue, clear_type);
            }
        }
    }
}

/// Client system to handle AbilityFailed events
/// Rolls back optimistic prediction when server rejects ability use
pub fn handle_ability_failed(
    mut reader: MessageReader<Do>,
) {
    for event in reader.read() {
        if let GameEvent::AbilityFailed { ent: _, reason: _ } = &event.event {
            // TODO Phase 6: Show error message in UI
            // For now, server will send corrective Stamina and ClearQueue events
        }
    }
}

/// Client system to activate GCD component when GCD events are received from server
/// Listens to Do<Event::Gcd> and calls gcd.activate() on the local entity
/// This ensures client-side prediction checks (predict_dodge, etc.) see accurate GCD state
pub fn apply_gcd(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut Gcd>,
    time: Res<Time>,
) {
    for event in reader.read() {
        if let GameEvent::Gcd { ent, typ } = event.event {
            if let Ok(mut gcd) = query.get_mut(ent) {
                // GCD duration for attacks (must match server durations)
                let duration = match typ {
                    GcdType::Attack => std::time::Duration::from_secs(1),  // 1s for attacks
                };

                // Activate local GCD
                gcd.activate(typ, duration, time.elapsed());
            }
        }
    }
}

/// Client passive auto-attack system for players
/// Automatically sends AutoAttack Try events when player has an adjacent target
/// Runs periodically (every 500ms) to check for auto-attack opportunities
///
/// Auto-attack will only fire if:
/// - Player has a Target set (via reactive targeting system)
/// - Target is adjacent (distance == 1)
/// - No GCD active (attacks are free actions)
/// - 1.5s has elapsed since last auto-attack
pub fn player_auto_attack(
    mut writer: MessageWriter<Try>,
    mut player_query: Query<(Entity, &Loc, &Target, &mut crate::common::components::LastAutoAttack, Option<&Gcd>, &crate::common::components::ActorAttributes)>,
    target_query: Query<&Loc>,
    input_queues: Res<crate::common::resources::InputQueues>,
    time: Res<Time>,
) {
    let now = time.elapsed();

    for (player_ent, player_loc, player_target, mut last_auto_attack, gcd_opt, attrs) in &mut player_query {
        // Only process local player (entity with InputQueue)
        if input_queues.get(&player_ent).is_none() {
            continue;
        }

        // Skip if GCD active (though auto-attack shouldn't trigger GCD)
        if let Some(gcd) = gcd_opt {
            if gcd.is_active(time.elapsed()) {
                continue;
            }
        }

        // Check cooldown (tier-based cadence from Presence commitment)
        let cooldown = attrs.cadence_interval();
        let time_since_last_attack = now.saturating_sub(last_auto_attack.last_attack_time);
        if time_since_last_attack < cooldown {
            continue; // Still on cooldown
        }

        // Get target entity
        let Some(target_ent) = player_target.entity else {
            continue; // No target
        };

        // Get target location
        let Ok(target_loc) = target_query.get(target_ent) else {
            continue; // Target not found (may have despawned)
        };

        // Check if target is adjacent (considering slopes)
        if !player_loc.is_adjacent(target_loc) {
            continue; // Target not adjacent
        }

        // Send AutoAttack Try event with target location
        writer.write(Try {
            event: GameEvent::UseAbility {
                ent: player_ent,
                ability: AbilityType::AutoAttack,
                target_loc: Some(**target_loc),
            },
        });

        // Update last attack time
        last_auto_attack.last_attack_time = now;
    }
}
