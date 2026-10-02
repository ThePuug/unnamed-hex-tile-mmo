use std::time::Duration;

use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};

/// An auto-attack: a blow of the caster's auto damage on its target, free
/// and outside the recovery, due on its own clock (the gate's to check).
/// The swings that came due behind it while it waited, up to its Patience,
/// land with this one, each at `Tuning::patience_power` of it, and the next
/// comes due an interval from now. Struck in a Perfect Stride it lands
/// `Tuning::stride_damage` harder.
pub fn swing(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let now = abilities.time.elapsed();
    let status = abilities.statuses.get(cast.ent).ok().copied();
    let interval = cast.attrs.cadence_interval();
    let banked = abilities.swings.get_mut(cast.ent).map_or(0, |mut swing| {
        let banked = swing.waited(now).map_or(0, |waited| cast.attrs.banked(waited, interval));
        swing.due = Some(now + interval);
        banked
    });
    let tuning = common_bevy::tuning::tuning();
    let stride = if status.is_some_and(|status| status.is_striding()) { 1.0 + tuning.stride_damage } else { 1.0 };
    let weight = (1.0 + banked as f32 * tuning.patience_power) * stride;
    abilities.deal(cast.ent, target, cast.attrs.auto_damage() * weight, AbilityType::AutoAttack, 0.0, Duration::ZERO);
    Ok(Some(target))
}
