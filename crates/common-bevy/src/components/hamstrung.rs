use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What a Juggernaut's Hamstrings have done to an actor this fight. Every
/// Hamstring adds a stack, and the stacks slow it, so the longer a fight
/// runs the less it escapes a Juggernaut. They clear when the actor leaves
/// combat.
///
/// The server works out `pace` from its tuning and sends it,
/// so a client moves a hamstrung actor at the pace the server does without
/// holding the tuning.
#[derive(Clone, Component, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Hamstrung {
    pub stacks: u8,
    /// Share of its speed the actor keeps
    pub pace: f32,
}

impl Default for Hamstrung {
    fn default() -> Self {
        Self { stacks: 0, pace: 1.0 }
    }
}

impl Hamstrung {
    /// The pace an actor keeps under `hamstrung`, whole without one
    pub fn pace_of(hamstrung: Option<&Hamstrung>) -> f32 {
        hamstrung.map_or(1.0, |hamstrung| hamstrung.pace)
    }
}
