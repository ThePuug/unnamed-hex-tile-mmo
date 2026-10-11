use bevy::prelude::*;
use common_bevy::{components::status::Timed, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast};
use crate::landing;

/// Lurk, Instinct's stance: for `Tuning::lurk_secs` as its Instinct line has
/// it (`ActorAttributes::line_power`), no foe beyond its user's reach sees
/// it (`targeting::unseen`): none targets it, acquires it or keeps it as a
/// target, and none leaps onto it. Its first attack ends it
/// (`process_deal_damage`).
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let lurk = Timed { pace: 1.0, remaining: tuning.lurk_secs * cast.attrs.line_power(&tuning, AbilityType::Lurk) };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.lurk = Some(lurk);
    });
    Ok(None)
}
