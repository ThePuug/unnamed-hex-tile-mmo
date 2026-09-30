use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::{landing, leap};

/// Seconds a Kick holds what it drives back: long enough that it does not
/// walk straight in again.
const STAGGER_SECS: f32 = 0.5;

/// Kick (R key): a reaction that clears as many threats from the front of
/// the queue as the window holds. Each living source of one that stands
/// beside the kicker takes a blow of three quarters of the kicker's
/// Tempo, is driven four tiles away over the ground (`leap::away`),
/// further by the kicker's hold (`ActorAttributes::hold`), and is held
/// there a moment. With nothing queued there is nothing to kick. Its
/// recovery is contested by the source of the first threat it answers.
pub fn answer(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let answered = abilities.window(cast.ent);
    if answered.is_empty() {
        return Err(AbilityFailReason::NoTargets);
    }

    let tiles = (4.0 * cast.attrs.hold()).round() as usize;
    for threat in &answered {
        let Ok((&source_loc, _, _, _, _, _, dead)) = abilities.actors.get(threat.source) else { continue };
        if dead || cast.loc.flat_distance(&source_loc) != 1 {
            continue;
        }
        abilities.deal(cast.ent, threat.source, cast.attrs.tempo() * 0.75, AbilityType::Kick, 0.0);
        // Driven back over the ground, away from the kicker, and held there
        // a moment; with nowhere to go it is only held
        if let Some(landing) = leap::away(&abilities.map, *source_loc, *cast.loc, tiles) {
            let pushed = landing.flat_distance(&source_loc) as u16;
            leap::slide(threat.source, landing, pushed * 125, None, &mut abilities.commands, &mut abilities.writer);
        }
        landing::update(threat.source, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| status.hold(STAGGER_SECS));
    }

    abilities.clear_front(cast.ent, answered.len());
    Ok(answered.first().map(|threat| threat.source))
}
