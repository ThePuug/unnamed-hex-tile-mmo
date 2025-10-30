use bevy::prelude::*;
use crate::common::{
    components::{reaction_queue::*, resources::*, ActorAttributes},
    message::{AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::queue as queue_utils,
};

/// Client system to handle InsertThreat events
/// Inserts threats into the visual reaction queue for display
pub fn handle_insert_threat(
    mut reader: EventReader<Do>,
    mut query: Query<&mut ReactionQueue>,
) {
    for event in reader.read() {
        if let GameEvent::InsertThreat { ent, threat } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
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

/// Client system to predict Dodge ability usage
/// Optimistically clears queue and consumes stamina before server confirmation
pub fn predict_dodge(
    mut try_reader: EventReader<Try>,
    mut query: Query<(&mut ReactionQueue, &mut Stamina, &ActorAttributes)>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::Dodge } = event.event {
            if let Ok((mut queue, mut stamina, attrs)) = query.get_mut(ent) {
                // Calculate dodge cost (15% of max stamina as per ADR)
                let dodge_cost = stamina.max * 0.15;

                // Check if we have enough stamina
                if stamina.state >= dodge_cost && !queue.is_empty() {
                    // Optimistically clear queue
                    let cleared_count = queue.threats.len();
                    queue.threats.clear();

                    // Consume stamina
                    stamina.state -= dodge_cost;
                    stamina.step = stamina.state;

                    info!(
                        "Client: Predicted Dodge for {:?}, cleared {} threats, stamina: {}/{}",
                        ent, cleared_count, stamina.state, stamina.max
                    );
                } else {
                    warn!(
                        "Client: Cannot dodge - insufficient stamina ({}/{}) or empty queue",
                        stamina.state, stamina.max
                    );
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
                // Convert message ClearType to queue_utils ClearType
                let queue_clear_type = match clear_type {
                    ClearType::All => crate::common::systems::combat::queue::ClearType::All,
                    ClearType::First(n) => crate::common::systems::combat::queue::ClearType::First(n),
                    ClearType::ByType(dt) => crate::common::systems::combat::queue::ClearType::ByType(dt),
                };
                let cleared = queue_utils::clear_threats(&mut queue, queue_clear_type);

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
