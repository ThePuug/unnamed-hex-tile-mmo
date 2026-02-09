use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    #[default] Unset,
    Controlled,
}

/// Marker component for player-controlled entities (vs NPC/AI-controlled)
///
/// Used for ally/enemy distinction in targeting, health bars, and other gameplay systems.
///
/// # Distinction from Behaviour::Controlled
///
/// On the **server**:
/// - `Behaviour::Controlled` = entity responds to player input
/// - `PlayerControlled` = entity is controlled by a human player (same as Behaviour::Controlled)
///
/// On the **client**:
/// - `Behaviour::Controlled` = entity movement is interpolated via server updates (ALL actors)
/// - `PlayerControlled` = entity represents a human player (for ally/enemy logic)
///
/// This separation allows:
/// - All client entities to use `Behaviour::Controlled` for smooth movement interpolation
/// - Only player entities to have `PlayerControlled` for ally targeting and UI
/// - Future faction/allegiance systems without changing movement code
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Reflect)]
#[reflect(Component)]
pub struct PlayerControlled;

