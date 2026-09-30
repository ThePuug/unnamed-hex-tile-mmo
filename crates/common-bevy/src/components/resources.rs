use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Health, as the server holds it and sends it
/// - state: what the actor has now
/// - max: Maximum HP calculated from ActorAttributes
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Health {
    pub state: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            state: 100.0,
            max: 100.0,
        }
    }
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

/// Stamina, as the server holds it and sends it
/// - state: what the actor has now
/// - max: Maximum stamina calculated from ActorAttributes
/// - regen_rate: Stamina regeneration per second
/// - last_update: Duration from Time::elapsed() when last regenerated
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Stamina {
    pub state: f32,
    pub max: f32,
    pub regen_rate: f32,
    #[serde(skip)]
    pub last_update: Duration,
}

impl Stamina {
    /// Stamina every actor regains each second
    pub const REGEN: f32 = 10.0;

    /// A pool of `max`, full, regenerating from `now`
    pub fn full(max: f32, now: Duration) -> Self {
        Self { state: max, max, regen_rate: Self::REGEN, last_update: now }
    }
}

impl Default for Stamina {
    fn default() -> Self {
        Self {
            state: 100.0,
            max: 100.0,
            regen_rate: 10.0,
            last_update: Duration::ZERO,
        }
    }
}

/// Endurance, as the server holds it and sends it: what an actor spends on
/// every skill and reaction beside its stamina. It refuses nothing; spent,
/// it tires the actor ([`Endurance::fatigue`]). It refills only while
/// stamina is full (`resources::regenerate_resources`).
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

    /// How spent the pool is: 0 full, 1 empty. Fatigue lengthens the
    /// actor's recoveries and shortens the windows of threats against it.
    pub fn fatigue(&self) -> f32 {
        if self.max > 0.0 { (1.0 - self.state / self.max).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// The fatigue of an actor with `endurance`; none where it has no pool
    pub fn fatigue_of(endurance: Option<&Endurance>) -> f32 {
        endurance.map_or(0.0, Endurance::fatigue)
    }
}

/// Mana, as the server holds it and sends it
/// - state: what the actor has now
/// - max: Maximum mana calculated from ActorAttributes
/// - regen_rate: Mana regeneration per second
/// - last_update: Duration from Time::elapsed() when last regenerated
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct Mana {
    pub state: f32,
    pub max: f32,
    pub regen_rate: f32,
    #[serde(skip)]
    pub last_update: Duration,
}

impl Mana {
    /// The mana every actor holds, no attribute deepening it
    pub const MAX: f32 = 100.0;
    /// Mana every actor regains each second
    pub const REGEN: f32 = 8.0;

    /// The pool, full, regenerating from `now`
    pub fn full(now: Duration) -> Self {
        Self { state: Self::MAX, max: Self::MAX, regen_rate: Self::REGEN, last_update: now }
    }
}

impl Default for Mana {
    fn default() -> Self {
        Self {
            state: 100.0,
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

/// The location players respawn at after death.
/// Initialized by the server with terrain-aware height.
#[derive(Clone, Copy, Debug, Resource)]
pub struct SpawnPoint(pub qrz::Qrz);

/// Respawn timer for dead players
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
