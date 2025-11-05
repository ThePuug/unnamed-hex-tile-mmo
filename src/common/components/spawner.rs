use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use rand::Rng;

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
    ForestSprite,
}

impl NpcTemplate {
    /// Get the ActorImpl for this NPC type
    pub fn actor_impl(&self) -> ActorImpl {
        match self {
            Self::Dog => ActorImpl::new(
                Origin::Evolved,
                Approach::Direct,
                Resilience::Primal,
                ActorIdentity::Npc(NpcType::WildDog),
            ),
            Self::Wolf => ActorImpl::new(
                Origin::Evolved,
                Approach::Ambushing,
                Resilience::Vital,
                ActorIdentity::Npc(NpcType::WildDog), // TODO: Add NpcType::Wolf variant
            ),
            Self::ForestSprite => ActorImpl::new(
                Origin::Essential,
                Approach::Distant,  // Ranged enemy that kites
                Resilience::Primal,
                ActorIdentity::Npc(NpcType::ForestSprite),
            ),
        }
    }

    /// Select a random NPC template with weighted distribution (ADR-010 Phase 4)
    ///
    /// ADR-010 specifies 40% Forest Sprites, 60% Wild Dogs for varied encounters.
    /// This creates tactical variety in spawned encounters.
    pub fn random_mixed() -> Self {
        let mut rng = rand::rng();
        let roll = rng.random_range(0..100);
        match roll {
            0..40 => Self::ForestSprite,  // 40% ranged
            _ => Self::Dog,              // 60% melee
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forest_sprite_template_actor_impl() {
        let sprite = NpcTemplate::ForestSprite.actor_impl();
        assert_eq!(sprite.origin, Origin::Essential);
        assert_eq!(sprite.approach, Approach::Distant); // Ranged enemy
        assert_eq!(sprite.resilience, Resilience::Primal);
        assert_eq!(sprite.identity, ActorIdentity::Npc(NpcType::ForestSprite));
    }
}
