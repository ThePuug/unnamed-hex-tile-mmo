use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};

/// Parry, every fighter's: a reaction that clears the threats in its
/// user's band ([`Abilities::answer`]), and sends nothing back.
/// Preparation fires it early after a strike (`combos::may_use`). Its
/// recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let cleared = abilities.answer(cast);
    Ok(cleared.first().map(|threat| threat.source))
}
