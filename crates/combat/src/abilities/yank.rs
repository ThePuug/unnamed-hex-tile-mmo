use common_bevy::message::{AbilityType, Outcome};

use super::{Abilities, Cast};

/// Yank, Physique's defence: drags its target, a living hostile within its
/// range and its user's arc as its activation ends (`AbilityType::reach`), up to
/// `Tuning::yank_tiles` toward its user as its line has it
/// (`ActorAttributes::line_tiles`), ending what the target was activating
/// (`Abilities::drag`). No attack, so nothing blocks it.
pub fn pull(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let Ok((target, _)) = cast.struck() else { return };
    let tiles = cast.attrs.line_tiles(&tuning, AbilityType::Yank, tuning.yank_tiles).min(i8::MAX as usize) as i8;
    abilities.told(cast.ent, cast.ability, Some(target), 0.0, Outcome::Landed);
    abilities.drag(target, cast.loc, tiles);
}
