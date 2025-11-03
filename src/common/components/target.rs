use bevy::prelude::*;

/// Target component for tracking which entity is currently targeted
/// Used by both players and NPCs for abilities and auto-attack
/// Updated reactively when heading or location changes
#[derive(Clone, Component, Copy, Debug, Deref, DerefMut)]
pub struct Target(pub Option<Entity>);

impl Target {
    pub fn new(ent: Option<Entity>) -> Self {
        Self(ent)
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn set(&mut self, ent: Entity) {
        self.0 = Some(ent);
    }

    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl Default for Target {
    fn default() -> Self {
        Self(None)
    }
}
