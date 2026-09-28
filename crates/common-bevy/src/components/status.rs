use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The status effects that slow or hasten an actor, one of each kind, a
/// fresh one replacing the one it lands on. Each is a share of its speed,
/// and its pace is all of them together: every caller of the physics takes
/// its speed through [`Status::pace_of`], so the server, the owner's
/// prediction and every remote simulation agree.
///
/// The server sends the whole of it whenever an ability changes it, and
/// both sides count its timed effects down, so a client moves an actor at
/// the pace the server does without waiting on word that an effect ended.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Status {
    /// A Volley's slow on its target
    pub slow: Option<Timed>,
    /// A Juggernaut's Rattles this fight
    pub daze: Option<Daze>,
    /// Carrying past the bag's burden limit
    pub burden: bool,
}

/// An effect that lasts `remaining` seconds at `pace` of the actor's speed.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Timed {
    pub pace: f32,
    pub remaining: f32,
}

impl Timed {
    fn pace(slot: Option<Timed>) -> f32 {
        slot.filter(|timed| timed.remaining > 0.0).map_or(1.0, |timed| timed.pace)
    }
}

/// A daze, `stacks` deep, that holds the actor to `pace` of its speed and
/// stretches its auto-attack interval and lockouts by the inverse, until it
/// leaves combat. The server works `pace` out from its tuning and sends it,
/// so a client holds none of the tuning.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Daze {
    pub stacks: u8,
    pub pace: f32,
}

/// The share of its speed an overburdened actor keeps: slow enough that its
/// gait plays the walk, which the actors' walk stride sets.
pub const BURDENED_PACE: f32 = 0.2;

impl Status {
    /// The share of its speed the actor moves at under every effect on it
    pub fn pace(&self) -> f32 {
        let burden = if self.burden { BURDENED_PACE } else { 1.0 };
        Timed::pace(self.slow) * self.daze_pace() * burden
    }

    /// The pace of an actor with `status`, whole with none
    pub fn pace_of(status: Option<&Status>) -> f32 {
        status.map_or(1.0, Status::pace)
    }

    /// The share of its pace a daze leaves the actor's swings and recovery
    pub fn daze_pace(&self) -> f32 {
        self.daze.map_or(1.0, |daze| daze.pace)
    }

    /// The daze stacks on an actor with `status`
    pub fn stacks_of(status: Option<&Status>) -> u8 {
        status.and_then(|status| status.daze).map_or(0, |daze| daze.stacks)
    }

    /// The auto-attack interval of an actor whose own is `interval`, under
    /// `status`'s daze. Every caller that times an auto-attack reads it here,
    /// so the server's NPCs and a client's own player swing alike.
    pub fn cadence(interval: std::time::Duration, status: Option<&Status>) -> std::time::Duration {
        interval.div_f32(status.map_or(1.0, Status::daze_pace))
    }

    /// Counts the timed effects down by `dt` seconds, dropping spent ones
    pub fn tick(&mut self, dt: f32) {
        for slot in [&mut self.slow] {
            if let Some(timed) = slot {
                timed.remaining -= dt;
                if timed.remaining <= 0.0 {
                    *slot = None;
                }
            }
        }
    }
}

/// Counts every actor's timed effects down, on the server and on each
/// client alike.
pub fn tick_status(mut query: Query<&mut Status>, time: Res<Time>) {
    let dt = time.delta_secs();
    for mut status in &mut query {
        if status.slow.is_some() {
            status.tick(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn every_effect_adjusts_the_one_pace() {
        let slowed = Status { slow: Some(Timed { pace: 0.8, remaining: 1.0 }), ..default() };
        let dazed = Status { daze: Some(Daze { stacks: 1, pace: 0.9 }), ..default() };
        let both = Status { slow: slowed.slow, daze: dazed.daze, ..default() };
        assert!(slowed.pace() < 1.0 && dazed.pace() < 1.0);
        assert!((both.pace() - slowed.pace() * dazed.pace()).abs() < 1e-6, "effects multiply");
        let burdened = Status { burden: true, ..both };
        assert!(burdened.pace() < both.pace());
        assert_eq!(Status::pace_of(None), 1.0);
    }

    #[test]
    fn a_timed_effect_runs_out() {
        let mut status = Status { slow: Some(Timed { pace: 0.8, remaining: 0.5 }), ..default() };
        status.tick(0.3);
        assert!(status.pace() < 1.0);
        status.tick(0.3);
        assert_eq!(status.slow, None);
        assert_eq!(status.pace(), 1.0);
    }

    #[test]
    fn a_deeper_daze_swings_slower_and_none_swings_as_its_own() {
        let own = Duration::from_millis(1500);
        let light = Status { daze: Some(Daze { stacks: 1, pace: 0.9 }), ..default() };
        let deep = Status { daze: Some(Daze { stacks: 3, pace: 0.7 }), ..default() };
        let slowed = Status { slow: Some(Timed { pace: 0.5, remaining: 1.0 }), ..default() };
        assert_eq!(Status::cadence(own, None), own);
        assert_eq!(Status::cadence(own, Some(&slowed)), own, "only a daze stretches a swing");
        assert!(Status::cadence(own, Some(&light)) > own);
        assert!(Status::cadence(own, Some(&deep)) > Status::cadence(own, Some(&light)));
    }
}
