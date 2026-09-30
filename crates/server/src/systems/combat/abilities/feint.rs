use std::time::Duration;

use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};

/// Feint, the Juggernaut's skill: two strikes on a target within the
/// caster's reach, queued together. The feint is made at once for
/// `Tuning::feint_share` of `feint_damage` of its Intuition; the real strike
/// follows `feint_gap` behind it for the rest, its window starting then, so
/// it lands that much later. A reaction that reaches no further than the
/// gap takes the feint and leaves the real strike. What the caster's Grit
/// banked is split between the two as the damage is.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.skill_potency(AbilityType::Feint) * tuning.feint_damage;
    let parts = [
        (tuning.feint_share, Duration::ZERO),
        (1.0 - tuning.feint_share, Duration::from_secs_f32(tuning.feint_gap)),
    ];
    abilities.strike(cast, target, damage, AbilityType::Feint, &parts);
    Ok(Some(target))
}
