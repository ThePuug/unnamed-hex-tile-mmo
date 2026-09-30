use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::ReactionQueue, resources::*, Loc, recovery::GlobalRecovery},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::{queue::clear_threats, synergies::{lockout, reacts_through}},
    resources::map::Map,
};


/// Damage a Disengage adds to its caster's next auto-attack, spent by that
/// blow, which comes behind a feint: a damage-free threat queued ahead of
/// it, so a reaction that takes the front of the queue takes the feint. A
/// second Disengage before the blow replaces the first's.
#[derive(Clone, Component, Copy, Debug)]
pub struct Poised(pub f32);

/// The tiles a Disengage leaps from an attacker `distance` away with its own
/// `reach`: `tuned`, or as many as land it beyond that reach where that is
/// more, one tile of distance for each.
pub fn leap_tiles(tuned: usize, reach: i32, distance: i32) -> usize {
    tuned.max((reach + 1 - distance).max(0) as usize)
}

/// Handle Disengage, the Skirmisher's signature: a reaction to the blow at
/// the front of its queue, whose source is the event's target, and the blow
/// misses: the front threat is cleared. In contact, within the caster's own
/// reach of that source, it leaps away `Tuning::disengage_leap` tiles, or as
/// many as break that reach where that is more (`leap_tiles`), so its swings
/// come due unanswered;
/// already out of contact, it leaps `Tuning::disengage_close` tiles toward
/// the source, stopping beside it, since distance escapes no ranged blow. Its
/// next auto-attack strikes harder, by `disengage_intuition` of its
/// Intuition, behind a feint (`Poised`), and with the swings Patience banked
/// while it stood out of reach. Preparation lets it through a lockout
/// (`synergies::reacts_through`).
pub fn handle_disengage(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    mut queue_query: Query<&mut ReactionQueue>,
    recovery_query: Query<&GlobalRecovery>,
    range_query: Query<&common_bevy::components::AttackRange>,
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
        // Out of lockout, or a reaction Preparation lets through the lockout
        let recovery = recovery_query.get(*ent).ok();
        if recovery.is_some_and(|recovery| recovery.is_active())
            && !reacts_through(AbilityType::Disengage, recovery, attrs_query.get(*ent).ok())
        {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        let (Ok(caster_loc), Some(Ok(target_loc))) = (loc_query.get(*ent), target.map(|t| loc_query.get(t))) else {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NoTargets } });
            continue;
        };
        if stamina.state < tuning.disengage_cost {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }

        // Its leap stands on its own, so it goes with the cast: away from an
        // attacker in contact, onto one already out of it
        let distance = caster_loc.distance(target_loc);
        let own_reach = range_query.get(*ent).copied().unwrap_or_default().0;
        let landing = if distance <= own_reach {
            crate::systems::combat::leap::away(&map, **caster_loc, **target_loc, leap_tiles(tuning.disengage_leap, own_reach, distance))
        } else {
            crate::systems::combat::leap::toward(&map, **caster_loc, **target_loc, tuning.disengage_close)
        };
        let Some(landing) = landing else {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        };

        stamina.state -= tuning.disengage_cost;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });
        crate::systems::combat::leap::leap(*ent, landing, &mut commands, &mut writer);

        if let Ok(mut queue) = queue_query.get_mut(*ent) {
            if !clear_threats(&mut queue, ClearType::First(1)).is_empty() {
                writer.write(Do { event: GameEvent::ClearQueue { ent: *ent, clear_type: ClearType::First(1) } });
            }
        }
        if let Ok(attrs) = attrs_query.get(*ent) {
            commands.entity(*ent).insert(Poised(attrs.intuition() * tuning.disengage_intuition));
        }

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Disengage, target: *target } });
        commands.entity(*ent).insert(lockout(AbilityType::Disengage, recovery_query.get(*ent).ok(), None, target.and_then(|t| attrs_query.get(t).ok())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leap_always_breaks_its_own_reach() {
        assert_eq!(leap_tiles(1, 2, 1), 2, "from beside a reach of two, two tiles to stand at three");
        assert_eq!(leap_tiles(1, 2, 2), 1, "from the edge of reach, one");
        assert_eq!(leap_tiles(4, 2, 1), 4, "a longer tuned leap goes further");
        assert_eq!(leap_tiles(1, 2, 5), 1, "already clear, the tuned leap");
    }
}
