use bevy::prelude::*;
use common_bevy::message::ClearType;

use super::{Abilities, AbilityFailReason, Cast};

/// Parry, the Ambusher's skill: a reaction that clears the front threat and
/// every threat landing within its user's span behind it, and sends nothing
/// back. With nothing queued there is nothing to parry. Preparation lets
/// parries follow one another through a recovery
/// (`combos::reacts_through`). Its recovery is contested by the source of
/// the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let answered = abilities.clear(cast.ent, ClearType::Span(cast.attrs.span()));
    let first = answered.first().ok_or(AbilityFailReason::NoTargets)?;
    Ok(Some(first.source))
}
