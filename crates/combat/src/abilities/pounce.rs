use common_bevy::message::AbilityType;

use super::{Abilities, Cast};
use crate::leap::{slide, toward, LEAP_MS};

/// Pounce, Instinct's defence: lunges toward its target, a living hostile,
/// up to `Tuning::pounce_tiles` as its line has it
/// (`ActorAttributes::line_tiles`), stopping beside it, over the ground
/// and inside its leash.
pub fn lunge(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let Ok((_, to)) = abilities.foe(cast) else { return };
    let tiles = cast.attrs.line_tiles(&tuning, AbilityType::Pounce, tuning.pounce_tiles);
    if let Some(landing) = toward(&abilities.map, *cast.loc, *to, tiles, abilities.leash(cast.ent)) {
        slide(cast.ent, landing, LEAP_MS, None, &mut abilities.commands, &mut abilities.writer);
    }
}
