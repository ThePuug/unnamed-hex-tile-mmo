use bevy::prelude::*;
use qrz::Qrz;
use std::ops::Deref;
use crate::{
    common::{
        components::{entity_type::*, resources::*, Loc, reaction_queue::ReactionQueue, recovery::{GlobalRecovery, get_ability_recovery_duration}},
        message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
        systems::combat::synergies::apply_synergies,
    },
    server::resources::RunTime,
};

/// Handle Knockback ability (E key) - REACTIVE COUNTER
/// - 30 stamina cost
/// - 2 hex range
/// - Targets the source of the most recent threat in your reaction queue
/// - Only works while threat is still in queue (1-1.75s window based on Instinct)
/// - Pushes attacker 1 hex directly away from you (using direction vector)
/// - Removes the threat from queue (cancels the incoming attack)
pub fn handle_knockback(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut caster_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&crate::common::components::recovery::SynergyUnlock>,
    respawn_query: Query<&RespawnTimer>,
    _runtime: Res<RunTime>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Knockback only
        let Some(AbilityType::Knockback) = (ability == &AbilityType::Knockback).then_some(ability) else {
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
                // Check if Knockback is synergy-unlocked (Overpower → Knockback)
                let is_synergy_unlocked = synergy_query
                    .get(*ent)
                    .ok()
                    .map(|synergy| {
                        synergy.ability == AbilityType::Knockback
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

        // Get caster's location and reaction queue
        let Ok((caster_loc, mut queue)) = caster_query.get_mut(*ent) else {
            // No ReactionQueue component - can't use knockback
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Get the most recent threat from the queue (back = newest)
        let Some(threat) = queue.threats.back().copied() else {
            // No threats in queue - nothing to knockback
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        let target_ent = threat.source;

        // Check if attacker is still alive
        if respawn_query.get(target_ent).is_ok() {
            // Attacker is dead
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        // Get attacker's location
        let Some(target_loc) = entity_query.get(target_ent).ok().map(|(_, loc)| *loc) else {
            // Attacker entity doesn't exist
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Check range (must be within 2 hexes for Knockback)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 2 || distance < 1 {
            // Target is out of range (or we're on top of them)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina (30 cost)
        let knockback_stamina_cost = 30.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < knockback_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
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
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Calculate push direction: directly away from caster
        // Get Qrz values via Deref trait (Loc derefs to Qrz)
        let target_qrz: Qrz = *target_loc.deref();  // target_loc is Loc, deref to &Qrz, then copy
        let caster_qrz: Qrz = *caster_loc.deref();  // caster_loc is &Loc, deref to &Qrz, then copy
        let direction = target_qrz - caster_qrz;

        // Among target's neighbors, pick the one most aligned with the away direction
        let target_neighbors = target_qrz.neighbors();
        let push_loc = target_neighbors
            .iter()
            .max_by_key(|&&neighbor| {
                // Calculate the step from target to this neighbor
                let step = neighbor - target_qrz;
                // Dot product: direction · step (higher = more aligned)
                direction.q * step.q + direction.r * step.r + direction.z * step.z
            })
            .copied()
            .unwrap_or(target_qrz); // Fallback to current loc if no neighbors

        // Update target's location (push)
        commands.entity(target_ent).insert(Loc::new(push_loc));

        // Broadcast location update to clients
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: target_ent,
                component: crate::common::message::Component::Loc(Loc::new(push_loc)),
            },
        });

        // Remove the threat from the queue (cancel the incoming attack)
        queue.threats.pop_back();

        // Broadcast threat removal to clients
        writer.write(Do {
            event: GameEvent::ClearQueue {
                ent: *ent,
                clear_type: ClearType::Last(1),
            },
        });

        // Broadcast ability success to clients (ADR-012: client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Knockback,
                target_loc: None, // Knockback doesn't use target_loc
            },
        });

        // Trigger recovery lockout (server-side state)
        let recovery_duration = get_ability_recovery_duration(AbilityType::Knockback);
        let recovery = GlobalRecovery::new(recovery_duration, AbilityType::Knockback);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state)
        apply_synergies(*ent, AbilityType::Knockback, &recovery, &mut commands);
    }
}
