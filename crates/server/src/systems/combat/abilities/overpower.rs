use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};

/// Overpower (W key): a heavy blow on a target beside the caster, for one
/// and a half times its Force.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    abilities.strike(cast, target, cast.attrs.force() * 1.5, AbilityType::Overpower);
    Ok(Some(target))
}
