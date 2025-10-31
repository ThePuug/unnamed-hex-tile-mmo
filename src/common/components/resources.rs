use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Health resource component following state/step prediction pattern
/// - state: Server-authoritative HP (confirmed)
/// - step: Client prediction (local player) OR interpolated value (remote entities)
/// - max: Maximum HP calculated from ActorAttributes
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Health {
    pub state: f32,
    pub step: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            state: 100.0,
            step: 100.0,
            max: 100.0,
        }
    }
}

impl Health {
    /// Returns the current health value
    /// On server: returns state (authoritative)
    /// On client: returns state (confirmed) or step (predicted for local player)
    pub fn current(&self) -> f32 {
        self.state
    }
}

/// Stamina resource component following state/step prediction pattern
/// - state: Server-authoritative stamina (confirmed)
/// - step: Client prediction (local player) OR interpolated value (remote entities)
/// - max: Maximum stamina calculated from ActorAttributes
/// - regen_rate: Stamina regeneration per second
/// - last_update: Duration from Time::elapsed() when last regenerated
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Stamina {
    pub state: f32,
    pub step: f32,
    pub max: f32,
    pub regen_rate: f32,
    #[serde(skip)]
    pub last_update: Duration,
}

impl Default for Stamina {
    fn default() -> Self {
        Self {
            state: 100.0,
            step: 100.0,
            max: 100.0,
            regen_rate: 10.0,
            last_update: Duration::ZERO,
        }
    }
}

/// Mana resource component following state/step prediction pattern
/// - state: Server-authoritative mana (confirmed)
/// - step: Client prediction (local player) OR interpolated value (remote entities)
/// - max: Maximum mana calculated from ActorAttributes
/// - regen_rate: Mana regeneration per second
/// - last_update: Duration from Time::elapsed() when last regenerated
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Mana {
    pub state: f32,
    pub step: f32,
    pub max: f32,
    pub regen_rate: f32,
    #[serde(skip)]
    pub last_update: Duration,
}

impl Default for Mana {
    fn default() -> Self {
        Self {
            state: 100.0,
            step: 100.0,
            max: 100.0,
            regen_rate: 8.0,
            last_update: Duration::ZERO,
        }
    }
}

/// Combat state component tracking whether entity is in combat
/// - in_combat: Whether entity is currently in combat
/// - last_action: Duration from Time::elapsed() when last combat action occurred
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct CombatState {
    pub in_combat: bool,
    #[serde(skip)]
    pub last_action: Duration,
}

impl Default for CombatState {
    fn default() -> Self {
        Self {
            in_combat: false,
            last_action: Duration::ZERO,
        }
    }
}

/// Respawn timer for dead players
/// Tracks time until respawn at origin (0,0,4)
#[derive(Clone, Component, Copy, Debug)]
pub struct RespawnTimer {
    /// Time when death occurred
    pub death_time: Duration,

    /// How long to wait before respawn (5 seconds)
    pub respawn_delay: Duration,
}

impl RespawnTimer {
    pub fn new(death_time: Duration) -> Self {
        Self {
            death_time,
            respawn_delay: Duration::from_secs(5),
        }
    }

    pub fn should_respawn(&self, current_time: Duration) -> bool {
        current_time.saturating_sub(self.death_time) >= self.respawn_delay
    }
}
