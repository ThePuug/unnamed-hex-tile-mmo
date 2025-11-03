use bevy::prelude::*;
use crate::{
    common::{
        components::{reaction_queue::*, resources::*, gcd::Gcd},
        message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
        systems::combat::{gcd::GcdType, queue as queue_utils},
    },
};

/// Handle Deflect ability (R key) - defensive ability that clears all queued threats
/// - 50 stamina cost
/// - Clears ALL queued threats
/// - Requires at least one threat in queue
/// - Triggers Attack GCD
pub fn handle_deflect(
    mut reader: EventReader<Try>,
    mut queue_query: Query<(&mut ReactionQueue, &mut Stamina, &mut Gcd)>,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Deflect only
        let Some(AbilityType::Deflect) = (ability == &AbilityType::Deflect).then_some(ability) else {
            continue;
        };

        // Get caster's queue, stamina, and GCD
        let Ok((mut queue, mut stamina, mut gcd)) = queue_query.get_mut(*ent) else {
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

        // Trigger Attack GCD immediately (prevents race conditions)
        let gcd_duration = std::time::Duration::from_secs(1); // 1s for Attack GCD (ADR-006)
        gcd.activate(GcdType::Attack, gcd_duration, time.elapsed());
    }
}
