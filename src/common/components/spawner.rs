use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::entity_type::actor::*;

/// Component for spawner entities that spawn NPCs
#[derive(Component, Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Spawner {
    pub npc_template: NpcTemplate,
    pub max_count: u8,
    pub spawn_radius: u8,
    pub player_activation_range: u8,
    pub leash_distance: u8,  // How far NPCs can wander from spawner before being pulled back
    pub despawn_distance: u8, // Despawn NPCs when all players are beyond this distance
    pub respawn_timer_ms: u32,
    pub last_spawn_attempt: u128,
}

impl Spawner {
    pub fn new(
        npc_template: NpcTemplate,
        max_count: u8,
        spawn_radius: u8,
        player_activation_range: u8,
        leash_distance: u8,
        despawn_distance: u8,
        respawn_timer_ms: u32,
    ) -> Self {
        Self {
            npc_template,
            max_count,
            spawn_radius,
            player_activation_range,
            leash_distance,
            despawn_distance,
            respawn_timer_ms,
            last_spawn_attempt: 0,
        }
    }
}

/// NPC templates defining what type of creatures can be spawned
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NpcTemplate {
    Dog,
    Wolf,
    Rabbit,
}

impl NpcTemplate {
    /// Get the ActorImpl for this NPC type
    pub fn actor_impl(&self) -> ActorImpl {
        match self {
            Self::Dog => ActorImpl::new(Origin::Natureborn, Approach::Direct, Resilience::Primal),
            Self::Wolf => ActorImpl::new(Origin::Natureborn, Approach::Ambushing, Resilience::Vital),
            Self::Rabbit => ActorImpl::new(Origin::Natureborn, Approach::Evasive, Resilience::Primal),
        }
    }
}
