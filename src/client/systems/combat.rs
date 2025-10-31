use bevy::prelude::*;
use std::time::Duration;
use crate::common::{
    components::{entity_type::*, heading::*, reaction_queue::*, resources::*, ActorAttributes, Loc},
    message::{AbilityType, Do, Try, Event as GameEvent},
    plugins::nntree::NNTree,
    systems::{combat::queue as queue_utils, targeting::*},
};

/// Client system to handle InsertThreat events
/// Inserts threats into the visual reaction queue for display
/// Skips duplicates if already predicted
pub fn handle_insert_threat(
    mut reader: EventReader<Do>,
    mut query: Query<&mut ReactionQueue>,
) {
    for event in reader.read() {
        if let GameEvent::InsertThreat { ent, threat } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                // Check if this threat was already predicted (deduplication)
                // Match by source and very close inserted_at timestamp (within 50ms tolerance)
                let is_duplicate = queue.threats.iter().any(|existing| {
                    existing.source == threat.source &&
                    existing.damage == threat.damage &&
                    existing.inserted_at.as_millis().abs_diff(threat.inserted_at.as_millis()) < 50
                });

                if !is_duplicate {
                    // Insert threat into client's visual queue
                    queue.threats.push_back(threat);
                }
            }
        }
    }
}

/// Client system to handle ApplyDamage events
/// Updates health and removes the corresponding threat from the queue
/// Spawns floating damage numbers above the entity
pub fn handle_apply_damage(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    mut health_query: Query<&mut Health>,
    mut queue_query: Query<&mut ReactionQueue>,
    transform_query: Query<&Transform>,
    time: Res<Time>,
) {
    for event in reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, source } = event.event {
            // Update health
            if let Ok(mut health) = health_query.get_mut(ent) {
                health.state = (health.state - damage).max(0.0);
                health.step = health.state;
            }

            // Remove the resolved threat from the queue
            // Match by source - the oldest threat from this source
            if let Ok(mut queue) = queue_query.get_mut(ent) {
                if let Some(pos) = queue.threats.iter().position(|t| t.source == source) {
                    queue.threats.remove(pos);
                }
            }

            // Spawn floating damage number above entity using UI system
            if let Ok(transform) = transform_query.get(ent) {
                let damage_text = format!("{:.0}", damage);
                let world_pos = transform.translation + Vec3::new(0.0, 2.5, 0.0);

                commands.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    Text::new(damage_text),
                    TextFont {
                        font_size: 32.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    TextLayout::new_with_justify(JustifyText::Center),
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
}

/// Client system to predict BasicAttack ability usage
/// Optimistically inserts threat into target's queue before server confirmation
pub fn predict_basic_attack(
    mut try_reader: EventReader<Try>,
    player_query: Query<(&Loc, &Heading), With<crate::common::components::Actor>>,
    mut target_query: Query<(Option<&mut ReactionQueue>, Option<&ActorAttributes>)>,
    entity_query: Query<(&EntityType, &Loc)>,
    nntree: Res<NNTree>,
    server: Res<crate::client::resources::Server>,
    time: Res<Time>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::BasicAttack } = event.event {
            // Get player's location and heading
            let Ok((player_loc, player_heading)) = player_query.get(ent) else { continue; };

            // Find target using same logic as server
            let target_opt = select_target(
                ent,
                *player_loc,
                *player_heading,
                None, // No tier lock in MVP
                &nntree,
                |target_ent| entity_query.get(target_ent).ok().map(|(et, _)| *et),
            );

            if let Some(target_ent) = target_opt {
                // Predict threat insertion (immediate UI feedback)
                if let Ok((Some(mut queue), Some(attrs))) = target_query.get_mut(target_ent) {
                    let now_ms = server.current_time(time.elapsed().as_millis());
                    let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
                    let timer_duration = queue_utils::calculate_timer_duration(attrs);

                    let predicted_threat = QueuedThreat {
                        source: ent,
                        damage: 20.0,
                        damage_type: DamageType::Physical,
                        inserted_at: now,
                        timer_duration,
                    };

                    // Insert predicted threat (client sees it immediately)
                    queue_utils::insert_threat(&mut queue, predicted_threat, now);
                }
            }
        }
    }
}

/// Client system to predict Dodge ability usage
/// Optimistically clears queue and consumes stamina before server confirmation
pub fn predict_dodge(
    mut try_reader: EventReader<Try>,
    mut query: Query<(&mut ReactionQueue, &mut Stamina, &ActorAttributes)>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::Dodge } = event.event {
            if let Ok((mut queue, mut stamina, _attrs)) = query.get_mut(ent) {
                // Calculate dodge cost (15% of max stamina as per ADR)
                let dodge_cost = stamina.max * 0.15;

                // Check if we have enough stamina
                if stamina.state >= dodge_cost && !queue.is_empty() {
                    // Optimistically clear queue
                    let _cleared_count = queue.threats.len();
                    queue.threats.clear();

                    // Consume stamina
                    stamina.state -= dodge_cost;
                    stamina.step = stamina.state;
                }
            }
        }
    }
}

/// Client system to handle ClearQueue events from server
/// Confirms queue clears (may be redundant with prediction but ensures sync)
pub fn handle_clear_queue(
    mut reader: EventReader<Do>,
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
    mut reader: EventReader<Do>,
) {
    for event in reader.read() {
        if let GameEvent::AbilityFailed { ent, reason } = &event.event {
            warn!("Client: Ability failed for {:?}: {:?}", ent, reason);
            // TODO Phase 6: Show error message in UI
            // For now, server will send corrective Stamina and ClearQueue events
        }
    }
}

/// System to update floating text (damage numbers)
/// Projects world position to screen space, moves text upward, fades out, and despawns
pub fn update_floating_text(
    mut commands: Commands,
    mut query: Query<(Entity, &mut crate::client::components::FloatingText, &mut Node, &mut TextColor)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    for (entity, mut floating_text, mut node, mut text_color) in &mut query {
        let elapsed = (time.elapsed() - floating_text.spawn_time).as_secs_f32();

        // Check if lifetime expired
        if elapsed >= floating_text.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Move upward in world space
        let delta = time.delta_secs();
        floating_text.world_position.y += floating_text.velocity * delta;

        // Project world position to screen space
        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, floating_text.world_position) {
            node.left = Val::Px(viewport_pos.x);
            node.top = Val::Px(viewport_pos.y);
        } else {
            // Position is behind camera or off-screen, hide it
            node.left = Val::Px(-1000.0);
        }

        // Fade out (alpha based on remaining lifetime)
        let alpha = 1.0 - (elapsed / floating_text.lifetime);
        text_color.0 = text_color.0.with_alpha(alpha);
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
