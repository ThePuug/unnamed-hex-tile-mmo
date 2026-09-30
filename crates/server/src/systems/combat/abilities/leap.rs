use bevy::prelude::*;
use common_bevy::{components::Loc, message::{AbilityType, ClearType}};

use super::{Abilities, AbilityFailReason, Cast, WHOLE};
use crate::systems::combat::leap::{away, slide, toward, LEAP_MS};

/// An actor has traded: a blow of its own has landed since it last leapt.
/// An NPC leaps clear of a fight only once it has (`npc::skills`).
#[derive(Clone, Component, Copy, Debug)]
pub struct Traded;

/// Leap, the Skirmisher's skill: an action that carries its user
/// `Tuning::leap_distance` tiles over the ground, by where its target, a
/// living hostile, stands.
///
/// In its own reach, it leaps clear of the target: the front threat and
/// every threat within its span miss, and its swing clock starts over with
/// a swing due, held while it stands out of reach and joined by what
/// Patience banks behind it.
///
/// Out of its reach, it dives toward the target and stops beside it. A
/// dive that ends with the target in reach strikes it for
/// `Tuning::leap_strike` of its user's Intuition, and the held swing and
/// the bank land beside that at once (the gate swings for an actor the
/// frame its target is in reach). A dive that falls short leaps and
/// strikes nothing.
///
/// An NPC's leap stops at its leash (`Abilities::leash`). With nowhere to
/// leap it is out of range.
pub fn leap(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let distance = common_bevy::tuning::tuning().leap_distance;
    let (target, target_loc) = abilities.foe(cast)?;
    let clear = cast.loc.distance(&target_loc) <= cast.reach;
    let leash = abilities.leash(cast.ent);
    let landing = if clear {
        away(&abilities.map, *cast.loc, *target_loc, distance, leash)
    } else {
        toward(&abilities.map, *cast.loc, *target_loc, distance, leash)
    };
    let landing = landing.ok_or(AbilityFailReason::OutOfRange)?;
    slide(cast.ent, landing, LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);
    abilities.commands.entity(cast.ent).remove::<Traded>();
    if clear {
        abilities.clear(cast.ent, ClearType::Span(cast.attrs.span()));
        let now = abilities.time.elapsed();
        if let Ok(mut swing) = abilities.swings.get_mut(cast.ent) {
            swing.due = Some(now);
        }
    } else if Loc::new(landing).distance(&target_loc) <= cast.reach {
        let damage = cast.attrs.skill_potency(AbilityType::Leap) * common_bevy::tuning::tuning().leap_strike;
        abilities.strike(cast, target, damage, AbilityType::Leap, &WHOLE);
    }
    Ok(Some(target))
}
