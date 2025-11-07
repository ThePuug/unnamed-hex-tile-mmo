use bevy::prelude::*;
use crate::{
    common::{
        components::{reaction_queue::*, resources::*, recovery::{GlobalRecovery, get_ability_recovery_duration}},
        message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
        systems::combat::{queue as queue_utils, synergies::apply_synergies},
    },
};

/// Handle Deflect ability (R key) - defensive ability that clears all queued threats
/// - 50 stamina cost
/// - Clears ALL queued threats
/// - Requires at least one threat in queue
pub fn handle_deflect(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut queue_query: Query<(&mut ReactionQueue, &mut Stamina)>,
    recovery_query: Query<&GlobalRecovery>,
    mut writer: EventWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Deflect only
        let Some(AbilityType::Deflect) = (ability == &AbilityType::Deflect).then_some(ability) else {
            continue;
        };

        // Check recovery lockout (Deflect has no synergies, so simple check)
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

        // Get caster's queue and stamina
        let Ok((mut queue, mut stamina)) = queue_query.get_mut(*ent) else {
            continue;
        };

        // Fixed deflect cost (ADR-009)
        let deflect_cost = 50.0;

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
                    component: crate::common::message::Component::Stamina(*stamina),
                },
            });
            continue;
        }

        if queue.is_empty() {
            // Nothing to deflect (no queued threats)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
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
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Broadcast ability success to clients (ADR-012: client will apply recovery/synergies)
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Deflect,
                target_loc: None, // Deflect doesn't use target_loc
            },
        });

        // Trigger recovery lockout (server-side state)
        let recovery_duration = get_ability_recovery_duration(AbilityType::Deflect);
        let recovery = GlobalRecovery::new(recovery_duration, AbilityType::Deflect);
        commands.entity(*ent).insert(recovery);

        // Apply synergies (server-side state)
        apply_synergies(*ent, AbilityType::Deflect, &recovery, &mut commands);
    }
}
