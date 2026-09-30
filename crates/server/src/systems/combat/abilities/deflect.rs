use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, resources::*, recovery::{Combo, GlobalRecovery, SynergyUnlock}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::{queue as queue_utils, synergies::{apply_synergies, is_early, lockout, may_use, reacts_through, settle_combo}},
};

/// Handle Deflect ability (R key) - defensive ability that clears all queued threats
/// - `Tuning::deflect_cost` stamina
/// - Clears ALL queued threats
/// - Discipline's Preparation lets it through a lockout (`synergies::reacts_through`)
pub fn handle_deflect(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    mut queue_query: Query<(&mut ReactionQueue, &mut Stamina)>,
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

        // Get caster's queue and stamina
        let Ok((mut queue, mut stamina)) = queue_query.get_mut(*ent) else {
            continue;
        };
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        let (prior, offer, combo) = (recovery_query.get(*ent).ok().copied(), synergy_query.get(*ent).ok().copied(), combo_query.get(*ent).ok());

        // Out of lockout, or a reaction Preparation lets through the lockout
        if !may_use(AbilityType::Deflect, prior.as_ref(), offer.as_ref(), combo)
            && !reacts_through(AbilityType::Deflect, prior.as_ref(), Some(attrs))
        {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }

        if queue.is_empty() {
            // Nothing to deflect (no queued threats)
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NoTargets } });
            continue;
        }

        let deflect_cost = common_bevy::tuning::tuning().deflect_cost;

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
        let recovery = lockout(AbilityType::Deflect, prior.as_ref(), offer.as_ref(), None);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state,)
        // Self-cast: both attacker and defender are the same entity
        apply_synergies(*ent, AbilityType::Deflect, &recovery, attrs, attrs, &mut commands);
        settle_combo(*ent, AbilityType::Deflect, early, common_bevy::components::recovery::get_ability_recovery_duration(AbilityType::Deflect), attrs, combo, &mut commands);
    }
}
