use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};
use crate::leap::{round, slide, LEAP_MS};

/// Sidestep, Agility's reaction: clears the threats in its user's band
/// ([`Abilities::answer`]) and steps round the source of the first it
/// cleared, on that one's ring, toward its back, up to
/// `Tuning::sidestep_steps` as its Agility line has it
/// (`ActorAttributes::line_tiles`), over the ground and inside its leash. A
/// pinned user, or one whose source faces nowhere, answers where it stands.
/// Its recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let answered = abilities.answer(cast);
    let Some(source) = answered.first().map(|threat| threat.source) else { return Ok(None) };
    let pinned = abilities.statuses.get(cast.ent).is_ok_and(|status| status.is_pinned());
    let Ok((&center, _, _, heading, ..)) = abilities.actors.get(source) else { return Ok(Some(source)) };
    let (Some(&heading), false) = (heading, pinned) else { return Ok(Some(source)) };
    let radius = cast.loc.flat_distance(&center).max(1);
    let back = *center + heading.reversed().hex_dir() * radius;
    let steps = cast.attrs.line_tiles(&tuning, AbilityType::Sidestep, tuning.sidestep_steps);
    if let Some(landing) = round(&abilities.map, *cast.loc, *center, back, steps, abilities.leash(cast.ent)) {
        slide(cast.ent, landing, LEAP_MS, Some(*center), &mut abilities.commands, &mut abilities.writer);
    }
    Ok(Some(source))
}
