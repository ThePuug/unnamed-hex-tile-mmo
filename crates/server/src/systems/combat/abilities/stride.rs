use bevy::prelude::*;
use common_bevy::components::status::Timed;

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::landing;

/// Perfect Stride, the Kiter's skill: for `Tuning::stride_secs` its user
/// strikes past its forward faces, as far round as its Grace opens, without
/// breaking stride, and runs `Tuning::stride_speed` faster, so it gains
/// ground while it strikes on the run. It is a status
/// (`Status::perfect_stride`), so every client moves its user at the pace
/// the server does.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let stride = Timed { pace: 1.0 + tuning.stride_speed, remaining: tuning.stride_secs };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.perfect_stride = Some(stride);
    });
    Ok(None)
}
