use bevy::prelude::*;
use common_bevy::{components::Loc, message::AbilityType};
use qrz::Qrz;

use super::{Abilities, AbilityFailReason, Blow, Cast};
use crate::leap::{away, slide, toward, LEAP_MS};

/// Where a Leap by `cast` lands, and on whom: its distance
/// (`ActorAttributes::leap_tiles`) over the ground, clear of its target, a
/// living hostile, where that stands in its user's reach, else toward it,
/// stopping beside it; an NPC's stops at its leash (`Abilities::leash`).
/// With nowhere to leap it is out of range.
pub fn landing(abilities: &Abilities, cast: &Cast) -> Result<(Entity, Loc, Qrz), AbilityFailReason> {
    let distance = cast.attrs.leap_tiles(&abilities.tuning);
    let (target, target_loc) = abilities.foe(cast)?;
    let leash = abilities.leash(cast.ent);
    let landing = if cast.loc.distance(&target_loc) <= cast.reach {
        away(&abilities.map, *cast.loc, *target_loc, distance, leash)
    } else {
        toward(&abilities.map, *cast.loc, *target_loc, distance, leash)
    };
    Ok((target, target_loc, landing.ok_or(AbilityFailReason::OutOfRange)?))
}

/// Leap, Discipline's defence: carries its user to its [`landing`] at
/// once. Clear of a target in reach, it strikes nothing. Toward one beyond
/// it, a leap that lands in reach strikes it for `Tuning::leap_strike` of
/// base potency as its line has it (`ActorAttributes::line_power`); one
/// that falls short strikes nothing.
pub fn leap(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let Ok((target, target_loc, landing)) = landing(abilities, cast) else { return };
    let dive = cast.loc.distance(&target_loc) > cast.reach;
    slide(cast.ent, landing, LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);
    if dive && Loc::new(landing).distance(&target_loc) <= cast.reach {
        let damage = cast.attrs.base_potency(&tuning) * tuning.damage(AbilityType::Leap) * cast.attrs.line_power(&tuning, AbilityType::Leap);
        abilities.attack(cast.ent, target, AbilityType::Leap, Blow { damage, ..Blow::default() });
    }
}
