use bevy::prelude::*;
use std::time::Duration;
use crate::{
    common::{
        components::{entity_type::*, resources::*, Loc, reaction_queue::{ReactionQueue, QueuedThreat}, recovery::{GlobalRecovery, get_ability_recovery_duration}},
        message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
        systems::combat::synergies::apply_synergies,
    },
    server::resources::RunTime,
};

/// Handle Counter ability (ADR-014) - REACTIVE COUNTER-ATTACK
/// - 30 stamina cost
/// - 1 hex range (melee)
/// - Counters ALL visible threats in your reaction window (ADR-030)
/// - Reflects damage back for each countered threat
/// - Reflected damage per threat: base (20% force) + bonus (30% threat damage), capped at 2× force
pub fn handle_counter(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut queue_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&crate::common::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&crate::common::components::recovery::SynergyUnlock>,
    respawn_query: Query<&RespawnTimer>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Counter only
        let Some(AbilityType::Counter) = (ability == &AbilityType::Counter).then_some(ability) else {
            continue;
        };

        // Check if caster is dead (has RespawnTimer)
        if respawn_query.get(*ent).is_ok() {
            // Dead players can't use abilities - silently ignore
            continue;
        }

        // Check recovery lockout (unless synergy-unlocked)
        if let Ok(recovery) = recovery_query.get(*ent) {
            if recovery.is_active() {
                // Check if Counter is synergy-unlocked (Overpower → Counter synergy)
                let is_synergy_unlocked = synergy_query
                    .get(*ent)
                    .ok()
                    .map(|synergy| {
                        synergy.ability == AbilityType::Counter
                            && synergy.is_unlocked(recovery.remaining)
                    })
                    .unwrap_or(false);

                if !is_synergy_unlocked {
                    writer.write(Do {
                        event: GameEvent::AbilityFailed {
                            ent: *ent,
                            reason: AbilityFailReason::OnCooldown,
                        },
                    });
                    continue;
                }
            }
        }

        // Get caster's attributes and location
        let Ok(caster_attrs) = attrs_query.get(*ent) else {
            continue;
        };

        let caster_loc = {
            let Ok((loc, _)) = queue_query.get(*ent) else {
                // No ReactionQueue component - can't use counter
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent: *ent,
                        reason: AbilityFailReason::NoTargets,
                    },
                });
                continue;
            };
            *loc
        };

        // Get all visible window threats to counter (collect and drop borrow)
        let (visible_threats, window_size) = {
            let Ok((_, queue)) = queue_query.get(*ent) else {
                continue;
            };
            let window = queue.window_size;
            let threats: Vec<QueuedThreat> = queue.threats.iter()
                .take(window)
                .copied()
                .collect();
            (threats, window)
        };

        if visible_threats.is_empty() {
            // No threats in queue - nothing to counter
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Helper to check if we can reflect to a target
        let can_reflect_to = |target: Entity| -> bool {
            respawn_query.get(target).is_err()
                && entity_query.get(target).ok().map(|(_, loc)| {
                    caster_loc.flat_distance(loc) as u32 == 1
                }).unwrap_or(false)
        };

        // Check stamina (30 cost)
        let counter_stamina_cost = 30.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < counter_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        // Consume stamina
        stamina.state -= counter_stamina_cost;
        stamina.step = stamina.state;

        // Broadcast updated stamina
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        // Counter each visible threat and reflect damage back
        use crate::common::systems::combat::queue::create_threat;

        for threat in &visible_threats {
            // Only reflect if target is alive and adjacent
            if !can_reflect_to(threat.source) {
                continue;
            }

            // Calculate reflected damage for this specific threat
            let base_reflect = caster_attrs.force() * 0.2;  // 20% force minimum
            let threat_bonus = threat.damage * 0.3;          // 30% of countered damage
            let uncapped = base_reflect + threat_bonus;
            let cap = caster_attrs.force() * 2.0;            // Cap at 2× defender's force
            let reflected_damage = uncapped.min(cap);

            // Get target's queue and attributes
            let Ok((_, mut target_queue)) = queue_query.get_mut(threat.source) else {
                continue;
            };
            let Ok(target_attrs) = attrs_query.get(threat.source) else {
                continue;
            };

            // Create reflected threat using standard helper (INV-003)
            let reflected_threat = create_threat(
                *ent,                         // Source: Counter caster
                target_attrs,                 // Target: Original attacker
                caster_attrs,                 // Source attrs: Counter caster's stats
                reflected_damage,             // Damage amount
                threat.damage_type,           // Preserve damage type
                Some(AbilityType::Counter),   // Ability
                now,                          // Current time
            );

            // Add to target's queue
            target_queue.threats.push_back(reflected_threat);

            // Broadcast threat insertion
            writer.write(Do {
                event: GameEvent::InsertThreat {
                    ent: threat.source,
                    threat: reflected_threat,
                },
            });
        }

        // Remove all countered threats from caster's queue
        if let Ok((_, mut caster_queue)) = queue_query.get_mut(*ent) {
            // Remove all visible window threats (drain first N)
            let count = visible_threats.len();
            caster_queue.threats.drain(..count);

            // Broadcast threat removal to clients
            writer.write(Do {
                event: GameEvent::ClearQueue {
                    ent: *ent,
                    clear_type: ClearType::First(count),  // Clear all countered threats
                },
            });
        }

        // Broadcast ability success to clients (ADR-012: client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Counter,
                target_loc: None,  // Counter doesn't use target_loc
            },
        });

        // Trigger recovery lockout (server-side state)
        let recovery_duration = get_ability_recovery_duration(AbilityType::Counter);
        let recovery = GlobalRecovery::new(recovery_duration, AbilityType::Counter);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state, SOW-021 Phase 2)
        // Note: Counter uses same ability type as Knockback for Overpower synergy
        // Self-cast: both attacker and defender are the same entity
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        apply_synergies(*ent, AbilityType::Counter, &recovery, attrs, attrs, &mut commands);
    }
}
