use bevy::prelude::*;

/// Target component for tracking which hostile entity is currently targeted
/// Used by both players and NPCs for abilities and auto-attack
/// Updated reactively when heading or location changes
///
/// Parallel to the AllyTarget component but for hostiles instead of allies
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Target {
    /// The currently selected hostile target (updated by targeting system)
    pub entity: Option<Entity>,
    /// The last hostile target (sticky for UI - persists even when no current target)
    pub last_target: Option<Entity>,
}
