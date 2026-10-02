use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};

/// Feint: a light strike on a target within the caster's reach, for
/// `Tuning::feint_damage` of its Intuition, cheap and quickly recovered
/// from. Its combo is a Parry.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.skill_potency(AbilityType::Feint) * common_bevy::tuning::tuning().feint_damage;
    abilities.strike(cast, target, damage, AbilityType::Feint, &WHOLE);
    Ok(Some(target))
}
