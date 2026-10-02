use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};

/// Punish, the Ambusher's skill: a strike on a target within the caster's
/// reach, for `Tuning::punish_damage` of base potency raised by its
/// Discipline line (`ActorAttributes::line_power`), `punish_bonus` harder
/// on a target still in recovery from what it last used.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (target, _) = cast.struck()?;
    let recovering = abilities.recoveries.get(target).is_ok_and(|recovery| recovery.is_active());
    let damage = cast.attrs.base_potency() * tuning.punish_damage * cast.attrs.line_power(AbilityType::Punish) * weight(recovering);
    abilities.strike(cast, target, damage, AbilityType::Punish, &WHOLE);
    Ok(Some(target))
}

/// What a Punish's damage is weighted by: `Tuning::punish_bonus` more on a
/// target `recovering`
fn weight(recovering: bool) -> f32 {
    if recovering { 1.0 + common_bevy::tuning::tuning().punish_bonus } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_punish_lands_harder_on_a_target_in_recovery() {
        assert_eq!(weight(false), 1.0);
        assert_eq!(weight(true), 1.0 + common_bevy::tuning::tuning().punish_bonus);
    }
}
