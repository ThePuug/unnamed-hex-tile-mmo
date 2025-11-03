use bevy::prelude::*;
use crate::{
    common::{
        components::{reaction_queue::*, resources::*},
        message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
        systems::combat::{queue as queue_utils, gcd::GcdType},
    },
    server::systems::combat::abilities::TriggerGcd,
};

/// Handle Deflect ability (R key) - defensive ability that clears all queued threats
/// - 50 stamina cost
/// - Clears ALL queued threats
/// - Requires at least one threat in queue
/// - Triggers Attack GCD
pub fn handle_deflect(
    mut reader: EventReader<Try>,
    mut queue_query: Query<(&mut ReactionQueue, &mut Stamina)>,
    mut writer: EventWriter<Do>,
    mut gcd_writer: EventWriter<TriggerGcd>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
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

        // Request GCD trigger (Deflect triggers Attack GCD)
        gcd_writer.write(TriggerGcd {
            ent: *ent,
            typ: GcdType::Attack,
        });
    }
}
