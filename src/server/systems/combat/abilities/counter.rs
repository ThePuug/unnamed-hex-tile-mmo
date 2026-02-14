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

        // Calculate reflected damage: additive scaling with cap
        // - Base damage from defender's force (level-appropriate minimum)
        // - Bonus from incoming threat (rewards countering big attacks)
        // - Capped at defender's force (prevents low-level abuse)
        let reflected_damage = {
            let Ok(defender_attrs) = attrs_query.get(*ent) else {
                continue;
            };

            let base_reflect = defender_attrs.force() * 0.2;  // 20% force minimum
            let threat_bonus = threat.damage * 0.3;            // 30% of countered damage
            let uncapped = base_reflect + threat_bonus;
            let cap = defender_attrs.force() * 2.0;            // Cap at 2x defender's force

            uncapped.min(cap)
        };

        // Queue reflected damage as a NEW threat in the attacker's ReactionQueue
        // (only if the attacker is still alive, exists, and is adjacent)
        if can_reflect {
        if let Ok((_, mut target_queue)) = queue_query.get_mut(target_ent) {
            // Use canonical threat creation helper (INV-003: ensures consistent timers)
            use crate::common::systems::combat::queue::create_threat;

            let target_attrs = attrs_query.get(target_ent).unwrap();  // Target always has attrs
            let counter_attrs = attrs_query.get(*ent).unwrap();       // Counter source always has attrs

            // Use game world time (server uptime + offset) for consistent time base
            let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
            let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

            // Create threat using standard helper (guarantees consistent timer calculation)
            let reflected_threat = create_threat(
                *ent,                    // Source: Counter caster
                target_attrs,            // Target: Original attacker (receives reflected damage)
                counter_attrs,           // Source attrs: Counter caster's stats
                reflected_damage,        // Damage amount
                threat.damage_type,      // Preserve original damage type
                Some(AbilityType::Counter), // Ability that created this threat
                now,                     // Current time
            );

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

        // Apply synergies (server-side state, SOW-021 Phase 2)
        // Note: Counter uses same ability type as Knockback for Overpower synergy
        // Self-cast: both attacker and defender are the same entity
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        apply_synergies(*ent, AbilityType::Counter, &recovery, attrs, attrs, &mut commands);
    }
}
