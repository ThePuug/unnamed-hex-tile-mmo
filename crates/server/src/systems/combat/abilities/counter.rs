use bevy::prelude::*;
use common_bevy::{
    message::{AbilityType, ClearType, Event as GameEvent, Try},
    systems::combat::queue::create_threat,
};

use super::{Abilities, AbilityFailReason, Cast};

/// Counter: a reaction that clears the front threat and every threat
/// landing within its user's span behind it. Each cleared threat goes back to its living
/// source wherever it stands, at a share of the threat's own damage and
/// nothing more, so a Counter returns what comes in, and lands at once: a
/// reflection never enters the source's queue, so it cannot be countered.
/// The share is `Tuning::counter_reflect` weighted by the counterer's
/// Resolve: its Concentration over base potency. With nothing queued there
/// is nothing to counter. Its recovery is contested by the source of the
/// first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let answered = abilities.clear(cast.ent, ClearType::Span(cast.attrs.span()));
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
        // A reflection lands on impact and never waits its window out
        let reflected = create_threat(
            cast.ent,
            source_attrs,
            &cast.attrs,
            threat.damage * tuning.counter_reflect * weight,
            Some(AbilityType::Counter),
            now,
            0.0,
            0.0,
        );
        abilities.commands.trigger(Try { event: GameEvent::ResolveThreat { ent: threat.source, threat: reflected } });
    }

    Ok(answered.first().map(|threat| threat.source))
}
