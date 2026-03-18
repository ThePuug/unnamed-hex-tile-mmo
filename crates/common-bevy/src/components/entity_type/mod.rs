pub mod actor;
pub mod decorator;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::entity_type::{
    actor::*,
    decorator::*,
};
use crate::spatial_difficulty::EnemyArchetype;

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum EntityType {
    #[default] Unset,
    Actor(ActorImpl),
    Decorator(Decorator),
    /// Terrain-derived NPC spawner. Unmeshed, non-solid. Placed during chunk
    /// generation based on terrain tags. Activates when a player approaches.
    Spawner { archetype: EnemyArchetype },
}

impl EntityType {
    /// Get human-readable display name for this entity type
    /// Returns a name suitable for showing to players in UI
    pub fn display_name(&self) -> &'static str {
        match self {
            EntityType::Unset => "Unknown",
            EntityType::Actor(actor_impl) => actor_impl.identity.display_name(),
            EntityType::Decorator(_) => "Object",
            EntityType::Spawner { .. } => "Spawner",
        }
    }
}