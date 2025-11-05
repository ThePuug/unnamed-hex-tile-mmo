//! Server-specific targeting systems
//!
//! This module contains targeting system implementations that are specific to the server,
//! primarily distinguished by the use of TargetLock component filtering.

use bevy::prelude::*;

use crate::common::{
    components::{heading::Heading, Loc, target::Target, tier_lock::TierLock, entity_type::EntityType},
    plugins::nntree::NNTree,
    systems::targeting::update_targets_impl,
};
use crate::server::components::target_lock::TargetLock as NpcTargetLock;

/// Update hostile targets every frame for responsive targeting (SERVER VERSION)
///
/// Runs unconditionally to detect when target entities move out of range/cone.
/// Excludes NPCs with NpcTargetLock - behavior tree targeting is their source of truth.
///
/// # Server-Specific Behavior
///
/// The server version excludes entities with NpcTargetLock component from reactive targeting.
/// This is critical for AI behavior - NPCs with NpcTargetLock use behavior tree targeting
/// (FindOrKeepTarget) as their source of truth.
///
/// Players have TierLock (for tier lock targeting), which is different from NpcTargetLock.
///
/// # Performance
///
/// Uses spatial index (NNTree) for fast proximity queries. Designed to run at 60fps.
/// If performance becomes an issue, can be changed to run on a timer (e.g., every 100ms).
pub fn update_targets(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut Target, Option<&TierLock>),
        Without<NpcTargetLock>
    >,
    entity_types: Query<&EntityType>,
    player_controlled: Query<&crate::common::components::behaviour::PlayerControlled>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target, tier_lock) in &mut query {
        update_targets_impl(
            ent,
            *loc,
            *heading,
            &mut target,
            tier_lock,
            &nntree,
            &entity_types,
            &player_controlled,
        );
    }
}
