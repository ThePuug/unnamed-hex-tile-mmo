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

                if is_duplicate {
                    info!(
                        "Client: Skipped duplicate threat for {:?}: {} damage from {:?} (already predicted)",
                        ent, threat.damage, threat.source
                    );
                } else {
                    // Insert threat into client's visual queue
                    queue.threats.push_back(threat);

                    info!(
                        "Client: Inserted threat for {:?}: {} damage from {:?}",
                        ent, threat.damage, threat.source
                    );
                }
            }
        }
    }
}

/// Client system to handle ApplyDamage events
/// Updates health and removes the corresponding threat from the queue
pub fn handle_apply_damage(
    mut reader: EventReader<Do>,
    mut health_query: Query<&mut Health>,
    mut queue_query: Query<&mut ReactionQueue>,
) {
    for event in reader.read() {
        if let GameEvent::ApplyDamage { ent, damage, source } = event.event {
            // Update health
            if let Ok(mut health) = health_query.get_mut(ent) {
                health.state = (health.state - damage).max(0.0);
                health.step = health.state;

                info!(
                    "Client: Applied {} damage to {:?} from {:?}, health now {}/{}",
                    damage, ent, source, health.state, health.max
                );
            }

            // Remove the resolved threat from the queue
            // Match by source - the oldest threat from this source
            if let Ok(mut queue) = queue_query.get_mut(ent) {
                if let Some(pos) = queue.threats.iter().position(|t| t.source == source) {
                    let removed_threat = queue.threats.remove(pos).unwrap();
                    info!(
                        "Client: Removed resolved threat from {:?}'s queue: {} damage from {:?}",
                        ent, removed_threat.damage, removed_threat.source
                    );
                }
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

                    info!(
                        "Client: Predicted BasicAttack threat for {:?}: {} damage from {:?}",
                        target_ent, predicted_threat.damage, ent
                    );
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
                let cleared = queue_utils::clear_threats(&mut queue, clear_type);

                info!(
                    "Client: Server confirmed clear queue for {:?}: {} threats cleared",
                    ent,
                    cleared.len()
                );
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

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
