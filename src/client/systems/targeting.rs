//! Client-specific targeting systems
//!
//! This module contains targeting system implementations that are specific to the client.
//! The main difference from the server is that the client doesn't filter by TargetLock
//! (since TargetLock is a server-only component).

use bevy::prelude::*;

use crate::common::{
    components::{
        ally_target::AllyTarget,
        heading::Heading,
        Loc,
        target::Target,
        targeting_state::TargetingState,
        entity_type::EntityType
    },
    plugins::nntree::NNTree,
    systems::targeting::{update_targets_impl, select_ally_target},
};

/// Reactive system that updates Target component when heading or location changes (CLIENT VERSION)
///
/// This system runs whenever an entity's Heading or Loc changes, automatically
/// recalculating what entity they are facing using select_target().
///
/// # Client-Specific Behavior
///
/// The client version does not filter by TargetLock because TargetLock is a server-only component.
/// The client applies reactive targeting to all entities that have changed heading, location, or
/// targeting state.
///
/// # Performance
///
/// Only runs for entities that actually changed (Bevy change detection).
/// No work done if no entities moved or turned.
pub fn update_targets_on_change(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut Target, Option<&mut TargetingState>),
        Or<(Changed<Heading>, Changed<Loc>, Changed<TargetingState>)>
    >,
    entity_types: Query<&EntityType>,
    player_controlled: Query<&crate::common::components::behaviour::PlayerControlled>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target, mut targeting_state) in &mut query {
        update_targets_impl(
            ent,
            *loc,
            *heading,
            &mut target,
            targeting_state.as_deref_mut(),
            &nntree,
            &entity_types,
            &player_controlled,
        );
    }
}

/// Reactive system that updates AllyTarget component when heading or location changes (CLIENT VERSION)
///
/// This system runs whenever an entity's Heading or Loc changes, automatically
/// recalculating which ally entity they are facing using select_ally_target().
///
/// # Architecture
///
/// This system mirrors update_targets_on_change() but for ally targeting.
/// Only the targeting system calls select_ally_target() - UI systems should
/// read from the AllyTarget component, not call selection functions directly.
///
/// # Client-Specific Behavior
///
/// Only runs on the client. Ally targeting is client-only since it's purely
/// for UI/friendly ability targeting, not server game logic.
///
/// # Performance
///
/// Only runs for entities that actually changed (Bevy change detection).
/// No work done if no entities moved or turned.
pub fn update_ally_targets_on_change(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut AllyTarget, Option<&TargetingState>),
        Or<(Changed<Heading>, Changed<Loc>, Changed<TargetingState>)>
    >,
    player_controlled: Query<&crate::common::components::behaviour::PlayerControlled>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut ally_target, targeting_state) in &mut query {
        // Get tier lock from TargetingState if present
        let tier_lock = targeting_state.and_then(|ts| ts.get_tier_lock());

        // Use select_ally_target to find what ally this entity is facing (with tier lock filter)
        let new_ally_target = select_ally_target(
            ent,
            *loc,
            *heading,
            tier_lock,
            &nntree,
            |e| player_controlled.contains(e),
        );

        // Update AllyTarget based on result
        match new_ally_target {
            Some(target_ent) => {
                // Ally found - update both entity and last_target
                ally_target.set(target_ent);
            }
            None => {
                // No ally found - clear entity but leave last_target intact for sticky UI
                ally_target.clear();
            }
        }
    }
}
