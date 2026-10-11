use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Carried, Cast};

/// Kick, Discipline's strike: a blow on a target within the caster's reach
/// for `Tuning::kick_damage` of base potency in any build, that knocks it
/// back as it lands, up to `Tuning::kick_tiles` along the line from the
/// caster as its Discipline line has it (`ActorAttributes::line_tiles`),
/// over the ground and inside the target's leash (`resolve_drag`).
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.base_potency(&tuning) * tuning.damage(AbilityType::Kick);
    let tiles = cast.attrs.line_tiles(&tuning, AbilityType::Kick, tuning.kick_tiles).min(i8::MAX as usize) as i8;
    abilities.carry(cast, target, Carried { damage, drag: -tiles, ..Carried::default() });
    Ok(Some(target))
}
