use bevy::prelude::*;

/// Target component for tracking which hostile entity is currently targeted
/// Used by both players and NPCs for abilities and auto-attack
/// Updated reactively when heading or location changes
///
/// Parallel to the AllyTarget component but for hostiles instead of allies
#[derive(Clone, Component, Copy, Debug)]
pub struct Target {
    /// The currently selected hostile target (updated by targeting system)
    pub entity: Option<Entity>,
    /// The last hostile target (sticky for UI - persists even when no current target)
    pub last_target: Option<Entity>,
}

impl Target {
    pub fn new() -> Self {
        Self {
            entity: None,
            last_target: None,
        }
    }

    /// Set the hostile target and update last_target
    pub fn set(&mut self, target: Entity) {
        self.entity = Some(target);
        self.last_target = Some(target);
    }

    /// Clear the current hostile target (but keep last_target for sticky UI)
    pub fn clear(&mut self) {
        self.entity = None;
        // Keep last_target for sticky UI behavior
    }

    /// Get the current hostile target
    pub fn get(&self) -> Option<Entity> {
        self.entity
    }

    /// Check if there is a current hostile target
    pub fn is_some(&self) -> bool {
        self.entity.is_some()
    }

    /// Check if there is no current hostile target
    pub fn is_none(&self) -> bool {
        self.entity.is_none()
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::new()
    }
}

// Deref to entity field for backward compatibility with code that uses *target
impl std::ops::Deref for Target {
    type Target = Option<Entity>;

    fn deref(&self) -> &Self::Target {
        &self.entity
    }
}

impl std::ops::DerefMut for Target {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entity
    }
}
