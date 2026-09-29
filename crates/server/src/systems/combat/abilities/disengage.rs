use bevy::prelude::*;
use common_bevy::{
    components::{prepared::Prepared, reaction_queue::ReactionQueue, resources::*, Loc, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::queue::clear_threats,
    resources::map::Map,
};


/// Damage a Disengage adds to its caster's next auto-attack, spent by that
/// blow, which comes behind a feint: a damage-free threat queued ahead of
/// it, so a reaction that takes the front of the queue takes the feint. A
/// second Disengage before the blow replaces the first's.
#[derive(Clone, Component, Copy, Debug)]
pub struct Poised(pub f32);

/// The tiles a Disengage leaps from an attacker of `reach` standing
/// `distance` away: `tuned`, or as many as land it beyond that reach where
/// that is more, one tile of distance for each.
pub fn leap_tiles(tuned: usize, reach: i32, distance: i32) -> usize {
    tuned.max((reach + 1 - distance).max(0) as usize)
}

/// Handle Disengage, the Skirmisher's signature: a reaction to the blow at
/// the front of its queue, whose source is the event's target. The caster
/// leaps `Tuning::disengage_leap` tiles, or as many as clear that source's
/// reach where that is more (`leap_tiles`), each the neighbour furthest
/// from that source, and the blow misses: the front threat is cleared. Its
/// next auto-attack strikes harder, by `disengage_endurance` of its
/// Endurance, behind a feint (`Poised`).
///
/// With nothing queued, a Disengage is prepared where the Skirmisher's
/// Preparation has room (`super::prepare`); one prepared fires free, even
/// mid-lockout, and starts none.
pub fn handle_disengage(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    mut queue_query: Query<&mut ReactionQueue>,
    recovery_query: Query<&GlobalRecovery>,
    range_query: Query<&common_bevy::components::AttackRange>,
    mut prepared_query: Query<&mut Prepared>,
    respawn_query: Query<&RespawnTimer>,
    map: Res<Map>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Disengage, target } } = event else {
            continue;
        };
        if respawn_query.get(*ent).is_ok() {
            continue;
        }
        // A prepared Disengage fires whatever the lockout; any other needs to be out of it
        let held = prepared_query.get(*ent).is_ok_and(|prepared| prepared.holds(AbilityType::Disengage));
        if !held && recovery_query.get(*ent).is_ok_and(|recovery| recovery.is_active()) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        // Nothing queued to leap from: prepare it, where Preparation has room
        if queue_query.get(*ent).is_ok_and(|queue| queue.threats.is_empty()) {
            let (Ok(attrs), Ok(mut prepared)) = (attrs_query.get(*ent), prepared_query.get_mut(*ent)) else {
                continue;
            };
            let outcome = if held {
                Err(AbilityFailReason::NoTargets)
            } else {
                super::prepare(*ent, AbilityType::Disengage, attrs, &mut stamina, &mut prepared, recovery_query.get(*ent).ok().copied(), None, None, &mut commands, &mut writer)
            };
            if let Err(reason) = outcome {
                writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason } });
            }
            continue;
        }

        let (Ok(caster_loc), Some(Ok(target_loc))) = (loc_query.get(*ent), target.map(|t| loc_query.get(t))) else {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NoTargets } });
            continue;
        };
        if !held && stamina.state < tuning.disengage_cost {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }

        // Its leap stands on its own, so it goes with the cast
        let reach = target.and_then(|source| range_query.get(source).ok()).copied().unwrap_or_default().0;
        let tiles = leap_tiles(tuning.disengage_leap, reach, caster_loc.distance(target_loc));
        let Some(landing) = crate::systems::combat::leap::away(&map, **caster_loc, **target_loc, tiles) else {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        };

        // A prepared Disengage was paid for when it was prepared
        if !held {
            stamina.state -= tuning.disengage_cost;
            stamina.step = stamina.state;
            writer.write(Do {
                event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
            });
        }
        crate::systems::combat::leap::leap(*ent, landing, &mut commands, &mut writer);

        if let Ok(mut queue) = queue_query.get_mut(*ent) {
            if !clear_threats(&mut queue, ClearType::First(1)).is_empty() {
                writer.write(Do { event: GameEvent::ClearQueue { ent: *ent, clear_type: ClearType::First(1) } });
            }
        }
        if let Ok(attrs) = attrs_query.get(*ent) {
            commands.entity(*ent).insert(Poised(attrs.endurance() * tuning.disengage_endurance));
        }

        // A prepared Disengage is spent, and starts no lockout
        if held {
            if let Ok(mut prepared) = prepared_query.get_mut(*ent) {
                super::fire_prepared(*ent, AbilityType::Disengage, *target, &mut prepared, &mut writer);
            }
            continue;
        }
        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Disengage, target: *target } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Disengage), AbilityType::Disengage));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leap_always_clears_the_attackers_reach() {
        assert_eq!(leap_tiles(1, 2, 1), 2, "from beside a reach of two, two tiles to stand at three");
        assert_eq!(leap_tiles(1, 2, 2), 1, "from the edge of reach, one");
        assert_eq!(leap_tiles(4, 2, 1), 4, "a longer tuned leap goes further");
        assert_eq!(leap_tiles(1, 2, 5), 1, "already clear, the tuned leap");
    }
}
