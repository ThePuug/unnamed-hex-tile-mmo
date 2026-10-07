use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast, WHOLE};
use common_bevy::tuning::Tuning;

/// Punish, the Ambusher's skill: a strike on a target within the caster's
/// reach, for `Tuning::punish_damage` of base potency raised by its
/// Instinct line (`ActorAttributes::line_power`), weighted by the stacks of
/// Overcommitted its target carries ([`weight`]).
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let stacks = abilities.statuses.get(target).map_or(0, |status| status.overcommits());
    let damage = cast.attrs.base_potency(&tuning) * tuning.punish_damage * cast.attrs.line_power(&tuning, AbilityType::Punish) * weight(&tuning, stacks);
    abilities.strike(cast, target, damage, AbilityType::Punish, &WHOLE);
    Ok(Some(target))
}

/// What a Punish's damage is weighted by on a target carrying `stacks` of
/// Overcommitted: half with none, `Tuning::punish_per_stack` more for each
pub(crate) fn weight(tuning: &Tuning, stacks: usize) -> f32 {
    0.5 + tuning.punish_per_stack * stacks as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_punish_lands_at_half_with_no_stacks_and_harder_for_each() {
        let tuning = Tuning::DEFAULT;
        assert_eq!(weight(&tuning, 0), 0.5);
        assert!(weight(&tuning, 3) > weight(&tuning, 2) && weight(&tuning, 2) > weight(&tuning, 0), "the more stacks, the harder");
        assert!((weight(&tuning, 3) - 1.0).abs() < 0.01, "at par from three");
    }
}
