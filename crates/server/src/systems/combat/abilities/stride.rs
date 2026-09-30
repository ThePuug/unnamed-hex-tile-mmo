use std::time::Duration;

use bevy::prelude::*;

use super::{Abilities, AbilityFailReason, Cast};

/// A Perfect Stride under way, until the server's clock reads `until`:
/// its holder's strikes across its own line break no stride
/// (`Abilities::strides`). One outlasting its time is only stale.
#[derive(Clone, Component, Copy, Debug)]
pub struct PerfectStride {
    pub until: Duration,
}

/// Perfect Stride, the Kiter's skill: for `Tuning::stride_secs` its user
/// strikes past its forward faces, as far round as its Grace opens, without
/// breaking stride, so it keeps its full pace while it strikes on the run.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let until = abilities.time.elapsed() + Duration::from_secs_f32(common_bevy::tuning::tuning().stride_secs);
    abilities.commands.entity(cast.ent).insert(PerfectStride { until });
    Ok(None)
}
