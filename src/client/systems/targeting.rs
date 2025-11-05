//! Client-specific targeting systems
//!
//! This module contains targeting system implementations that are specific to the client.
//! The main difference from the server is that the client doesn't filter by TargetLock
//! (since TargetLock is a server-only component).

use bevy::prelude::*;

use crate::common::{
    components::{heading::Heading, Loc, target::Target, targeting_state::TargetingState, entity_type::EntityType},
    plugins::nntree::NNTree,
    systems::targeting::update_targets_impl,
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
