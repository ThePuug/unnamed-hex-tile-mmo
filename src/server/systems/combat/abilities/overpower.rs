use bevy::prelude::*;
use crate::{
    common::{
        components::{resources::*, tier_lock::TierLock, target::Target, Loc, reaction_queue::DamageType, gcd::Gcd},
        message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
        systems::{targeting::get_range_tier, combat::gcd::GcdType},
    },
};

/// Handle Overpower ability (W key)
/// - 40 stamina cost
/// - 80 base damage
/// - Melee range (adjacent hex)
/// - Triggers Attack GCD
pub fn handle_overpower(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    entity_query: Query<&Loc>,
    loc_target_query: Query<(&Loc, &Target, Option<&TierLock>)>,
    mut stamina_query: Query<&mut Stamina>,
    mut gcd_query: Query<&mut Gcd>,
    respawn_query: Query<&RespawnTimer>,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Overpower only
        let Some(AbilityType::Overpower) = (ability == &AbilityType::Overpower).then_some(ability) else {
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

        // Overpower is melee only - check distance (must be adjacent = distance 1)
        let Some(target_loc) = entity_query.get(target_ent).ok() else {
            continue;
        };

        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 1 {
            // Target is too far for melee attack
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina cost (40)
        let stamina_cost = 40.0;
        let base_damage = 80.0;

        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
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
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Emit DealDamage event
        commands.trigger_targets(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage,
                    damage_type: DamageType::Physical,
                },
            },
            target_ent,
        );

        // Trigger Attack GCD immediately (prevents race conditions)
        if let Ok(mut gcd) = gcd_query.get_mut(*ent) {
            let gcd_duration = std::time::Duration::from_secs(1); // 1s for Attack GCD (ADR-006)
            gcd.activate(GcdType::Attack, gcd_duration, time.elapsed());
        }
    }
}
