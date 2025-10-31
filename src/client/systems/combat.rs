use bevy::prelude::*;
use crate::common::{
    components::{reaction_queue::*, resources::*, gcd::Gcd},
    message::{Do, Event as GameEvent},
    systems::combat::{queue as queue_utils, gcd::GcdType},
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

/// Client system to activate GCD component when GCD events are received from server
/// Listens to Do<Event::Gcd> and calls gcd.activate() on the local entity
/// This ensures client-side prediction checks (predict_dodge, etc.) see accurate GCD state
pub fn apply_gcd(
    mut reader: EventReader<Do>,
    mut query: Query<&mut Gcd>,
    time: Res<Time>,
) {
    for event in reader.read() {
        if let GameEvent::Gcd { ent, typ } = event.event {
            if let Ok(mut gcd) = query.get_mut(ent) {
                // Determine GCD duration based on type (must match server durations)
                let duration = match typ {
                    GcdType::Attack => std::time::Duration::from_secs(1),  // 1s for attacks
                    GcdType::Spawn(_) => std::time::Duration::from_millis(500),  // 0.5s for spawning
                    GcdType::PlaceSpawner(_) => std::time::Duration::from_secs(2),  // 2s for spawner placement
                };

                // Activate local GCD
                gcd.activate(typ, duration, time.elapsed());
            }
        }
    }
}
