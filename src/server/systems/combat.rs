use bevy::prelude::*;
use crate::common::{
    components::{entity_type::*, heading::*, reaction_queue::*, resources::*, gcd::Gcd, LastAutoAttack, *},
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
    mut query: Query<(&mut ReactionQueue, &ActorAttributes, &Health)>,
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

        // Get target's queue, attributes, and health
        let Ok((mut queue, attrs, health)) = query.get_mut(*target) else {
            return;
        };

        // Don't queue threats on dead targets
        if health.state <= 0.0 {
            return;
        }

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

            // Send authoritative health value to sync clients
            writer.write(Do {
                event: GameEvent::Incremental {
                    ent: *ent,
                    component: crate::common::message::Component::Health(*health),
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
                AbilityType::AutoAttack | AbilityType::Overpower => {
                    // Auto-Attack (passive, free, 20 dmg) and Overpower (W, 40 stam, 80 dmg)
                    // Both are melee attacks (adjacent hex range)
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
                        // BasicAttack is melee only - check distance (must be adjacent = distance 1)
                        let target_loc = entity_query.get(target_ent).ok().map(|(_, loc, _)| *loc);

                        if let Some(target_loc) = target_loc {
                            let distance = caster_loc.flat_distance(&target_loc) as u32;

                            if distance > 1 {
                                // Target is too far for melee attack
                                writer.write(Do {
                                    event: GameEvent::AbilityFailed {
                                        ent,
                                        reason: AbilityFailReason::OutOfRange,
                                    },
                                });
                                continue;
                            }

                            // Determine damage and stamina cost based on ability
                            let (base_damage, stamina_cost) = match ability {
                                AbilityType::AutoAttack => (20.0, 0.0),    // Free attack
                                AbilityType::Overpower => (80.0, 40.0),    // Heavy strike
                                _ => unreachable!(),
                            };

                            // Check stamina if ability has a cost
                            if stamina_cost > 0.0 {
                                if let Ok((_, mut stamina, _)) = param_set.p2().get_mut(ent) {
                                    if stamina.state < stamina_cost {
                                        writer.write(Do {
                                            event: GameEvent::AbilityFailed {
                                                ent,
                                                reason: AbilityFailReason::InsufficientStamina,
                                            },
                                        });
                                        continue;
                                    }
                                    stamina.state -= stamina_cost;
                                    stamina.step = stamina.state;

                                    // Broadcast updated stamina
                                    writer.write(Do {
                                        event: GameEvent::Incremental {
                                            ent,
                                            component: crate::common::message::Component::Stamina(*stamina),
                                        },
                                    });
                                }
                            }

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

                            // Only Overpower triggers GCD - AutoAttack is passive and shouldn't trigger GCD
                            if ability == AbilityType::Overpower {
                                writer.write(Do {
                                    event: GameEvent::Gcd {
                                        ent,
                                        typ: crate::common::systems::combat::gcd::GcdType::Attack,
                                    },
                                });
                            }
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
                AbilityType::Lunge => {
                    // Lunge: Gap closer (Q, 4 hex range, 20 stam, 40 dmg, teleport adjacent to target)
                    // Get caster's location and heading
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

                    // Use targeting system to find target
                    let target_opt = select_target(
                        ent,
                        caster_loc,
                        caster_heading,
                        None, // No tier lock
                        &nntree,
                        |target_ent| {
                            // Skip dead players
                            if respawn_query.get(target_ent).is_ok() {
                                return None;
                            }
                            entity_query.get(target_ent).ok().and_then(|(et, _, player_controlled_opt)| {
                                let target_is_player = player_controlled_opt.is_some();
                                // Asymmetric targeting: can only attack entities on opposite "team"
                                if caster_is_player != target_is_player {
                                    Some(*et)
                                } else {
                                    None
                                }
                            })
                        },
                    );

                    if let Some(target_ent) = target_opt {
                        let target_loc = entity_query.get(target_ent).ok().map(|(_, loc, _)| *loc);

                        if let Some(target_loc) = target_loc {
                            // Check range (must be within 4 hexes for Lunge)
                            let distance = caster_loc.flat_distance(&target_loc) as u32;

                            if distance > 4 || distance < 1 {
                                // Target is out of range (or we're already on top of them)
                                writer.write(Do {
                                    event: GameEvent::AbilityFailed {
                                        ent,
                                        reason: AbilityFailReason::OutOfRange,
                                    },
                                });
                                continue;
                            }

                            // Check stamina (20 cost)
                            let lunge_stamina_cost = 20.0;
                            if let Ok((_, mut stamina, _)) = param_set.p2().get_mut(ent) {
                                if stamina.state < lunge_stamina_cost {
                                    writer.write(Do {
                                        event: GameEvent::AbilityFailed {
                                            ent,
                                            reason: AbilityFailReason::InsufficientStamina,
                                        },
                                    });
                                    continue;
                                }

                                // Consume stamina
                                stamina.state -= lunge_stamina_cost;
                                stamina.step = stamina.state;

                                // Broadcast updated stamina
                                writer.write(Do {
                                    event: GameEvent::Incremental {
                                        ent,
                                        component: crate::common::message::Component::Stamina(*stamina),
                                    },
                                });
                            } else {
                                continue;
                            }

                            // Find landing position: adjacent to target, closest to caster
                            let target_neighbors = (*target_loc).neighbors();
                            let landing_loc = target_neighbors
                                .iter()
                                .min_by_key(|neighbor_loc| caster_loc.flat_distance(neighbor_loc))
                                .copied()
                                .unwrap_or(*target_loc); // Fallback to target loc if no neighbors

                            // Update caster's location (teleport)
                            // Need to update Loc component via commands
                            commands.entity(ent).insert(Loc::new(landing_loc));

                            // Broadcast location update to clients
                            writer.write(Do {
                                event: GameEvent::Incremental {
                                    ent,
                                    component: crate::common::message::Component::Loc(Loc::new(landing_loc)),
                                },
                            });

                            // Deal damage (40 base damage)
                            commands.trigger_targets(
                                Try {
                                    event: GameEvent::DealDamage {
                                        source: ent,
                                        target: target_ent,
                                        base_damage: 40.0,
                                        damage_type: DamageType::Physical,
                                    },
                                },
                                target_ent,
                            );

                            // Broadcast GCD event (Lunge triggers Attack GCD)
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
                AbilityType::Knockback => {
                    // Knockback: Push enemy (E, 2 hex range, 30 stam, push 1 hex away)
                    // Get caster's location and heading
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

                    // Use targeting system to find target
                    let target_opt = select_target(
                        ent,
                        caster_loc,
                        caster_heading,
                        None, // No tier lock
                        &nntree,
                        |target_ent| {
                            // Skip dead players
                            if respawn_query.get(target_ent).is_ok() {
                                return None;
                            }
                            entity_query.get(target_ent).ok().and_then(|(et, _, player_controlled_opt)| {
                                let target_is_player = player_controlled_opt.is_some();
                                // Asymmetric targeting: can only attack entities on opposite "team"
                                if caster_is_player != target_is_player {
                                    Some(*et)
                                } else {
                                    None
                                }
                            })
                        },
                    );

                    if let Some(target_ent) = target_opt {
                        let target_loc = entity_query.get(target_ent).ok().map(|(_, loc, _)| *loc);

                        if let Some(target_loc) = target_loc {
                            // Check range (must be within 2 hexes for Knockback)
                            let distance = caster_loc.flat_distance(&target_loc) as u32;

                            if distance > 2 || distance < 1 {
                                // Target is out of range (or we're on top of them)
                                writer.write(Do {
                                    event: GameEvent::AbilityFailed {
                                        ent,
                                        reason: AbilityFailReason::OutOfRange,
                                    },
                                });
                                continue;
                            }

                            // Check stamina (30 cost)
                            let knockback_stamina_cost = 30.0;
                            if let Ok((_, mut stamina, _)) = param_set.p2().get_mut(ent) {
                                if stamina.state < knockback_stamina_cost {
                                    writer.write(Do {
                                        event: GameEvent::AbilityFailed {
                                            ent,
                                            reason: AbilityFailReason::InsufficientStamina,
                                        },
                                    });
                                    continue;
                                }

                                // Consume stamina
                                stamina.state -= knockback_stamina_cost;
                                stamina.step = stamina.state;

                                // Broadcast updated stamina
                                writer.write(Do {
                                    event: GameEvent::Incremental {
                                        ent,
                                        component: crate::common::message::Component::Stamina(*stamina),
                                    },
                                });
                            } else {
                                continue;
                            }

                            // Calculate push direction: find neighbor of target that's furthest from caster
                            let target_neighbors = (*target_loc).neighbors();
                            let push_loc = target_neighbors
                                .iter()
                                .max_by_key(|neighbor_loc| caster_loc.flat_distance(neighbor_loc))
                                .copied()
                                .unwrap_or(*target_loc); // Fallback to current loc if no neighbors

                            // Update target's location (push)
                            commands.entity(target_ent).insert(Loc::new(push_loc));

                            // Broadcast location update to clients
                            writer.write(Do {
                                event: GameEvent::Incremental {
                                    ent: target_ent,
                                    component: crate::common::message::Component::Loc(Loc::new(push_loc)),
                                },
                            });

                            // Broadcast GCD event (Knockback triggers Attack GCD)
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
                AbilityType::Deflect => {
                    // Deflect: Clear all queued threats (R, 50 stamina)
                    // Get caster's queue and stamina from Query 2
                    if let Ok((mut queue, mut stamina, _attrs)) = param_set.p2().get_mut(ent) {
                        // Fixed deflect cost (ADR-009)
                        let deflect_cost = 50.0;

                        // Validate ability usage
                        if stamina.state < deflect_cost {
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
                            // Nothing to deflect (no queued threats)
                            writer.write(Do {
                                event: GameEvent::AbilityFailed {
                                    ent,
                                    reason: AbilityFailReason::NoTargets,
                                },
                            });
                            continue;
                        }

                        // Valid deflect - consume stamina
                        stamina.state -= deflect_cost;
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

                        // Broadcast GCD event (Deflect triggers Attack GCD)
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

/// CRITICAL: until we add a new system ... we need this one to bypass a random magic 
/// number of systems causing scheduling issues
pub fn do_nothing(){}

/// System to automatically trigger auto-attacks when adjacent to hostiles (ADR-009)
/// Runs periodically to check if actors have adjacent hostiles and can auto-attack
/// Auto-attack cooldown: 1.5s (1500ms)
pub fn process_passive_auto_attack(
    mut query: Query<
        (Entity, &Loc, &mut LastAutoAttack, Option<&Gcd>, &crate::server::systems::behaviour::Target),
        Without<crate::common::components::behaviour::PlayerControlled>
    >,
    entity_query: Query<(&EntityType, &Loc, Option<&RespawnTimer>)>,
    time: Res<Time>,
    runtime: Res<crate::server::resources::RunTime>,
    mut writer: EventWriter<Try>,
) {
    // Use game world time (server uptime + offset) for consistent time base
    let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
    let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

    const AUTO_ATTACK_COOLDOWN_MS: u64 = 1500; // 1.5 seconds

    // Only iterate over NPCs (entities Without PlayerControlled)
    for (ent, loc, mut last_auto_attack, gcd_opt, target) in query.iter_mut() {
        // Check if on GCD
        if let Some(gcd) = gcd_opt {
            if gcd.is_active(time.elapsed()) {
                continue; // Skip if on GCD
            }
        }

        // Check cooldown (1.5s between auto-attacks)
        let time_since_last_attack = now.saturating_sub(last_auto_attack.last_attack_time);
        if time_since_last_attack.as_millis() < AUTO_ATTACK_COOLDOWN_MS as u128 {
            continue; // Still on cooldown
        }

        // ADR-009: Check if NPC's locked target (from behavior tree) is adjacent
        // Get target's location
        let Ok((_, target_loc, respawn_timer_opt)) = entity_query.get(**target) else {
            continue; // Target entity doesn't exist or missing components
        };

        // Skip dead targets
        if respawn_timer_opt.is_some() {
            continue;
        }

        // Check if target is adjacent (distance == 1)
        let distance = loc.flat_distance(target_loc);
        if distance == 1 {
            // Target is adjacent - trigger auto-attack
            writer.write(Try {
                event: GameEvent::UseAbility {
                    ent,
                    ability: AbilityType::AutoAttack,
                },
            });

            // Update last attack time
            last_auto_attack.last_attack_time = now;
        }
    }
}

// Tests removed - need proper integration testing setup with Bevy's event system
// The core logic is tested through the common/systems/reaction_queue tests
