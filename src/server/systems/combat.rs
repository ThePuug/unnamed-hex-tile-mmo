use bevy::prelude::*;
use crate::common::{
    components::{entity_type::*, heading::*, reaction_queue::*, resources::*, gcd::Gcd, *},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    plugins::nntree::*,
    systems::{
        combat::{damage as damage_calc, queue as queue_utils, gcd::GcdType},
        targeting::*,
    },
};

/// System to process DealDamage events (Phase 1: Outgoing damage calculation)
/// Rolls for crit, calculates outgoing damage, inserts into reaction queue
pub fn process_deal_damage(
    trigger: Trigger<Try>,
    mut commands: Commands,
    mut query: Query<(&mut ReactionQueue, &ActorAttributes)>,
    all_attrs: Query<&ActorAttributes>,
    time: Res<Time>,
    runtime: Res<crate::server::resources::RunTime>,
    mut writer: EventWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::DealDamage { source, target, base_damage, damage_type } = event {
        // Get attacker attributes for scaling
        let Ok(source_attrs) = all_attrs.get(*source) else {
            return;
        };

        // Roll for critical hit
        let (_was_crit, crit_mult) = damage_calc::roll_critical(source_attrs);

        // Calculate outgoing damage (Phase 1)
        let outgoing = damage_calc::calculate_outgoing_damage(*base_damage, source_attrs, *damage_type);
        let outgoing_with_crit = outgoing * crit_mult;

        // Get target's queue and attributes
        let Ok((mut queue, attrs)) = query.get_mut(*target) else {
            return;
        };

        // Insert threat into queue
        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
        let timer_duration = queue_utils::calculate_timer_duration(attrs);

        let threat = QueuedThreat {
            source: *source,
            damage: outgoing_with_crit,
            damage_type: *damage_type,
            inserted_at: now,
            timer_duration,
        };

        // Try to insert threat into queue
        let overflow = queue_utils::insert_threat(&mut queue, threat, now);

        // Send InsertThreat event to clients
        writer.write(Do {
            event: GameEvent::InsertThreat {
                ent: *target,
                threat,
            },
        });

        // If queue overflowed, immediately resolve the overflow threat
        if let Some(overflow_threat) = overflow {
            // Emit ResolveThreat event for the overflow
            commands.trigger_targets(
                Try {
                    event: GameEvent::ResolveThreat {
                        ent: *target,
                        threat: overflow_threat,
                    },
                },
                *target,
            );
        }
    }
}

