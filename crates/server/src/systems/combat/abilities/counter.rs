use bevy::prelude::*;
use std::time::Duration;
use common_bevy::{
    components::{entity_type::*, resources::*, Loc, reaction_queue::{ReactionQueue, QueuedThreat}, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::synergies::apply_synergies,
};
use crate::resources::RunTime;

/// Reactive counter-attack. Costs 30 stamina and clears as many threats from
/// the front of the queue as the window holds. Each cleared threat goes back
/// to its living source wherever it stands, at a share of Technique plus a
/// share of the threat's damage, both `ArchetypeTuning`'s, and lands at once: a
/// reflection never enters the source's queue, so it cannot be countered.
pub fn handle_counter(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut queue_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&common_bevy::components::recovery::SynergyUnlock>,
    respawn_query: Query<&RespawnTimer>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: _ } } = event else {
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

        // Get caster's attributes
        let Ok(caster_attrs) = attrs_query.get(*ent) else {
            continue;
        };

        if queue_query.get(*ent).is_err() {
            // No ReactionQueue component - can't use counter
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        // Get all visible window threats to counter (collect and drop borrow)
        let (visible_threats, _window_size) = {
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

        // A reflection needs a living source to go back to
        let can_reflect_to = |target: Entity| -> bool {
            respawn_query.get(target).is_err() && entity_query.get(target).is_ok()
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
                component: common_bevy::message::Component::Stamina(*stamina),
            },
        });

        // Use game world time (server uptime + offset) for consistent time base
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        // Counter each visible threat and reflect damage back
        use common_bevy::systems::combat::queue::create_threat;

        for threat in &visible_threats {
            if !can_reflect_to(threat.source) {
                continue;
            }

            let reflected_damage = caster_attrs.technique() * tuning.counter_technique + threat.damage * tuning.counter_reflect;

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

            // A reflection lands on impact, never queued, so a counter cannot be countered back
            commands.trigger(Try {
                event: GameEvent::ResolveThreat { ent: threat.source, threat: reflected_threat },
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

        // Broadcast ability success to clients (client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Counter,
                target: None,  // Counter is self-targeted
            },
        });

        // Trigger recovery lockout (server-side state)
        let recovery_duration = get_ability_recovery_duration(AbilityType::Counter);
        let recovery = GlobalRecovery::new(recovery_duration, AbilityType::Counter);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state,)
        // Note: Counter uses same ability type as Knockback for Overpower synergy
        // Self-cast: both attacker and defender are the same entity
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        apply_synergies(*ent, AbilityType::Counter, &recovery, attrs, attrs, &mut commands);
    }
}
