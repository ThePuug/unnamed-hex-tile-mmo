use bevy::prelude::*;
use common_bevy::message::ClearType;

use super::{Abilities, AbilityFailReason, Cast};

/// Parry, the Ambusher's skill: a reaction that clears the threats in its
/// user's span, the front one and those landing within the span behind it,
/// and pays endurance for the damage it turns aside
/// (`ActorAttributes::parry_effort`), where any other skill pays its own
/// flat cost. It takes them in the order a reaction does, the front first,
/// each whole while the endurance its user has left pays for it; the first
/// it cannot pay for ends the parry, and it and what stands behind it land.
/// It sends nothing back. With nothing queued, or not the endurance for the
/// front threat, there is nothing it can parry. Preparation lets parries
/// follow one another through a recovery (`combos::reacts_through`). Its
/// recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let had = abilities.endurance.get(cast.ent).map_or(0.0, |endurance| endurance.state);
    let mut left = had;
    let span = cast.attrs.span();
    let paid_for: Vec<_> = abilities.queues.get(cast.ent).map_or(Vec::new(), |queue| {
        queue.swept(span).take_while(|threat| {
            let effort = cast.attrs.parry_effort(threat.damage);
            let pays = effort <= left;
            if pays {
                left -= effort;
            }
            pays
        }).copied().collect()
    });
    let first = paid_for.first().ok_or(AbilityFailReason::NoTargets)?.source;
    for threat in paid_for {
        abilities.clear(cast.ent, ClearType::Threat { source: threat.source, inserted_at: threat.inserted_at });
    }
    abilities.tire(cast.ent, had - left);
    Ok(Some(first))
}
