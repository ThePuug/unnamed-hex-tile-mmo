use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::leap::{away, slide, toward, LEAP_MS};

/// Leap, the Skirmisher's skill: an action that carries its user
/// `Tuning::leap_distance` tiles over the ground, by where its target, a
/// living hostile, stands: clear of it where it is in its user's reach,
/// toward it, stopping beside it, where it is not.
///
/// An NPC's leap stops at its leash (`Abilities::leash`). With nowhere to
/// leap it is out of range.
pub fn leap(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let distance = common_bevy::tuning::tuning().leap_distance;
    let (target, target_loc) = abilities.foe(cast)?;
    let leash = abilities.leash(cast.ent);
    let landing = if cast.loc.distance(&target_loc) <= cast.reach {
        away(&abilities.map, *cast.loc, *target_loc, distance, leash)
    } else {
        toward(&abilities.map, *cast.loc, *target_loc, distance, leash)
    };
    let landing = landing.ok_or(AbilityFailReason::OutOfRange)?;
    slide(cast.ent, landing, LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);
    Ok(Some(target))
}
