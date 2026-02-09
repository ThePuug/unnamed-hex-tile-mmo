use bevy::prelude::*;
use qrz::Qrz;

use crate::common::components::heading::Heading;

/// Server-side component for tracking movement intent broadcast state (ADR-011)
///
/// Tracks the last destination and heading broadcast for an entity.
/// Used to prevent re-sending intents when neither has changed.
#[derive(Component, Debug)]
pub struct MovementIntentState {
    /// Last destination we broadcast to clients
    pub last_broadcast_dest: Qrz,
    /// Last heading we broadcast to clients
    pub last_broadcast_heading: Heading,
}

impl Default for MovementIntentState {
    fn default() -> Self {
        Self {
            last_broadcast_dest: Qrz { q: 0, r: 0, z: 0 },
            last_broadcast_heading: default(),
        }
    }
}