/// System to resolve threats (Phase 2: Apply passive modifiers and apply to health)
/// Processes ResolveThreat events emitted by expiry system or overflow
pub fn resolve_threat(
    trigger: Trigger<Try>,
    _commands: Commands,
    mut query: Query<(&mut Health, &ActorAttributes)>,
    mut writer: EventWriter<Do>,
) {
    let event = &trigger.event().event;

    if let GameEvent::ResolveThreat { ent, threat } = event {
        if let Ok((mut health, attrs)) = query.get_mut(*ent) {
            // Apply passive modifiers (Phase 2)
            let final_damage = damage_calc::apply_passive_modifiers(
                threat.damage,
                attrs,
                threat.damage_type,
            );

            // Apply damage to health
            health.state = (health.state - final_damage).max(0.0);
            health.step = health.state; // Snap step to state for immediate feedback

            // Broadcast damage event to clients
            writer.write(Do {
                event: GameEvent::ApplyDamage {
                    ent: *ent,
                    damage: final_damage,
                    source: threat.source,
                },
            });

            // Death check moved to dedicated check_death system (decoupled from combat)
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
    entity_query: Query<(&EntityType, &Loc, Option<&crate::common::components::behaviour::PlayerControlled>)>,
    caster_respawn_query: Query<&RespawnTimer>,
    gcd_query: Query<&Gcd>,  // GCD validation query
    respawn_query: Query<&RespawnTimer>,
    nntree: Res<NNTree>,
    time: Res<Time>,
    runtime: Res<crate::server::resources::RunTime>,
    mut writer: EventWriter<Do>,
) {
    for event in reader.read() {
        if let GameEvent::UseAbility { ent, ability } = event.event {
            // Ignore abilities from dead players (those with RespawnTimer)
            if caster_respawn_query.get(ent).is_ok() {
                continue;
            }

            // Check GCD - if active, reject ability
            if let Ok(gcd) = gcd_query.get(ent) {
                if gcd.is_active(time.elapsed()) {
                    writer.write(Do {
                        event: GameEvent::AbilityFailed {
                            ent,
                            reason: AbilityFailReason::OnCooldown,
                        },
                    });
                    continue;
                }
            }

            match ability {
                AbilityType::BasicAttack => {
                    // Get caster's location and heading from Query 0
                    let (caster_loc, caster_heading) = if let Ok(data) = param_set.p0().get(ent) {
                        (*data.0, *data.1)
                    } else {
                        continue;
                    };

                    // Determine if caster is a player (for asymmetric targeting)
                    let caster_is_player = entity_query
                        .get(ent)
                        .ok()
                        .and_then(|(_, _, pc_opt)| pc_opt)
                        .is_some();

                    // Use targeting system to select target (asymmetric: players attack NPCs, NPCs attack players)
                    let target_opt = select_target(
                        ent, // caster entity
                        caster_loc,
                        caster_heading,
                        None, // No tier lock in MVP
                        &nntree,
                        |target_ent| {
                            // Skip entities with RespawnTimer (dead players)
                            if respawn_query.get(target_ent).is_ok() {
                                return None;
                            }
                            entity_query.get(target_ent).ok().and_then(|(et, _, player_controlled_opt)| {
                                let target_is_player = player_controlled_opt.is_some();

                                // Asymmetric targeting: can only attack entities on opposite "team"
                                // Players attack NPCs, NPCs attack players (no friendly fire)
                                if caster_is_player != target_is_player {
                                    Some(*et)
                                } else {
                                    None  // Same team - no friendly fire
                                }
                            })
                        },
                    );

                    if let Some(target_ent) = target_opt {
                        // BasicAttack: 20 base physical damage (no stamina cost)
                        let base_damage = 20.0;

                        // Emit DealDamage event
                        commands.trigger_targets(
                            Try {
                                event: GameEvent::DealDamage {
                                    source: ent,
                                    target: target_ent,
                                    base_damage,
                                    damage_type: DamageType::Physical,
                                },
                            },
                            target_ent,
                        );

                        // Broadcast GCD event (BasicAttack triggers Attack GCD)
                        writer.write(Do {
                            event: GameEvent::Gcd {
                                ent,
                                typ: crate::common::systems::combat::gcd::GcdType::Attack,
                            },
                        });
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
                        // Fixed dodge cost
                        let dodge_cost = 60.0;

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
                        queue_utils::clear_threats(&mut queue, ClearType::All);

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

                        // Broadcast GCD event (Dodge triggers Attack GCD)
                        writer.write(Do {
                            event: GameEvent::Gcd {
                                ent,
                                typ: crate::common::systems::combat::gcd::GcdType::Attack,
                            },
                        });
                    }
                }
            }
        }
    }
}

/// Server system to activate GCD component when GCD events are emitted
/// Listens to Do<Event::Gcd> and calls gcd.activate() on the entity
pub fn apply_gcd(
    mut reader: EventReader<Do>,
    mut query: Query<&mut Gcd>,
    time: Res<Time>,
) {
    for event in reader.read() {
        if let GameEvent::Gcd { ent, typ } = event.event {
            if let Ok(mut gcd) = query.get_mut(ent) {
                // Determine GCD duration based on type
                let duration = match typ {
                    GcdType::Attack => std::time::Duration::from_secs(1),  // 1s for attacks (ADR-006)
                    GcdType::Spawn(_) => std::time::Duration::from_millis(500),  // 0.5s for spawning entities
                    GcdType::PlaceSpawner(_) => std::time::Duration::from_secs(2),  // 2s for spawner placement
                };

                // Activate GCD
                gcd.activate(typ, duration, time.elapsed());
            }
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
