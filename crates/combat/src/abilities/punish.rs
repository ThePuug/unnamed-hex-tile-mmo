use bevy::prelude::*;
use common_bevy::{components::ActorAttributes, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast, WHOLE};
use common_bevy::tuning::Tuning;

/// Punish, the Ambusher's skill: a strike on a target within the caster's
/// reach, for [`share`] of base potency.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let stacks = abilities.statuses.get(target).map_or(0, |status| status.overcommits());
    let damage = cast.attrs.base_potency(&tuning) * share(&tuning, &cast.attrs, stacks);
    abilities.strike(cast, target, damage, AbilityType::Punish, &WHOLE);
    Ok(Some(target))
}

/// The share of base potency a Punish by `attrs` strikes for on a target
/// carrying `stacks` of Overcommitted: `Tuning::punish_base` in any build,
/// and `Tuning::punish_per_stack` more for each stack, raised by its
/// Instinct line (`ActorAttributes::line_power`), the only part the line
/// raises
pub(crate) fn share(tuning: &Tuning, attrs: &ActorAttributes, stacks: usize) -> f32 {
    tuning.punish_base + tuning.punish_per_stack * attrs.line_power(tuning, AbilityType::Punish) * stacks as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_punish_strikes_its_base_in_any_build_and_its_line_raises_only_the_stacks() {
        let tuning = Tuning::DEFAULT;
        let patient = ActorAttributes::new(0, 0, 0, 0, 0, 0, -25, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(share(&tuning, &patient, 0), share(&tuning, &plain, 0), "with no stacks, the same in any build");
        assert_eq!(share(&tuning, &plain, 0), tuning.punish_base);
        let step = |attrs: &ActorAttributes, stacks| share(&tuning, attrs, stacks + 1) - share(&tuning, attrs, stacks);
        assert!((step(&patient, 2) - step(&patient, 0)).abs() < 1e-5, "each stack adds the same");
        assert!(step(&patient, 0) > step(&plain, 0), "Instinct raises what each adds");
        assert!(step(&plain, 0) > 0.0, "and a stack adds something in any build");
    }
}
