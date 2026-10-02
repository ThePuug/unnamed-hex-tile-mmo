use std::time::Duration;

use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};

/// An auto-attack: a blow of the caster's auto damage on its target, free
/// and outside the recovery, due on its own clock (the gate's to check);
/// the next comes due an interval from now. Struck in a Perfect Stride it
/// lands `Tuning::stride_damage` harder.
pub fn swing(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let now = abilities.time.elapsed();
    let status = abilities.statuses.get(cast.ent).ok().copied();
    if let Ok(mut swing) = abilities.swings.get_mut(cast.ent) {
        swing.due = Some(now + cast.attrs.cadence_interval());
    }
    let stride = if status.is_some_and(|status| status.is_striding()) { 1.0 + common_bevy::tuning::tuning().stride_damage } else { 1.0 };
    abilities.deal(cast.ent, target, cast.attrs.auto_damage() * stride, AbilityType::AutoAttack, 0.0, Duration::ZERO);
    Ok(Some(target))
}
