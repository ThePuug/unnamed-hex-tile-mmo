use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, Carried};

/// Counter, the Defender's skill: a reaction that clears the threats in its
/// user's band ([`Abilities::answer`]), which Awareness widens. It returns one
/// blow to each source it cleared ([`Abilities::send_back`]),
/// `Tuning::counter_reflect` of all it cleared from that source, raised by
/// the counterer's Resolve line (`ActorAttributes::line_power`). Its
/// recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let answered = abilities.answer(cast);
    let share = tuning.counter_reflect * cast.attrs.line_power(&tuning, AbilityType::Counter);
    abilities.send_back(cast, &answered, |sum| Carried { damage: sum * share, ..Carried::default() });
    Ok(answered.first().map(|threat| threat.source))
}
