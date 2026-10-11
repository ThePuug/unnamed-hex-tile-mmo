use bevy::prelude::*;
use common_bevy::{components::status::Timed, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast};
use crate::landing;

/// Bloodlust, Might's stance: for `Tuning::bloodlust_secs`,
/// `Tuning::bloodlust_share` of the damage its user's attacks land comes back
/// to it as health, as its Might line has it (`ActorAttributes::line_power`).
/// It is a status (`Status::bloodlust`), read as each attack lands
/// (`resolve_threat`).
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let bloodlust = Timed { pace: tuning.bloodlust_share * cast.attrs.line_power(&tuning, AbilityType::Bloodlust), remaining: tuning.bloodlust_secs };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.bloodlust = Some(bloodlust);
    });
    Ok(None)
}
