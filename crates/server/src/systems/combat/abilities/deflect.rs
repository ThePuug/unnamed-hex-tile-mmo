use bevy::prelude::*;
use common_bevy::{
    components::{prepared::Prepared, reaction_queue::*, resources::*, recovery::{Combo, GlobalRecovery, SynergyUnlock}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::{queue as queue_utils, synergies::{apply_synergies, is_early, lockout, may_use, settle_combo}},
};

/// Handle Deflect ability (R key) - defensive ability that clears all queued threats
/// - `Tuning::deflect_cost` stamina
/// - Clears ALL queued threats
/// - With none queued, it is prepared where the deflector's Preparation has
///   room (`super::prepare`); one prepared fires free, even mid-lockout, and
///   starts none
pub fn handle_deflect(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut queue_query: Query<(&mut ReactionQueue, &mut Stamina, &mut Prepared)>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&SynergyUnlock>,
    combo_query: Query<&Combo>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: _ } } = event else {
            continue;
        };

        // Filter for Deflect only
        let Some(AbilityType::Deflect) = (ability == &AbilityType::Deflect).then_some(ability) else {
            continue;
        };

        // Get caster's queue, stamina and what it holds prepared
        let Ok((mut queue, mut stamina, mut prepared)) = queue_query.get_mut(*ent) else {
            continue;
        };
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        let (prior, offer, combo) = (recovery_query.get(*ent).ok().copied(), synergy_query.get(*ent).ok().copied(), combo_query.get(*ent).ok());

        // A prepared Deflect fires whatever the lockout; any other needs to be out of it
        let held = prepared.holds(AbilityType::Deflect);
        if !held && !may_use(AbilityType::Deflect, prior.as_ref(), offer.as_ref(), combo) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }

        // Nothing to deflect: prepare it, where Preparation has room
        if queue.is_empty() {
            let outcome = if held {
                Err(AbilityFailReason::NoTargets)
            } else {
                super::prepare(*ent, AbilityType::Deflect, attrs, &mut stamina, &mut prepared, prior, offer, combo, &mut commands, &mut writer)
            };
            if let Err(reason) = outcome {
                writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason } });
            }
            continue;
        }

        // A prepared Deflect was paid for when it was prepared
        if held {
            super::fire_prepared(*ent, AbilityType::Deflect, &mut prepared, &mut writer);
        }
        let deflect_cost = if held { 0.0 } else { common_bevy::tuning::tuning().deflect_cost };

        // Validate ability usage
        if stamina.state < deflect_cost {
            // Not enough stamina
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            // Send correct stamina state
            writer.write(Do {
                event: GameEvent::Incremental {
                    ent: *ent,
                    component: common_bevy::message::Component::Stamina(*stamina),
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
                ent: *ent,
                clear_type: ClearType::All,
            },
        });

        // Broadcast updated stamina
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: common_bevy::message::Component::Stamina(*stamina),
            },
        });

        // A prepared Deflect starts no lockout and offers nothing
        if held {
            continue;
        }

        // Broadcast ability success to clients (client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Deflect,
                target: None, // Deflect is self-targeted
            },
        });

        // Trigger recovery lockout (server-side state)
        let early = is_early(AbilityType::Deflect, prior.as_ref(), offer.as_ref());
        let recovery = lockout(AbilityType::Deflect, prior.as_ref(), offer.as_ref());
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state,)
        // Self-cast: both attacker and defender are the same entity
        apply_synergies(*ent, AbilityType::Deflect, &recovery, attrs, attrs, &mut commands);
        settle_combo(*ent, AbilityType::Deflect, early, common_bevy::components::recovery::get_ability_recovery_duration(AbilityType::Deflect), attrs, combo, &mut commands);
    }
}
