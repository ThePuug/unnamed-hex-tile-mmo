//! Server-specific targeting systems

//! This module contains targeting system implementations that are specific to the server,
//! primarily distinguished by the use of TargetLock component filtering.

use bevy::prelude::*;

use common_bevy::{
    components::{behaviour::Side, heading::Heading, ActorAttributes, Loc, target::Target},
    plugins::nntree::NNTree,
    systems::targeting::{arc_of, update_targets_impl},
};
use crate::components::target_lock::TargetLock as NpcTargetLock;

/// Update hostile targets every frame for responsive targeting (SERVER VERSION)

/// Runs unconditionally to detect when target entities move out of range/cone.
/// Excludes NPCs with NpcTargetLock - behavior tree targeting is their source of truth.

/// # Server-Specific Behavior

/// The server version excludes entities with NpcTargetLock component from reactive targeting.
/// This is critical for AI behavior - NPCs with NpcTargetLock use behavior tree targeting
/// (FindOrKeepTarget) as their source of truth.

/// # Performance

/// Uses spatial index (NNTree) for fast proximity queries. Designed to run at 60fps.
/// If performance becomes an issue, can be changed to run on a timer (e.g., every 100ms).
pub fn update_targets(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut Target, Option<&ActorAttributes>),
        Without<NpcTargetLock>
    >,
    sides: Query<&Side>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target, attrs) in &mut query {
        update_targets_impl(
            ent,
            *loc,
            *heading,
            arc_of(attrs),
            &mut target,
            &nntree,
            |e| sides.get(e).ok().copied(),
        );
    }
}
