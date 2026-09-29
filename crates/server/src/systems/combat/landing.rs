//! What a blow does beyond its damage, applied as it lands: a stun, a daze,
//! a slow, a Kiter's run. Every such effect waits in its target's queue with
//! the threat that carries it, so it lands no sooner than the damage, and a
//! reaction that clears the threat clears the effect with it. Only a wound's
//! DoT lands ahead of its blow, ticking while the wound stands.

use bevy::prelude::*;
use common_bevy::{
    components::{
        recovery::GlobalRecovery,
        status::{Daze, Status, Timed},
        stunned::Stunned,
    },
    message::{AbilityType, Component, Do, Event as GameEvent},
};

use common_bevy::tuning::Tuning;

/// The least pace a daze leaves: a dazed actor still moves and swings.
const MIN_PACE: f32 = 0.1;

/// Lands the effect of a blow from `ability`, struck by `source` on
/// `target`, as its threat resolves or is dismissed.
#[allow(clippy::too_many_arguments)]
pub fn land(
    ability: Option<AbilityType>,
    target: Entity,
    source: Entity,
    tuning: &Tuning,
    statuses: &mut Query<&mut Status>,
    recoveries: &Query<&GlobalRecovery>,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
) {
    match ability {
        // A stun holds its target completely, a lockout as long with it
        Some(AbilityType::Flank) => {
            let stunned = Stunned { remaining: tuning.flank_stun };
            let lockout = recoveries.get(target).map_or(0.0, |recovery| recovery.remaining).max(stunned.remaining);
            if let Ok(mut entity) = commands.get_entity(target) {
                entity.try_insert((stunned, GlobalRecovery::new(lockout, AbilityType::Flank)));
            }
            writer.write(Do { event: GameEvent::Incremental { ent: target, component: Component::Stunned(stunned) } });
        }
        Some(AbilityType::Rattle) => update(target, statuses, commands, writer, |status| {
            let stacks = Status::stacks_of(Some(status)).saturating_add(1).min(tuning.rattle_stacks);
            status.daze = Some(Daze { stacks, pace: (1.0 - tuning.rattle_daze * stacks as f32).max(MIN_PACE) });
        }),
        // Each shot that lands slows its target and sets its Kiter running afresh
        Some(AbilityType::Volley) => {
            update(target, statuses, commands, writer, |status| {
                status.slow = Some(Timed { pace: 1.0 - tuning.volley_slow, remaining: tuning.volley_slow_secs });
            });
            update(source, statuses, commands, writer, |status| {
                status.run = Some(Timed { pace: tuning.volley_run, remaining: tuning.volley_run_secs });
            });
        }
        _ => {}
    }
}

/// Changes `ent`'s status by `change`, giving it one if it has none, and
/// sends the whole of it.
fn update(
    ent: Entity,
    statuses: &mut Query<&mut Status>,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
    change: impl FnOnce(&mut Status),
) {
    let status = match statuses.get_mut(ent) {
        Ok(mut status) => {
            change(&mut status);
            *status
        }
        Err(_) => {
            let Ok(mut entity) = commands.get_entity(ent) else { return };
            let mut status = Status::default();
            change(&mut status);
            entity.try_insert(status);
            status
        }
    };
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Status(status) } });
}
