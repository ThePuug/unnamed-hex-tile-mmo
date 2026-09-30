use bevy::prelude::*;
use common_bevy::message::{AbilityType, ClearType};

use super::{Abilities, AbilityFailReason, Cast};

/// Parry, the Ambusher's skill: a reaction that clears the threats in its
/// user's span, the front one and those landing within the span behind it,
/// up to a budget of damage: `Tuning::parry_capacity` of its Concentration.
/// It takes them in the order a reaction does, the front first, each whole
/// while what is left of the budget covers it; the first it cannot cover
/// ends the parry, and it and what stands behind it land. It sends nothing
/// back. With nothing queued, or a front threat past the whole budget,
/// there is nothing it can parry. Preparation lets parries follow one
/// another through a recovery (`combos::reacts_through`). Its recovery is
/// contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let mut budget = cast.attrs.skill_potency(AbilityType::Parry) * common_bevy::tuning::tuning().parry_capacity;
    let span = cast.attrs.span();
    let covered: Vec<_> = abilities.queues.get(cast.ent).map_or(Vec::new(), |queue| {
        queue.swept(span).take_while(|threat| {
            budget -= threat.damage;
            budget >= 0.0
        }).copied().collect()
    });
    let first = covered.first().ok_or(AbilityFailReason::NoTargets)?.source;
    for threat in covered {
        abilities.clear(cast.ent, ClearType::Threat { source: threat.source, inserted_at: threat.inserted_at });
    }
    Ok(Some(first))
}
