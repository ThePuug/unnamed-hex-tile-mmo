use bevy::prelude::*;
use crate::{
    common::{
        components::{resources::*, tier_lock::TierLock, target::Target, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
        message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
        systems::{targeting::get_range_tier, combat::synergies::apply_synergies},
    },
};

/// Handle Lunge ability (Q key)
/// - 20 stamina cost
/// - 40 base damage
/// - 4 hex range
/// - Teleports caster adjacent to target
pub fn handle_lunge(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<&Loc>,
    loc_target_query: Query<(&Loc, &Target, Option<&TierLock>)>,
    mut stamina_query: Query<&mut Stamina>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Lunge only
        let Some(AbilityType::Lunge) = (ability == &AbilityType::Lunge).then_some(ability) else {
            continue;
        };

        // Check if caster is dead (has RespawnTimer)
        if respawn_query.get(*ent).is_ok() {
            // Dead players can't use abilities - silently ignore
            continue;
        }

        // Check recovery lockout (universal lockout for all abilities)
        if let Ok(recovery) = recovery_query.get(*ent) {
            if recovery.is_active() {
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent: *ent,
                        reason: AbilityFailReason::OnCooldown,
                    },
                });
                continue;
            }
        }

        // Get caster's location, current target, and targeting state
        let Ok((caster_loc, target, targeting_state_opt)) = loc_target_query.get(*ent) else {
            continue;
        };

        // Get the current target from Target component
        let target_ent_opt = target.entity;  // Get the entity field (Option<Entity>)

        // If tier locked, validate target is in correct tier
        let validated_target = if let (Some(targeting_state), Some(target_ent)) = (targeting_state_opt, target_ent_opt) {
            if let Some(locked_tier) = targeting_state.get() {
                // Tier locked - validate target is in the correct tier
                if let Ok(target_loc) = entity_query.get(target_ent) {
                    let distance = caster_loc.flat_distance(target_loc) as u32;
                    let target_tier = get_range_tier(distance);

                    if target_tier == locked_tier {
                        Some(target_ent) // Target is in correct tier
                    } else {
                        None // Target not in locked tier, can't use ability
                    }
                } else {
                    None // Target doesn't exist
                }
            } else {
                target_ent_opt // Not tier locked, use target as-is
            }
        } else {
            target_ent_opt // No targeting state or no target
        };

        let Some(target_ent) = validated_target else {
            // No valid target
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Check if target is alive
        if respawn_query.get(target_ent).is_ok() {
            // Target is dead
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        let Some(target_loc) = entity_query.get(target_ent).ok() else {
            continue;
        };

        // Check range (must be within 4 hexes for Lunge)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 4 || distance < 1 {
            // Target is out of range (or we're already on top of them)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina (20 cost)
        let lunge_stamina_cost = 20.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < lunge_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
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
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Find landing position: adjacent to target, closest to caster
        let target_neighbors = (**target_loc).neighbors();
        let landing_loc = target_neighbors
            .iter()
            .min_by_key(|neighbor_loc| caster_loc.flat_distance(neighbor_loc))
            .copied()
            .unwrap_or(**target_loc); // Fallback to target loc if no neighbors

        // Update caster's location (teleport 2+ hexes to target's neighbor)
        commands.entity(*ent).insert(Loc::new(landing_loc));

        // Broadcast Loc update to clients
        // NOTE: Client detects teleport by hex distance (>=2 hexes) and snaps VisualPosition automatically
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Loc(Loc::new(landing_loc)),
            },
        });

        // Deal damage (40 base damage)
        commands.trigger(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage: 40.0,
                    damage_type: DamageType::Physical,
                    ability: Some(AbilityType::Lunge),
                },
            },
        );

        // Broadcast ability success to clients (ADR-012: client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Lunge,
                target_loc: Some(**target_loc),
            },
        });

        // Trigger recovery lockout (server-side state)
        let recovery_duration = get_ability_recovery_duration(AbilityType::Lunge);
        let recovery = GlobalRecovery::new(recovery_duration, AbilityType::Lunge);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state)
        apply_synergies(*ent, AbilityType::Lunge, &recovery, &mut commands);
    }
}
