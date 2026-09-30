use bevy::prelude::*;
use common_bevy::message::AbilityType;

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::landing::VolleyBurst;

/// Volley, the Kiter's signature: a burst of `Tuning::volley_shots` shots
/// at a target within the Kiter's reach, each striking for
/// `Tuning::volley_precision` of Precision. Every shot is its own threat.
/// Each shot, as it lands (`landing::land`), slows its target by
/// `volley_slow` for `volley_slow_secs`, and the first of the burst to land
/// leaps the Kiter `volley_leap` tiles straight away from it: the gap opens
/// only when the target can no longer close it.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (target, _) = cast.struck()?;
    // Its shots are queued now, the time a landing knows the burst by
    let at = abilities.game_now();
    abilities.commands.entity(cast.ent).insert(VolleyBurst { at, leapt: false });
    for _ in 0..tuning.volley_shots {
        abilities.deal(cast.ent, target, cast.attrs.precision() * tuning.volley_precision, AbilityType::Volley, 0.0);
    }
    Ok(Some(target))
}
