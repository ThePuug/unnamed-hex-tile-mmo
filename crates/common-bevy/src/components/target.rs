use bevy::prelude::*;

/// What an actor's targeting keeps: the entity it selects now, and the
/// last one it selected, which stays once nothing is selected so a frame
/// that keeps showing it has it. `Target` and `AllyTarget` are selected by
/// the one rule (`targeting::update_targets_impl`).
pub trait Selection {
    /// Makes `picked` the selection: none clears the current one and
    /// leaves the last.
    fn select(&mut self, picked: Option<Entity>);
}

/// Target component for tracking which hostile entity is currently targeted
/// Used by both players and NPCs for abilities and auto-attack
/// Updated reactively when heading or location changes

/// Parallel to the AllyTarget component but for hostiles instead of allies
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Target {
    /// The currently selected hostile target (updated by targeting system)
    pub entity: Option<Entity>,
    /// The last hostile target (sticky for UI - persists even when no current target)
    pub last_target: Option<Entity>,
}

impl Selection for Target {
    fn select(&mut self, picked: Option<Entity>) {
        self.entity = picked;
        if picked.is_some() {
            self.last_target = picked;
        }
    }
}
