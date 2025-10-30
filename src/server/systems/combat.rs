use bevy::prelude::*;
use crate::common::{
    components::{reaction_queue::*, resources::*, ActorAttributes},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::queue as queue_utils,
};

/// Helper function to deal damage to an entity
/// Inserts threat into reaction queue if entity has one
/// Returns true if threat was queued, false if immediate damage was applied
pub fn deal_damage(
    commands: &mut Commands,
    target: Entity,
    source: Entity,
    damage: f32,
    damage_type: DamageType,
    queue_opt: Option<&mut ReactionQueue>,
    attrs_opt: Option<&ActorAttributes>,
    time: &Time,
    writer: &mut EventWriter<Do>,
) {
    // If target has reaction queue, insert threat
    if let (Some(queue), Some(attrs)) = (queue_opt, attrs_opt) {
        let now = time.elapsed();
        let timer_duration = queue_utils::calculate_timer_duration(attrs);

        let threat = QueuedThreat {
            source,
            damage,
            damage_type,
            inserted_at: now,
            timer_duration,
        };

        // Try to insert threat into queue
        let overflow = queue_utils::insert_threat(queue, threat, now);

        // Send InsertThreat event to clients
        writer.write(Do {
            event: GameEvent::InsertThreat {
                ent: target,
                threat,
            },
        });

        // If queue overflowed, immediately resolve the overflow threat
        if let Some(overflow_threat) = overflow {
            info!(
                "Queue overflow for {:?}: immediately resolving threat of {} damage from {:?}",
                target, overflow_threat.damage, overflow_threat.source
            );
            // Emit ResolveThreat event for the overflow
            commands.trigger_targets(
                Try {
                    event: GameEvent::ResolveThreat {
                        ent: target,
                        threat: overflow_threat,
                    },
                },
                target,
            );
        }
    } else {
        // No reaction queue - apply damage immediately
        info!(
            "No reaction queue for {:?}: immediate damage {} from {:?}",
            target, damage, source
        );
        // In MVP, just log - Phase 4 will apply actual damage
    }
}

/// System to resolve threats (apply damage with modifiers)
/// Processes ResolveThreat events emitted by expiry system or overflow
pub fn resolve_threat(
    trigger: Trigger<Try>,
    mut query: Query<(&mut Health, &ActorAttributes)>,
    mut writer: EventWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok((mut health, _attrs)) = query.get_mut(*ent) {
            // For MVP Phase 3: Just apply raw damage (Phase 4 will add modifiers)
            let damage_to_apply = threat.damage;

            // Apply damage to health
            health.state = (health.state - damage_to_apply).max(0.0);
            health.step = health.state; // Snap step to state for immediate feedback

            info!(
                "Resolved threat for {:?}: {} damage from {:?}, health now {}/{}",
                ent, damage_to_apply, threat.source, health.state, health.max
            );

            // Broadcast damage event to clients
            writer.write(Do {
                event: GameEvent::ApplyDamage {
                    ent: *ent,
                    damage: damage_to_apply,
                    source: threat.source,
                },
            });

            // Check for death
            if health.state <= 0.0 {
                info!("Entity {:?} died from threat", ent);
                // Death is a Try event (server-internal)
                // We need to use an EventWriter<Try> for this, but we only have EventWriter<Do>
                // For now, skip death handling in resolve_threat - it should be handled by check_death system
            }
        }
    }
}

/// Server system to validate and process ability usage
/// Handles UseAbility events from clients
pub fn handle_use_ability(
    mut reader: EventReader<Try>,
    mut query: Query<(&mut ReactionQueue, &mut Stamina, &ActorAttributes)>,
    mut writer: EventWriter<Do>,
) {
    for event in reader.read() {
        if let GameEvent::UseAbility { ent, ability } = event.event {
            match ability {
                AbilityType::Dodge => {
                    if let Ok((mut queue, mut stamina, _attrs)) = query.get_mut(ent) {
                        // Calculate dodge cost (15% of max stamina)
                        let dodge_cost = stamina.max * 0.15;

                        // Validate ability usage
                        if stamina.state < dodge_cost {
                            // Not enough stamina
                            writer.write(Do {
                                event: GameEvent::AbilityFailed {
                                    ent,
                                    reason: AbilityFailReason::InsufficientStamina,
                                },
                            });
                            // Send correct stamina state
                            writer.write(Do {
                                event: GameEvent::Incremental {
                                    ent,
                                    component: crate::common::message::Component::Stamina(*stamina),
                                },
                            });
                            continue;
                        }

                        if queue.is_empty() {
                            // Nothing to dodge
                            writer.write(Do {
                                event: GameEvent::AbilityFailed {
                                    ent,
                                    reason: AbilityFailReason::NoTargets,
                                },
                            });
                            continue;
                        }

                        // Valid dodge - consume stamina
                        stamina.state -= dodge_cost;
                        stamina.step = stamina.state;

                        // Clear queue
                        let cleared = queue_utils::clear_threats(&mut queue, ClearType::All);

                        info!(
                            "Server: {:?} dodged {} threats, stamina: {}/{}",
                            ent,
                            cleared.len(),
                            stamina.state,
                            stamina.max
                        );

                        // Broadcast clear queue event
                        writer.write(Do {
                            event: GameEvent::ClearQueue {
                                ent,
                                clear_type: ClearType::All,
                            },
                        });

                        // Broadcast updated stamina
                        writer.write(Do {
                            event: GameEvent::Incremental {
                                ent,
                                component: crate::common::message::Component::Stamina(*stamina),
                            },
                        });
                    }
                }
            }
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
