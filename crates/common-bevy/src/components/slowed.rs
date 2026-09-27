use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// An actor slowed for a while: it keeps `pace` of its speed for
/// `remaining` seconds. A fresh slow replaces the one it lands on.
///
/// Both sides count it down, so a client moves a slowed actor at the pace
/// the server does without waiting on word that it ended.
#[derive(Clone, Component, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Slowed {
    /// Share of its speed the actor keeps
    pub pace: f32,
    pub remaining: f32,
}

impl Slowed {
    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// The pace an actor keeps under `slowed`, whole without one or once it
    /// has run out
    pub fn pace_of(slowed: Option<&Slowed>) -> f32 {
        slowed.filter(|slowed| slowed.is_active()).map_or(1.0, |slowed| slowed.pace)
    }
}

/// Counts every slow down, on the server and on each client alike, and
/// leaves a spent one at zero.
pub fn tick_slowed(mut query: Query<&mut Slowed>, time: Res<Time>) {
    for mut slowed in &mut query {
        if slowed.is_active() {
            slowed.remaining = (slowed.remaining - time.delta_secs()).max(0.0);
        }
    }
}
