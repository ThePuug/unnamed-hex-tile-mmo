use bevy::prelude::*;
use common_bevy::{
    components::status::Status,
    message::AbilityType,
};

use super::{Abilities, AbilityFailReason, Cast};

/// Rattle, the Juggernaut's signature: a strike on a target within melee
/// reach that, as it lands (`landing::land`), adds a stack to its daze
/// (`Status::daze`), up to `rattle_stacks`.
/// Each stack takes `rattle_daze` of its pace, its movement, auto-attacks
/// and recovery alike. The strike is Vitality's: `rattle_health` of the
/// Juggernaut's own health, and `rattle_growth` more for each stack already
/// on the target, so the longer a fight runs the harder a Juggernaut hits
/// and the less the target escapes or presses it.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (target, _) = cast.struck()?;
    let held = Status::stacks_of(abilities.statuses.get(target).ok());
    let damage = cast.health_max * tuning.rattle_health * (1.0 + tuning.rattle_growth * held as f32);
    abilities.strike(cast, target, damage, AbilityType::Rattle);
    Ok(Some(target))
}
