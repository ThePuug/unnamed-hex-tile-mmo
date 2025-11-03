use tinyvec::ArrayVec;
use bevy::prelude::*;
use qrz::Qrz;
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

/// Defines how PathTo approaches its destination
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PathLimit {
    /// Move N tiles towards dest, then succeed (even if not at dest)
    By(u16),
    /// Move towards dest until N tiles away, then succeed
    Until(u16),
    /// Move all the way to dest (complete path)
    Complete,
}

/// Defines the pathfinding algorithm used by PathTo
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PathAlgorithm {
    /// Full A* pathfinding every time dest changes (slow but intelligent)
    Full,
    /// Greedy chase: move directly toward dest each tile, no pathfinding (fast, dumb)
    Greedy,
}

impl Default for PathLimit {
    fn default() -> Self {
        PathLimit::Complete
    }
}

impl Default for PathAlgorithm {
    fn default() -> Self {
        PathAlgorithm::Full
    }
}

#[derive(Clone, Component, Debug, Deserialize, Serialize)]
pub struct PathTo {
    pub dest: Qrz,
    pub path: ArrayVec<[Qrz; 20]>,
    pub limit: PathLimit,
    pub algorithm: PathAlgorithm,
}

impl Default for PathTo {
    fn default() -> Self {
        Self {
            dest: Qrz::default(),
            path: ArrayVec::new(),
            limit: PathLimit::default(),
            algorithm: PathAlgorithm::default(),
        }
    }
}