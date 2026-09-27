use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// An actor held completely: no movement, no turning, no jumping, no
/// abilities, no auto-attacks, no reactions, for `remaining` seconds.
///
/// Abilities and reactions are held by the universal lockout the stun lays
/// on alongside it; movement and auto-attacks read this. Both sides count
/// it down, so a client stops predicting a stunned player's movement
/// without waiting on the server's word that it ended.
#[derive(Clone, Component, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Stunned {
    pub remaining: f32,
}

impl Stunned {
    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// Whether `stunned` holds its actor now
    pub fn holds(stunned: Option<&Stunned>) -> bool {
        stunned.is_some_and(Stunned::is_active)
    }
}

/// Counts every stun down, on the server and on each client alike, and
/// leaves a spent one at zero.
pub fn tick_stunned(mut query: Query<&mut Stunned>, time: Res<Time>) {
    for mut stunned in &mut query {
        if stunned.is_active() {
            stunned.remaining = (stunned.remaining - time.delta_secs()).max(0.0);
        }
    }
}
