use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};

/// Frenzy, the Berserker's skill: a bite on a target within the caster's
/// reach, for `Tuning::frenzy_damage` of its Intuition. Its combo is another
/// bite, so Ferocity fires bites before they unlock and pays less for the
/// burst after (`combos::recovery_after`).
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.skill_potency(AbilityType::Frenzy) * common_bevy::tuning::tuning().frenzy_damage;
    abilities.strike(cast, target, damage, AbilityType::Frenzy, &WHOLE);
    Ok(Some(target))
}
