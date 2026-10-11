use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};
use crate::leap::{slide, toward, LEAP_MS};

/// Pounce, Instinct's reaction: clears the threats in its user's band
/// ([`Abilities::answer`]) and lunges toward the source of the first it
/// cleared, up to `Tuning::pounce_tiles` as its Instinct line has it
/// (`ActorAttributes::line_tiles`), stopping beside it, over the ground and
/// inside its leash. A pinned user answers where it stands. Its recovery
/// is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let answered = abilities.answer(cast);
    let Some(source) = answered.first().map(|threat| threat.source) else { return Ok(None) };
    let pinned = abilities.statuses.get(cast.ent).is_ok_and(|status| status.is_pinned());
    let Ok((&to, ..)) = abilities.actors.get(source) else { return Ok(Some(source)) };
    if pinned {
        return Ok(Some(source));
    }
    let tiles = cast.attrs.line_tiles(&tuning, AbilityType::Pounce, tuning.pounce_tiles);
    if let Some(landing) = toward(&abilities.map, *cast.loc, *to, tiles, abilities.leash(cast.ent)) {
        slide(cast.ent, landing, LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);
    }
    Ok(Some(source))
}
