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

/// Reactive system that updates Target component when heading or location changes (SERVER VERSION)
///
/// This system runs whenever an entity's Heading or Loc changes, automatically
/// recalculating what entity they are facing using select_target().
///
/// Used by:
/// - Players: Target updates as they turn or move
/// - NPCs WITHOUT TargetLock: Target updates reactively based on FOV
///
/// NPCs with TargetLock are excluded - behavior tree targeting (FindOrKeepTarget)
/// is the source of truth for their targets, not reactive FOV targeting.
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
/// Only runs for entities that actually changed (Bevy change detection).
/// No work done if no entities moved or turned.
pub fn update_targets_on_change(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut Target, Option<&TierLock>),
        (Or<(Changed<Heading>, Changed<Loc>, Changed<TierLock>)>, Without<NpcTargetLock>)
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
