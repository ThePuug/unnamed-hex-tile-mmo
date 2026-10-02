use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};

/// Parry, the Ambusher's skill: a reaction that clears the threats in its
/// user's span, as far as its endurance pays for them
/// ([`Abilities::react`]), and sends nothing back. Preparation lets parries
/// follow one another through a recovery (`combos::reacts_through`). Its
/// recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let cleared = abilities.react(cast)?;
    Ok(cleared.first().map(|threat| threat.source))
}
