use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};

/// A skill that is one strike on a target within the caster's reach, for
/// its share of base potency (`Tuning::damage`) raised by its line
/// (`ActorAttributes::line_power`), whole for a skill of no line:
///
/// - Frenzy, the Berserker's: a bite raised by its Might line. Its combo
///   is another bite, so Ferocity fires bites before they unlock and pays
///   less for the burst after (`combos::recovery_after`).
/// - Feint: a light strike, the same in any build, cheap and quickly
///   recovered from. Its combo is a Parry.
/// - Overpower, the Juggernaut's: one heavy blow raised by its Physique
///   line.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let (target, _) = cast.struck()?;
    let damage = cast.attrs.base_potency(&tuning) * tuning.damage(cast.ability) * cast.attrs.line_power(&tuning, cast.ability);
    abilities.strike(cast, target, damage, cast.ability);
    Ok(Some(target))
}
