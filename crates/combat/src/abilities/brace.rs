use bevy::prelude::*;
use common_bevy::{components::status::Timed, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast};
use crate::landing;

/// Brace, Physique's stance: for `Tuning::brace_secs` as its Physique line
/// has it (`ActorAttributes::line_power`), no hold, slow, root, pin or broken
/// stride a foe lays on its user takes, and no drag moves it
/// (`Status::is_braced`). It lifts none already on it.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let braced = Timed { pace: 1.0, remaining: tuning.brace_secs * cast.attrs.line_power(&tuning, AbilityType::Brace) };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.braced = Some(braced);
    });
    Ok(None)
}
