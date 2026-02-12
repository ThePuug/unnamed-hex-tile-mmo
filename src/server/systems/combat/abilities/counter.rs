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
/// - Targets the source of the OLDEST threat in your reaction queue (front)
/// - Only works while threat is still in queue (1-1.75s window based on Instinct)
/// - Fully negates the threat's damage (you take 0 damage)
/// - Reflects 50% of the threat's damage back to attacker's ReactionQueue
pub fn handle_counter(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut queue_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
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

        // Get caster's location and reaction queue to read threat info
        let (caster_loc, threat, target_ent) = {
            let Ok((caster_loc, queue)) = queue_query.get(*ent) else {
                // No ReactionQueue component - can't use counter
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent: *ent,
                        reason: AbilityFailReason::NoTargets,
                    },
                });
                continue;
            };

            // Get the OLDEST threat from the queue (front = oldest)
            // This is the key difference from Knockback (which pops from back)
            let Some(threat) = queue.threats.front().copied() else {
                // No threats in queue - nothing to counter
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent: *ent,
                        reason: AbilityFailReason::NoTargets,
                    },
                });
                continue;
            };

            let target_ent = threat.source;
            (*caster_loc, threat, target_ent)
        };

        // Determine if we can reflect damage back (target alive, exists, and adjacent)
        let can_reflect = respawn_query.get(target_ent).is_err()
            && entity_query.get(target_ent).ok().map(|(_, loc)| {
                caster_loc.flat_distance(loc) as u32 == 1
            }).unwrap_or(false);

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

        // Calculate reflected damage (50% of the threat's damage)
        let reflected_damage = threat.damage * 0.5;

        // Queue reflected damage as a NEW threat in the attacker's ReactionQueue
        // (only if the attacker is still alive, exists, and is adjacent)
        if can_reflect {
        if let Ok((_, mut target_queue)) = queue_query.get_mut(target_ent) {
            // Calculate timer duration (standard threat timing - 1.5s base)
            // TODO: Could scale with target's Instinct for proper reaction window
            let timer_duration = Duration::from_millis(1500);

            // Use game world time (server uptime + offset) for consistent time base
            let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
            let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

            let reflected_threat = QueuedThreat {
                source: *ent,  // Counter caster is the source of reflected damage
                damage: reflected_damage,
                damage_type: threat.damage_type,  // Preserve original damage type
                inserted_at: now,
                timer_duration,
                ability: Some(AbilityType::Counter),
                precision_mod: 1.0, // Reflected damage uses neutral contest
            };

            // Add to back of target's queue (newest threat)
            target_queue.threats.push_back(reflected_threat);

            // Broadcast threat insertion to clients
            writer.write(Do {
                event: GameEvent::InsertThreat {
                    ent: target_ent,
                    threat: reflected_threat,
                },
            });
        }
        }

        // Remove the countered threat from the caster's queue (pop from FRONT)
        if let Ok((_, mut caster_queue)) = queue_query.get_mut(*ent) {
            caster_queue.threats.pop_front();
        }

        // Broadcast threat removal to clients
        writer.write(Do {
            event: GameEvent::ClearQueue {
                ent: *ent,
                clear_type: ClearType::First(1),  // Clear first (oldest) threat
            },
        });

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

        // Apply synergies (server-side state)
        // Note: Counter uses same ability type as Knockback for Overpower synergy
        apply_synergies(*ent, AbilityType::Counter, &recovery, &mut commands);
    }
}
