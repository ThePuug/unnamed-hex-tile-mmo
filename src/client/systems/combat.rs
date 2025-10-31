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
                let new_health = (health.state - damage).max(0.0);

                // Check if prediction was correct (for local player)
                let prediction_error = (health.step - new_health).abs();
                if prediction_error > 0.1 {
                    warn!("CLIENT: Health prediction mismatch! Predicted: {:.1}, Actual: {:.1}, Error: {:.1}",
                        health.step, new_health, prediction_error);
                    // Rollback: snap step to server's authoritative value
                    health.step = new_health;
                }

                health.state = new_health;
                health.step = new_health; // Ensure sync for non-predicted entities (NPCs)
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

/// Client system to predict local player damage when threats expire
/// Provides instant visual feedback for health changes
pub fn predict_threat_resolution(
    mut query: Query<(&mut ReactionQueue, &mut Health, &ActorAttributes), With<crate::common::components::Actor>>,
    time: Res<Time>,
    server: Res<crate::client::resources::Server>,
) {
    for (mut queue, mut health, attrs) in &mut query {
        // Calculate current time (using server time for consistency)
        let now_ms = server.current_time(time.elapsed().as_millis());
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        // Check for expired threats (same logic as server)
        let expired_threats = crate::common::systems::combat::queue::check_expired_threats(&queue, now);

        // Predict damage for each expired threat
        for threat in expired_threats {
            // Calculate final damage using Phase 2 mitigation
            let final_damage = crate::common::systems::combat::damage::apply_passive_modifiers(
                threat.damage,
                attrs,
                threat.damage_type,
            );

            // Apply predicted damage to health.step (not state - that's server-authoritative)
            health.step = (health.step - final_damage).max(0.0);

            info!("CLIENT PREDICTION: Threat expired, predicted damage: {:.1}, new health.step: {:.1}",
                final_damage, health.step);
        }

        // Remove expired threats from queue (client-side cleanup)
        // Server will send ApplyDamage events which will confirm/correct our prediction
        queue.threats.retain(|threat| {
            let time_since_insert = now.saturating_sub(threat.inserted_at);
            time_since_insert < threat.timer_duration
        });
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

/// System to spawn health bars for entities that need them
/// Health bars are shown for entities in combat or with HP < max
pub fn spawn_health_bars(
    mut commands: Commands,
    query: Query<(Entity, &Health)>,
    existing_bars: Query<&crate::client::components::HealthBar>,
) {
    for (entity, health) in &query {
        // Check if this entity already has a health bar
        let has_bar = existing_bars.iter().any(|bar| bar.tracked_entity == entity);
        if has_bar {
            continue;
        }

        // Spawn health bar if entity is damaged (check both state and step for client prediction)
        let is_damaged = health.state < health.max || health.step < health.max;

        if is_damaged {

            // Create container with relative positioning for children
            let container = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),
                    height: Val::Px(6.0),
                    ..default()
                },
                crate::client::components::HealthBar {
                    tracked_entity: entity,
                },
            )).id();

            // Create background bar (grey, full width)
            let background = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
            )).id();

            // Create foreground bar (green, variable width)
            let foreground = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(50.0),  // Will be updated based on health ratio
                    height: Val::Px(6.0),
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.8, 0.2)),
                ZIndex(1), // Foreground on top
            )).id();

            // Add children to container
            commands.entity(container).add_children(&[background, foreground]);
        }
    }
}

/// System to update health bar positions and widths
/// Projects world positions to screen space and updates bar width based on health ratio
pub fn update_health_bars(
    mut commands: Commands,
    mut bar_query: Query<(Entity, &crate::client::components::HealthBar, &Children, &mut Node)>,
    mut child_node_query: Query<&mut Node, Without<crate::client::components::HealthBar>>,
    health_query: Query<&Health>,
    transform_query: Query<&Transform>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    for (bar_entity, health_bar, children, mut container_node) in &mut bar_query {
        // Get the tracked entity's health and transform
        let Ok(health) = health_query.get(health_bar.tracked_entity) else {
            // Entity no longer exists, despawn health bar
            commands.entity(bar_entity).despawn();
            continue;
        };

        let Ok(transform) = transform_query.get(health_bar.tracked_entity) else {
            continue;
        };

        // Check if entity should still show health bar (check both state and step)
        let is_full_health = health.state >= health.max && health.step >= health.max;
        if is_full_health {
            // Full health, despawn bar
            commands.entity(bar_entity).despawn();
            continue;
        }

        // Calculate world position (above entity)
        let world_pos = transform.translation + Vec3::new(0.0, 1.5, 0.0);

        // Project to screen space
        if let Ok(viewport_pos) = camera.world_to_viewport(camera_transform, world_pos) {
            // Update container position (centered horizontally)
            container_node.left = Val::Px(viewport_pos.x - 25.0);
            container_node.top = Val::Px(viewport_pos.y);

            // Update foreground bar width based on health ratio
            // Children: [0] = background, [1] = foreground
            if children.len() >= 2 {
                let foreground_entity = children[1];
                if let Ok(mut foreground_node) = child_node_query.get_mut(foreground_entity) {
                    let health_ratio = (health.step / health.max).clamp(0.0, 1.0);
                    foreground_node.width = Val::Px(50.0 * health_ratio);
                }
            }
        } else {
            // Off-screen, hide it
            container_node.left = Val::Px(-10000.0);
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
