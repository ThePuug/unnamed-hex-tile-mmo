use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A daze: what a Juggernaut's Rattles have done to an actor this fight.
/// Every Rattle adds a stack, and the stacks slow everything it does, its
/// movement and its auto-attacks alike, so the longer a fight runs the less
/// it escapes a Juggernaut or presses one. They clear when the actor leaves
/// combat.
///
/// The server works out `pace` from its tuning and sends it, so a client
/// moves and swings a dazed actor at the pace the server does without
/// holding the tuning.
#[derive(Clone, Component, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Dazed {
    pub stacks: u8,
    /// Share of its pace the actor keeps, above zero: its speed is this
    /// share, and its auto-attack interval stretches by its inverse
    pub pace: f32,
}

impl Default for Dazed {
    fn default() -> Self {
        Self { stacks: 0, pace: 1.0 }
    }
}

impl Dazed {
    /// The pace an actor keeps under `dazed`, whole without one
    pub fn pace_of(dazed: Option<&Dazed>) -> f32 {
        dazed.map_or(1.0, |dazed| dazed.pace)
    }

    /// The auto-attack interval of an actor whose own is `interval`, under
    /// `dazed`. Every caller that times an auto-attack reads it here, so the
    /// server's NPCs and a client's own player swing alike.
    pub fn cadence(interval: std::time::Duration, dazed: Option<&Dazed>) -> std::time::Duration {
        interval.div_f32(Self::pace_of(dazed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_deeper_daze_swings_slower_and_none_swings_as_its_own() {
        let own = Duration::from_millis(1500);
        let light = Dazed { stacks: 1, pace: 0.9 };
        let deep = Dazed { stacks: 3, pace: 0.7 };
        assert_eq!(Dazed::cadence(own, None), own);
        assert!(Dazed::cadence(own, Some(&light)) > own);
        assert!(Dazed::cadence(own, Some(&deep)) > Dazed::cadence(own, Some(&light)));
    }
}
