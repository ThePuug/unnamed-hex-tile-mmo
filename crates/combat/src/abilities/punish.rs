use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};
use common_bevy::tuning::Tuning;

/// Punish, the Ambusher's skill: a strike on a target within the caster's
/// reach, for `Tuning::punish_damage` of base potency raised by its
/// Discipline line (`ActorAttributes::line_power`), `punish_bonus` harder
/// on a target still in recovery from what it last used.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let recovering = abilities.recoveries.get(target).is_ok_and(|recovery| recovery.is_active());
    let damage = cast.attrs.base_potency(&tuning) * tuning.punish_damage * cast.attrs.line_power(&tuning, AbilityType::Punish) * weight(&tuning, recovering);
    abilities.strike(cast, target, damage, AbilityType::Punish, &WHOLE);
    Ok(Some(target))
}

/// What a Punish's damage is weighted by: `Tuning::punish_bonus` more on a
/// target `recovering`
pub(crate) fn weight(tuning: &Tuning, recovering: bool) -> f32 {
    if recovering { 1.0 + tuning.punish_bonus } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_punish_lands_harder_on_a_target_in_recovery() {
        let tuning = Tuning::DEFAULT;
        assert_eq!(weight(&tuning, false), 1.0);
        assert_eq!(weight(&tuning, true), 1.0 + tuning.punish_bonus);
    }
}
