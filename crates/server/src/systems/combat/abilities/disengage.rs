use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::leap;

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

/// Disengage, the Skirmisher's signature: a reaction to the blow at the
/// front of its queue, whose source it is asked about, and the blow misses:
/// the front threat is cleared. In contact, within the caster's own reach of
/// that source, it leaps away `Tuning::disengage_leap` tiles, or as many as
/// break that reach where that is more (`leap_tiles`), so its swings come
/// due unanswered; already out of contact, it leaps
/// `Tuning::disengage_close` tiles toward the source, stopping beside it,
/// since distance escapes no ranged blow. With nowhere to leap it is out of
/// range. Its next auto-attack strikes harder, by `disengage_intuition` of
/// its Intuition, behind a feint (`Poised`), and with the swings Patience
/// banked while it stood out of reach.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (attacker, attacker_loc) = cast.struck()?;

    // Its leap stands on its own, so it goes with the cast: away from an
    // attacker in contact, onto one already out of it
    let distance = cast.loc.distance(&attacker_loc);
    let landing = if distance <= cast.reach {
        leap::away(&abilities.map, *cast.loc, *attacker_loc, leap_tiles(tuning.disengage_leap, cast.reach, distance))
    } else {
        leap::toward(&abilities.map, *cast.loc, *attacker_loc, tuning.disengage_close)
    };
    let Some(landing) = landing else {
        return Err(AbilityFailReason::OutOfRange);
    };
    leap::slide(cast.ent, landing, leap::LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);

    abilities.clear(cast.ent, common_bevy::message::ClearType::First(1));
    abilities.commands.entity(cast.ent).insert(Poised(cast.attrs.intuition() * tuning.disengage_intuition));
    Ok(Some(attacker))
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
