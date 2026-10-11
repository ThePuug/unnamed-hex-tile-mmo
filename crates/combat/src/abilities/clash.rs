use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, Carried};

/// Clash, Might's reaction: clears the threats in its user's band
/// ([`Abilities::answer`]) and stuns each source it cleared for
/// `Tuning::clash_stun` seconds as its Might line has it
/// (`ActorAttributes::line_power`), the stun queued on that source
/// ([`Abilities::send_back`]). Its recovery is contested by the source of the
/// first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let answered = abilities.answer(cast);
    let stun = tuning.clash_stun * cast.attrs.line_power(&tuning, AbilityType::Clash);
    abilities.send_back(cast, &answered, |_| Carried { stun, ..Carried::default() });
    Ok(answered.first().map(|threat| threat.source))
}
