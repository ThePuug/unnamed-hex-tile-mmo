use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};

/// Overpower, the Juggernaut's skill: one heavy blow on a target within the
/// caster's reach, for `Tuning::overpower_damage` of its Intuition.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.skill_potency(AbilityType::Overpower) * common_bevy::tuning::tuning().overpower_damage;
    abilities.strike(cast, target, damage, AbilityType::Overpower, &WHOLE);
    Ok(Some(target))
}
