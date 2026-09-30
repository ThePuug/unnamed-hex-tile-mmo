use bevy::prelude::*;
use common_bevy::{
    message::AbilityType,
    systems::combat::resources::LUNGE_RANGE,
};

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::leap;

/// Lunge (Q key): a charge over the ground to beside a target up to
/// `LUNGE_RANGE` away (`leap::toward`), a strike for `Tuning::lunge_force`
/// of Force, and a wound whose DoT, a bleed, deals `Tuning::lunge_dot` of
/// Force each tick until a reaction clears the wound or it lands. With no
/// way over the ground to beside its target the target is out of range;
/// already beside it, the Lunge strikes from where it stands.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (target, target_loc) = cast.struck()?;
    let landing = leap::toward(&abilities.map, *cast.loc, *target_loc, LUNGE_RANGE as usize);
    if landing.unwrap_or(*cast.loc).flat_distance(&target_loc) > 1 {
        return Err(AbilityFailReason::OutOfRange);
    }
    // The charge: a fast dash along that way
    if let Some(landing) = landing {
        let charge_ms = (cast.loc.flat_distance(&target_loc) as u16 * 50).max(100);
        leap::slide(cast.ent, landing, charge_ms, None, &mut abilities.commands, &mut abilities.writer);
    }
    let force = cast.attrs.force();
    abilities.strike(cast, target, force * tuning.lunge_force, AbilityType::Lunge);
    abilities.deal(cast.ent, target, 0.0, AbilityType::Lunge, force * tuning.lunge_dot);
    Ok(Some(target))
}
