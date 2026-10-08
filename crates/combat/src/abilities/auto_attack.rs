use std::time::Duration;

use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};

/// An auto-attack: a blow of the caster's auto damage on its target, free
/// and outside the recovery, due on its own clock (the gate's to check);
/// the next comes due an interval from now, by its Tempo over its target's
/// Reflex.
pub fn swing(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let now = abilities.time.elapsed();
    if let Ok(mut swing) = abilities.swings.get_mut(cast.ent) {
        let foe = abilities.actors.get(target).ok().map(|(_, attrs, ..)| *attrs);
        swing.due = Some(now + cast.attrs.cadence_interval(&tuning, foe.as_ref()));
    }
    abilities.deal(cast.ent, target, cast.attrs.auto_damage(&tuning), AbilityType::AutoAttack, 0.0, Duration::ZERO);
    Ok(Some(target))
}
