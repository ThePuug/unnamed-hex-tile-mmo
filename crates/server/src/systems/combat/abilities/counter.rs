use bevy::prelude::*;
use common_bevy::{
    message::{AbilityType, Event as GameEvent, Try},
    systems::combat::queue::create_threat,
};

use super::{Abilities, AbilityFailReason, Cast};

/// Counter: a reaction that clears as many threats from the front of the
/// queue as the window holds. Each cleared threat goes back to its living
/// source wherever it stands, at a share of the threat's own damage and
/// nothing more, so a Counter returns what comes in, and lands at once: a
/// reflection never enters the source's queue, so it cannot be countered.
/// The share is `Tuning::counter_reflect` weighted by the counterer's
/// Resolve: its Concentration over base potency. With nothing queued there
/// is nothing to counter. Its lockout is contested by the source of the
/// first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let answered = abilities.window(cast.ent);
    if answered.is_empty() {
        return Err(AbilityFailReason::NoTargets);
    }

    let now = abilities.game_now();
    let weight = cast.attrs.concentration() / cast.attrs.base_potency();
    for threat in &answered {
        // A reflection needs a living source to go back to
        let Ok((_, source_attrs, _, _, _, _, dead)) = abilities.actors.get(threat.source) else { continue };
        if dead {
            continue;
        }
        // The same window every threat between the two takes (INV-003),
        // though a reflection lands on impact and never waits it out
        let reflected = create_threat(
            cast.ent,
            source_attrs,
            &cast.attrs,
            threat.damage * tuning.counter_reflect * weight,
            Some(AbilityType::Counter),
            now,
            0.0,
        );
        abilities.commands.trigger(Try { event: GameEvent::ResolveThreat { ent: threat.source, threat: reflected } });
    }

    abilities.clear_front(cast.ent, answered.len());
    Ok(answered.first().map(|threat| threat.source))
}
