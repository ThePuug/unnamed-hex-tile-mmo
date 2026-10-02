use bevy::prelude::*;
use common_bevy::{components::Loc, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast, WHOLE};
use crate::leap::{away, slide, toward, LEAP_MS};

/// Leap, the Skirmisher's skill: an action that carries its user its
/// distance (`ActorAttributes::leap_tiles`) over the ground, by where its
/// target, a living hostile, stands.
///
/// In its user's reach, it leaps clear of the target, and the threats in
/// its user's span miss, paid for as a reaction's clearing is
/// ([`Abilities::answer_span`]).
///
/// Out of reach, it leaps toward the target and stops beside it, and one
/// that lands in reach strikes it for `Tuning::leap_strike` of base
/// potency as its Instinct line has it (`ActorAttributes::line_power`).
/// One that falls short strikes nothing.
///
/// An NPC's leap stops at its leash (`Abilities::leash`). With nowhere to
/// leap it is out of range.
pub fn leap(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let distance = cast.attrs.leap_tiles(&tuning);
    let (target, target_loc) = abilities.foe(cast)?;
    let leash = abilities.leash(cast.ent);
    let clear = cast.loc.distance(&target_loc) <= cast.reach;
    let landing = if clear {
        away(&abilities.map, *cast.loc, *target_loc, distance, leash)
    } else {
        toward(&abilities.map, *cast.loc, *target_loc, distance, leash)
    };
    let landing = landing.ok_or(AbilityFailReason::OutOfRange)?;
    slide(cast.ent, landing, LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);
    if clear {
        abilities.answer_span(cast);
    } else if Loc::new(landing).distance(&target_loc) <= cast.reach {
        let damage = cast.attrs.base_potency(&tuning) * tuning.leap_strike * cast.attrs.line_power(&tuning, AbilityType::Leap);
        abilities.strike(cast, target, damage, AbilityType::Leap, &WHOLE);
    }
    Ok(Some(target))
}
