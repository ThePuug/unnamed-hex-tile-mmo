use bevy::prelude::*;

/// AllyTarget component for tracking which ally entity is currently targeted
/// Used for friendly abilities and target frames
/// Updated reactively when heading or location changes
///
/// Parallel to the Target component but for allies instead of hostiles
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AllyTarget {
    /// The currently selected ally target (updated by targeting system)
    pub entity: Option<Entity>,
    /// The last ally target (sticky for UI - persists even when no current target)
    pub last_target: Option<Entity>,
}
