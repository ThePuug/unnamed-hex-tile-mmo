use bevy::prelude::*;
use common_bevy::{components::status::Timed, message::AbilityType};

use super::{Abilities, AbilityFailReason, Cast};
use crate::landing;

/// Perfect Stride, the Kiter's skill: for `Tuning::stride_secs` its user
/// strikes past its forward faces, as far round as its Grace opens, without
/// breaking stride, runs `Tuning::stride_speed` faster as its Agility line
/// has it (`ActorAttributes::line_power`), and its auto-attacks land
/// `Tuning::stride_damage` harder (`auto_attack::swing`). It is a status
/// (`Status::perfect_stride`), so every client moves its user at the pace
/// the server does.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let stride = Timed { pace: 1.0 + tuning.stride_speed * cast.attrs.line_power(&tuning, AbilityType::PerfectStride), remaining: tuning.stride_secs };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.perfect_stride = Some(stride);
    });
    Ok(None)
}
