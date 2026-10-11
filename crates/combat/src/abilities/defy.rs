use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};

/// Defy, Resolve's strike: a blow on a target within the caster's reach for
/// `Tuning::defy_damage` of base potency, `Tuning::defy_per_threat` more for
/// each threat waiting in the caster's own queue as its Resolve line has it
/// (`ActorAttributes::line_power`): struck back while the blows still come.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let waiting = abilities.queues.get(cast.ent).map_or(0, |queue| queue.threats.len());
    let per_threat = tuning.defy_per_threat * cast.attrs.line_power(&tuning, AbilityType::Defy);
    let damage = cast.attrs.base_potency(&tuning) * tuning.damage(AbilityType::Defy) * (1.0 + per_threat * waiting as f32);
    abilities.strike(cast, target, damage, AbilityType::Defy);
    Ok(Some(target))
}
