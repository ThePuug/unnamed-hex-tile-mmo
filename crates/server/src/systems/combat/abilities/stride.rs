use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::landing;

/// Perfect Stride, the Kiter's skill: for `Tuning::stride_secs` its user
/// strikes past its forward faces, as far round as its Grace opens, without
/// breaking stride, and its auto-attacks land `Tuning::stride_damage`
/// harder (`auto_attack::swing`). It is a status (`Status::perfect_stride`), so every
/// client knows it as the server does.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let remaining = common_bevy::tuning::tuning().stride_secs;
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.perfect_stride = Some(remaining);
    });
    Ok(None)
}
