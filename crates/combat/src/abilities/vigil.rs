use bevy::prelude::*;
use common_bevy::{components::status::Timed, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast};
use crate::landing;

/// Vigil, Resolve's stance: for `Tuning::vigil_secs` as its Resolve line has
/// it (`ActorAttributes::line_power`), no strike takes its user's flank
/// (`targeting::takes_flank`): a blow from past its forward faces lands as
/// one from in front, breaks no stride, and no Backstab may be struck.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let vigil = Timed { pace: 1.0, remaining: tuning.vigil_secs * cast.attrs.line_power(&tuning, AbilityType::Vigil) };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.vigil = Some(vigil);
    });
    Ok(None)
}
