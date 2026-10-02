use bevy::prelude::*;
use common_bevy::{
    message::{AbilityType, Event as GameEvent, Try},
    systems::combat::queue::create_threat,
};

use super::{Abilities, AbilityFailReason, Cast};

/// Counter, the Defender's skill: a reaction that clears the threats in its
/// user's span, as far as its endurance pays for them ([`Abilities::react`]),
/// so Awareness answers more with one. Each cleared threat goes back to its living
/// source wherever it stands, at `Tuning::counter_reflect` of the threat's
/// own damage weighted by the counterer's Concentration over base potency
/// and nothing more, so a Counter returns what comes in, and lands at
/// once: a reflection never enters the source's queue, so it
/// cannot be countered. Its recovery is contested by the source of the
/// first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let answered = abilities.react(cast)?;

    let now = abilities.game_now();
    let weight = cast.attrs.skill_potency(AbilityType::Counter) / cast.attrs.base_potency();
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
