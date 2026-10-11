use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, Carried};

/// Yank, Physique's reaction: clears the threats in its user's band
/// ([`Abilities::answer`]) and drags each source it cleared up to
/// `Tuning::yank_tiles` toward its user as its Physique line has it
/// (`ActorAttributes::line_tiles`), the pull queued on that source
/// ([`Abilities::send_back`]) and walked as it lands (`resolve_drag`). Its
/// recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let answered = abilities.answer(cast);
    let drag = cast.attrs.line_tiles(&tuning, AbilityType::Yank, tuning.yank_tiles).min(i8::MAX as usize) as i8;
    abilities.send_back(cast, &answered, |_| Carried { drag, ..Carried::default() });
    Ok(answered.first().map(|threat| threat.source))
}
