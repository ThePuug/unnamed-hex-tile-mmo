use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};

/// Overpower, the Juggernaut's skill: one heavy blow on a target within the
/// caster's reach, for `Tuning::overpower_damage` of base potency, raised
/// by its Vitality line (`ActorAttributes::line_power`).
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.base_potency() * common_bevy::tuning::tuning().overpower_damage * cast.attrs.line_power(AbilityType::Overpower);
    abilities.strike(cast, target, damage, AbilityType::Overpower, &WHOLE);
    Ok(Some(target))
}
