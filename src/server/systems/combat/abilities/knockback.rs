use bevy::prelude::*;
use qrz::Qrz;
use std::ops::Deref;
use crate::{
    common::{
        components::{entity_type::*, resources::*, Loc, gcd::Gcd, reaction_queue::ReactionQueue},
        message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
        systems::combat::gcd::GcdType,
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
/// - Triggers Attack GCD
pub fn handle_knockback(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut caster_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
    mut gcd_query: Query<&mut Gcd>,
    respawn_query: Query<&RespawnTimer>,
    _runtime: Res<RunTime>,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
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

        // Check GCD (must not be on cooldown)
        let Ok(gcd) = gcd_query.get(*ent) else {
            continue;
        };

        if gcd.is_active(time.elapsed()) {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OnCooldown,
                },
            });
            continue;
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

        // Trigger Attack GCD immediately (prevents race conditions)
        if let Ok(mut gcd) = gcd_query.get_mut(*ent) {
            let gcd_duration = std::time::Duration::from_secs(1); // 1s for Attack GCD (ADR-006)
            gcd.activate(GcdType::Attack, gcd_duration, time.elapsed());
        }
    }
}
