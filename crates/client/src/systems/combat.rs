use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, resources::*, target::Target, Loc, ActorAttributes},
    message::{AbilityType, Do, Event as GameEvent, Try},
    systems::combat::{damage as damage_calc, queue as queue_utils},
};

/// Client system to handle InsertThreat events
/// Inserts threats into the visual reaction queue for display
/// Also applies recovery pushback (Impact vs Composure) to match server behavior
/// No deduplication needed - we don't predict threat insertions
pub fn handle_insert_threat(
    mut reader: MessageReader<Do>,
    mut query: Query<(&mut ReactionQueue, &ActorAttributes, Option<&mut common_bevy::components::recovery::GlobalRecovery>)>,
    attrs_query: Query<&ActorAttributes>,
    l2r: Res<crate::resources::EntityMap>,
    time: Res<Time>,
    server: Res<crate::resources::Server>,
) {
    for event in reader.read() {
        if let GameEvent::InsertThreat { ent, threat } = event.event {
            if let Ok((mut queue, defender_attrs, recovery_opt)) = query.get_mut(ent) {
                // Calculate current server time
                let client_now = time.elapsed().as_millis();
                let server_now_ms = server.current_time(client_now);
                let server_now = std::time::Duration::from_millis(server_now_ms.min(u64::MAX as u128) as u64);

                // Insert always succeeds (unbounded queue)
                queue_utils::insert_threat(&mut queue, threat, server_now);

                // Recovery pushback: mirror server's Impact vs Composure contest
                if let Some(mut recovery) = recovery_opt {
                    // The threat carries the server's source; its attributes are ours
                    let source = l2r.get_by_right(&threat.source).copied();
                    if let Some(Ok(source_attrs)) = source.map(|source| attrs_query.get(source)) {
                        let pushback_pct = damage_calc::calculate_recovery_pushback(
                            source_attrs.impact(),
                            defender_attrs.composure(),
                            common_bevy::systems::combat::damage::level_edge(source_attrs.total_level(), defender_attrs.total_level()),
                        );
                        recovery.apply_pushback(pushback_pct);
                    }
                }
            }
        }
    }
}

/// Client system to handle ApplyDamage events
/// Removes the corresponding threat from the queue and spawns floating damage numbers
/// NOTE: Does NOT update health - server sends authoritative health via Incremental{Health}
/// Only spawns damage numbers over NPCs (outgoing damage), not over player (incoming damage shown in resolved threats)
pub fn handle_apply_damage(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    _health_query: Query<&mut Health>,
    _queue_query: Query<&ReactionQueue>,
    viewed: Query<Entity, With<crate::components::Viewed>>,
    transform_query: Query<&Transform>,
    time: Res<Time>,
) {
    // What lands on the actor the client sees as shows in its resolved stack
    let player_entity = viewed.single().ok();

    for event in reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, dot, .. } = event.event {
            // Don't remove from queue - ClearQueue event already did that!
            // This was causing double-removal and queue desync
            // (ApplyDamage is for damage display only, not queue management)

            // Skip player incoming damage - shown via resolved threats stack
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
                    font_size: FontSize::Px(32.0),
                    ..default()
                },
                TextColor(if dot { crate::systems::threat_icons::DOT_COLOR } else { Color::WHITE }),
                TextLayout::justify(Justify::Center),
                crate::components::FloatingText {
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

/// Client passive auto-attack system for players
/// Automatically sends AutoAttack Try events when player has an adjacent target
/// Runs periodically (every 500ms) to check for auto-attack opportunities

/// Auto-attack will only fire if:
/// - Player has a Target set (via reactive targeting system)
/// - Target is within its `AttackRange` and its facing cone
/// - 1.5s has elapsed since last auto-attack
pub fn player_auto_attack(
    mut writer: MessageWriter<Try>,
    mut player_query: Query<(Entity, &Loc, &Target, &mut common_bevy::components::LastAutoAttack, &common_bevy::components::ActorAttributes, Option<&common_bevy::components::AttackRange>, Option<&common_bevy::components::heading::Heading>, Option<&common_bevy::components::status::Status>)>,
    target_query: Query<&Loc>,
    input_queues: Res<common_bevy::resources::InputQueues>,
    time: Res<Time>,
) {
    let now = time.elapsed();

    for (player_ent, player_loc, player_target, mut last_auto_attack, attrs, attack_range_opt, heading, status) in &mut player_query {
        // Only process local player (entity with InputQueue)
        if input_queues.get(&player_ent).is_none() {
            continue;
        }

        // Check cooldown: the fixed interval, stretched by a daze
        let cooldown = common_bevy::components::status::Status::cadence(attrs.cadence_interval(), status);
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

        // Check if target is within auto-attack range (manhattan: flat hex distance + z difference)
        let max_range = attack_range_opt.copied().unwrap_or_default().0;
        if player_loc.distance(target_loc) > max_range {
            continue; // Target out of range
        }
        if !common_bevy::systems::targeting::faces(heading, attrs.arc(), player_loc, target_loc) {
            continue; // Target behind
        }

        // Send AutoAttack Try event with target entity
        writer.write(Try {
            event: GameEvent::UseAbility {
                ent: player_ent,
                ability: AbilityType::AutoAttack,
                target: Some(target_ent),
            },
        });

        // Update last attack time
        last_auto_attack.last_attack_time = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::message::ClearType;
    use std::time::Duration;

    #[test]
    fn a_threat_clears_after_its_source_is_gone_here() {
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.insert_resource(crate::resources::Server::default());
        app.init_resource::<crate::resources::EntityMap>();
        app.add_systems(Update, (handle_insert_threat, handle_clear_queue).chain());

        let player = app.world_mut().spawn((ReactionQueue::new(1), ActorAttributes::default())).id();
        let attacker = app.world_mut().spawn(ActorAttributes::default()).id();
        let on_server = Entity::from_raw_u32(9_000).unwrap();
        app.world_mut().resource_mut::<crate::resources::EntityMap>().insert(attacker, on_server);

        let threat = queue_utils::create_threat(
            on_server, &ActorAttributes::default(), &ActorAttributes::default(),
            50.0, DamageType::Physical, Some(AbilityType::Lunge), Duration::from_secs(10), 0.0,
        );
        app.world_mut().write_message(Do { event: GameEvent::InsertThreat { ent: player, threat } });
        app.update();
        assert_eq!(app.world().get::<ReactionQueue>(player).unwrap().threats.len(), 1);

        // The attacker dies and is despawned here before its threat lands
        app.world_mut().resource_mut::<crate::resources::EntityMap>().remove_by_left(&attacker);
        app.world_mut().despawn(attacker);
        let clear_type = ClearType::Threat { source: on_server, inserted_at: threat.inserted_at };
        app.world_mut().write_message(Do { event: GameEvent::ClearQueue { ent: player, clear_type } });
        app.update();
        assert!(app.world().get::<ReactionQueue>(player).unwrap().threats.is_empty(), "the landing clears it all the same");
    }
}
