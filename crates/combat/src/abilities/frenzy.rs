use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};

/// Frenzy, the Berserker's skill: a bite on a target within the caster's
/// reach, for `Tuning::frenzy_damage` of base potency, raised by its Might
/// line (`ActorAttributes::line_power`). Its combo is another
/// bite, so Ferocity fires bites before they unlock and pays less for the
/// burst after (`combos::recovery_after`).
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.base_potency() * common_bevy::tuning::tuning().frenzy_damage * cast.attrs.line_power(AbilityType::Frenzy);
    abilities.strike(cast, target, damage, AbilityType::Frenzy, &WHOLE);
    Ok(Some(target))
}
