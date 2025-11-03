
use bevy::ecs::event::Event;
use serde::{Deserialize, Serialize};

/// Global Cooldown (GCD) type for ability system
///
/// Currently only Attack is used - abilities trigger Attack GCD.
/// Spawn and PlaceSpawner were removed (dead code, replaced by spawner system).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Event, PartialEq, Serialize)]
pub enum GcdType {
    /// Attack GCD triggered by all combat abilities
    Attack,
}
