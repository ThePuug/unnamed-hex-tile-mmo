use common_bevy::message::AbilityType;

use super::{Abilities, Cast};
use crate::leap::{round, slide, LEAP_MS};

/// Sidestep, Agility's defence: steps round its target, a living hostile,
/// on that one's ring, toward its back, up to `Tuning::sidestep_steps` as
/// its line has it (`ActorAttributes::line_tiles`), over the ground and
/// inside its leash: out of the arc a strike at it was aimed along. Round
/// a target that faces nowhere it stands where it is.
pub fn step(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let Ok((target, center)) = abilities.foe(cast) else { return };
    let Some(heading) = abilities.actors.get(target).ok().and_then(|(_, _, heading, ..)| heading.copied()) else { return };
    let radius = cast.loc.flat_distance(&center).max(1);
    let back = *center + heading.reversed().hex_dir() * radius;
    let steps = cast.attrs.line_tiles(&tuning, AbilityType::Sidestep, tuning.sidestep_steps);
    if let Some(landing) = round(&abilities.map, *cast.loc, *center, back, steps, abilities.leash(cast.ent)) {
        slide(cast.ent, landing, LEAP_MS, Some(*center), &mut abilities.commands, &mut abilities.writer);
    }
}
