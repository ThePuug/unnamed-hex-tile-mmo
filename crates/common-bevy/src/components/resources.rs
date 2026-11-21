use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::moment::Moment;

/// Health, as the server holds it and sends it
/// - state: what the actor has now
/// - max: Maximum HP calculated from ActorAttributes
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Health {
    pub state: f32,
    pub max: f32,
}

/// What a pool of `max` holding `state` holds once it is resized to
/// `new_max`: as full as it was, so a change of attributes neither fills
/// nor drains it.
fn resized(state: f32, max: f32, new_max: f32) -> f32 {
    if max > 0.0 { state / max * new_max } else { new_max }
}

impl Health {
    /// A pool of `max`, full
    pub fn full(max: f32) -> Self {
        Self { state: max, max }
    }

    /// Resizes the pool to `max`, as full as it was
    pub fn resize(&mut self, max: f32) {
        *self = Self { state: resized(self.state, self.max, max), max };
    }

    /// What the actor has now
    pub fn current(&self) -> f32 {
        self.state
    }
}

/// Endurance, as the server holds it and sends it: what an actor spends on
/// every skill, as it starts. A skill that costs more than its user holds
/// is refused. It refills steadily, in combat or out
/// (`resources::regenerate_resources`).
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Endurance {
    pub state: f32,
    pub max: f32,
}

impl Endurance {
    /// A pool of `max`, full
    pub fn full(max: f32) -> Self {
        Self { state: max, max }
    }

    /// Resizes the pool to `max`, as full as it was
    pub fn resize(&mut self, max: f32) {
        *self = Self { state: resized(self.state, self.max, max), max };
    }
}

/// Combat state component tracking whether entity is in combat
/// - in_combat: Whether entity is currently in combat
/// - last_action: the moment of the last combat action, on the server's clock
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct CombatState {
    pub in_combat: bool,
    #[serde(skip)]
    pub last_action: Moment,
}

impl Default for CombatState {
    fn default() -> Self {
        Self {
            in_combat: false,
            last_action: Moment::ZERO,
        }
    }
}

/// The location players enter and respawn at, standing on the haven's
/// ground. The server inserts it once that ground is read, which takes a
/// while after start; until then it is absent.
#[derive(Clone, Copy, Debug, Resource)]
pub struct SpawnPoint(pub qrz::Qrz);

/// Respawn timer for dead players
#[derive(Clone, Component, Copy, Debug)]
pub struct RespawnTimer {
    /// The moment of death, on the server's clock
    pub death_time: Moment,

    /// How long to wait before respawn (5 seconds)
    pub respawn_delay: Duration,
}

impl RespawnTimer {
    pub fn new(death_time: Moment) -> Self {
        Self {
            death_time,
            respawn_delay: Duration::from_secs(5),
        }
    }

    pub fn should_respawn(&self, current_time: Moment) -> bool {
        current_time.since(self.death_time) >= self.respawn_delay
    }
}
