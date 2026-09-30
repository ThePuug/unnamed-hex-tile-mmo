use bevy::prelude::*;
use common_bevy::{
    components::{resources::*, tier_lock::TierLock, Loc, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    systems::{targeting::get_range_tier, combat::synergies::{apply_synergies, is_early, lockout, may_use, settle_combo}},
};

/// Handle Lunge ability (Q key)
/// - `Tuning::lunge_cost` stamina
/// - Strikes for `Tuning::lunge_force` of Force (scales with might + level)
/// - `LUNGE_RANGE` hex range
/// - Carries the caster over the ground to beside its target (`leap::toward`)
/// - Queues a strike and a wound whose DoT, a bleed, deals `Tuning::lunge_dot`
///   of Force each tick until a reaction clears the wound or it lands
pub fn handle_lunge(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<&Loc>,
    loc_tierlock_query: Query<(&Loc, Option<&TierLock>)>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&common_bevy::components::recovery::SynergyUnlock>,
    combo_query: Query<&common_bevy::components::recovery::Combo>,
    respawn_query: Query<&RespawnTimer>,
    heading_query: Query<&common_bevy::components::heading::Heading>,
    map: Res<common_bevy::resources::map::Map>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: event_target } } = event else {
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

        // Out of lockout, or taking the follow-up the last ability offered
        if !may_use(AbilityType::Lunge, recovery_query.get(*ent).ok(), synergy_query.get(*ent).ok(), combo_query.get(*ent).ok()) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }

        // Read target from the event (player's intended target)
        let target_ent_opt = *event_target;

        // Get caster's location and targeting state
        let Ok((caster_loc, targeting_state_opt)) = loc_tierlock_query.get(*ent) else {
            continue;
        };

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

        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > common_bevy::systems::combat::resources::LUNGE_RANGE || distance < 1 {
            // Target is out of range (or we're already on top of them)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        if !super::in_arc(heading_query.get(*ent).ok(), attrs_query.get(*ent).ok(), caster_loc, target_loc) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NotFacing } });
            continue;
        }

        // Where it lands: over the ground toward the target, as far as
        // beside it. With no way beside it the target is out of reach;
        // already beside it, the Lunge strikes from where it stands.
        let landing = crate::systems::combat::leap::toward(&map, **caster_loc, **target_loc, common_bevy::systems::combat::resources::LUNGE_RANGE as usize);
        if landing.unwrap_or(**caster_loc).flat_distance(&**target_loc) > 1 {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }

        let lunge_stamina_cost = common_bevy::tuning::tuning().lunge_cost;
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
                component: common_bevy::message::Component::Stamina(*stamina),
            },
        });

        // The charge: a fast dash along that way
        if let Some(landing) = landing {
            let charge_duration_ms = (distance as u16 * 50).max(100);
            crate::systems::combat::leap::slide(*ent, landing, charge_duration_ms, None, &mut commands, &mut writer);
        }

        // Deal damage (a share of the Force meta-attribute)
        let attrs = attrs_query.get(*ent).expect("Lunge caster must have ActorAttributes");
        let base_damage = attrs.force() * tuning.lunge_force;

        commands.trigger(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage,
                    ability: Some(AbilityType::Lunge),
                    dot: 0.0,
                },
            },
        );
        commands.trigger(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage: 0.0,
                    ability: Some(AbilityType::Lunge),
                    dot: attrs.force() * tuning.lunge_dot,
                },
            },
        );

        // Broadcast ability success to clients (client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Lunge,
                target: Some(target_ent),
            },
        });
        super::stride(*ent, heading_query.get(*ent).ok(), caster_loc, target_loc, &mut commands);

        // Trigger recovery lockout (server-side state)
        let (prior, offer) = (recovery_query.get(*ent).ok().copied(), synergy_query.get(*ent).ok().copied());
        let early = is_early(AbilityType::Lunge, prior.as_ref(), offer.as_ref());

        let recovery = lockout(AbilityType::Lunge, prior.as_ref(), offer.as_ref(), attrs_query.get(target_ent).ok());
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state,)
        // Contest: player's flow vs target's reflex
        let Ok(attacker_attrs) = attrs_query.get(*ent) else {
            continue;
        };
        let defender_attrs = attrs_query.get(target_ent).unwrap_or(attacker_attrs);
        apply_synergies(*ent, AbilityType::Lunge, &recovery, attacker_attrs, defender_attrs, &mut commands);
        settle_combo(*ent, AbilityType::Lunge, early, get_ability_recovery_duration(AbilityType::Lunge), attacker_attrs, combo_query.get(*ent).ok(), &mut commands);
    }
}
