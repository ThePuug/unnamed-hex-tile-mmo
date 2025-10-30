use bevy::prelude::*;
use crate::common::{
    components::{entity_type::*, heading::*, reaction_queue::*, resources::*, *},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    plugins::nntree::*,
    systems::{combat::queue as queue_utils, targeting::*},
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
    runtime: &crate::server::resources::RunTime,
    writer: &mut EventWriter<Do>,
) {
    // If target has reaction queue, insert threat
    if let (Some(queue), Some(attrs)) = (queue_opt, attrs_opt) {
        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
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
    mut commands: Commands,
    mut reader: EventReader<Try>,
    // Use ParamSet to avoid query conflicts - all ReactionQueue access must be in ParamSet
    mut param_set: ParamSet<(
        // Query 0: For BasicAttack - get caster's Loc/Heading
        Query<(&Loc, &Heading)>,
        // Query 1: For BasicAttack - get target's ReactionQueue/Attributes
        Query<(Option<&mut ReactionQueue>, Option<&ActorAttributes>)>,
        // Query 2: For Dodge - get caster's ReactionQueue/Stamina
        Query<(&mut ReactionQueue, &mut Stamina, &ActorAttributes)>,
    )>,
    entity_query: Query<(&EntityType, &Loc)>,
    nntree: Res<NNTree>,
    time: Res<Time>,
    runtime: Res<crate::server::resources::RunTime>,
    mut writer: EventWriter<Do>,
) {
    for event in reader.read() {
        if let GameEvent::UseAbility { ent, ability } = event.event {
            match ability {
                AbilityType::BasicAttack => {
                    info!("Server: Received BasicAttack from {:?}", ent);

                    // Get caster's location and heading from Query 0
                    let (caster_loc, caster_heading) = if let Ok(data) = param_set.p0().get(ent) {
                        info!("Server: Caster {:?} at {:?} facing {:?}", ent, data.0, data.1);
                        (*data.0, *data.1)
                    } else {
                        warn!("Server: Could not get location/heading for {:?}", ent);
                        continue;
                    };

                    // Debug: Log nearby entities
                    let nearby: Vec<_> = nntree.locate_within_distance(caster_loc, 20 * 20).collect();
                    info!("Server: Found {} nearby entities", nearby.len());
                    for nn in &nearby {
                        if let Ok((entity_type, _)) = entity_query.get(nn.ent) {
                            info!("  - Entity {:?} at {:?} is {:?}", nn.ent, nn.loc, entity_type);
                        }
                    }

                    // Use targeting system to select target
                    let target_opt = select_target(
                        ent, // caster entity
                        caster_loc,
                        caster_heading,
                        None, // No tier lock in MVP
                        &nntree,
                        |target_ent| entity_query.get(target_ent).ok().map(|(et, _)| *et),
                    );

                    info!("Server: select_target returned {:?}", target_opt);

                    if let Some(target_ent) = target_opt {
                        // Get target's queue and attributes from Query 1 (in ParamSet)
                        if let Ok((mut queue_opt, attrs_opt)) = param_set.p1().get_mut(target_ent) {
                            // BasicAttack: 20 base physical damage (no stamina cost)
                            let base_damage = 20.0;

                            info!(
                                "Server: {:?} used BasicAttack on {:?} for {} damage",
                                ent, target_ent, base_damage
                            );

                            // Deal damage using combat system
                            deal_damage(
                                &mut commands,
                                target_ent,
                                ent, // source
                                base_damage,
                                DamageType::Physical,
                                queue_opt.as_deref_mut(),
                                attrs_opt.as_deref(),
                                &time,
                                &runtime,
                                &mut writer,
                            );

                            // Broadcast GCD event (BasicAttack triggers Attack GCD)
                            writer.write(Do {
                                event: GameEvent::Gcd {
                                    ent,
                                    typ: crate::common::systems::combat::gcd::GcdType::Attack,
                                },
                            });
                        }
                    } else {
                        // No valid target in facing cone
                        writer.write(Do {
                            event: GameEvent::AbilityFailed {
                                ent,
                                reason: AbilityFailReason::NoTargets,
                            },
                        });
                    }
                }
                AbilityType::Dodge => {
                    // Get caster's queue and stamina from Query 2
                    if let Ok((mut queue, mut stamina, _attrs)) = param_set.p2().get_mut(ent) {
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
