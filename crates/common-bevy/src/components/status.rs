use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The status effects that slow or hold an actor, one of each kind, a
/// fresh one replacing the one it lands on. Each is a share of its speed,
/// and its pace is all of them together: every caller of the physics takes
/// its speed through [`Status::pace_of`], so the server, the owner's
/// prediction and every remote simulation agree. A hold is an effect at no
/// pace at all, and stops more than the walk: a held actor does not turn,
/// jump or swing either ([`Status::holds`]).
///
/// The server sends the whole of it whenever an ability changes it, and
/// both sides count its timed effects down, so a client moves an actor at
/// the pace the server does without waiting on word that an effect ended.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Status {
    /// Slowed, by a blow Grit's bank struck back: held to `pace` of its
    /// speed for `remaining` seconds ([`Status::slow`]). The server works
    /// `pace` out from its tuning and sends it, so a client holds none of
    /// the tuning.
    pub slow: Option<Timed>,
    /// A strike across the striker's own line, its stride broken
    pub stride: Option<Timed>,
    /// A Perfect Stride under way: its strikes across its own line break no
    /// stride, and it runs at `pace` of its speed, above whole
    pub perfect_stride: Option<Timed>,
    /// Held in place, by a stun or by the stagger of a Kick ([`Status::hold`])
    pub held: Option<Timed>,
    /// Carrying past the bag's burden limit
    pub burden: bool,
    /// Waiting on a swing it could not strike, and nothing struck or used
    /// since: Patience refills its stamina faster
    /// (`ActorAttributes::patience_regen`). The server decides and sends it.
    pub waiting: bool,
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

/// The share of its speed an overburdened actor keeps: slow enough that its
/// gait plays the walk, which the actors' walk stride sets.
pub const BURDENED_PACE: f32 = 0.2;

impl Status {
    /// The share of its speed the actor moves at under every effect on it
    pub fn pace(&self) -> f32 {
        let burden = if self.burden { BURDENED_PACE } else { 1.0 };
        Timed::pace(self.slow) * Timed::pace(self.stride) * Timed::pace(self.perfect_stride) * Timed::pace(self.held) * burden
    }

    /// The pace of an actor with `status`, whole with none
    pub fn pace_of(status: Option<&Status>) -> f32 {
        status.map_or(1.0, Status::pace)
    }

    /// Whether a Perfect Stride holds now
    pub fn is_striding(&self) -> bool {
        self.perfect_stride.is_some_and(|stride| stride.remaining > 0.0)
    }

    /// Whether the actor is held in place now
    pub fn is_held(&self) -> bool {
        self.held.is_some_and(|held| held.remaining > 0.0)
    }

    /// Whether an actor with `status` is held in place now: it neither
    /// moves, turns, jumps nor swings. Its abilities and reactions are for
    /// the recovery whatever held it lays on alongside.
    pub fn holds(status: Option<&Status>) -> bool {
        status.is_some_and(Status::is_held)
    }

    /// Holds the actor for `seconds`, or for what a hold already on it has
    /// left where that is longer: a short hold never cuts a long one.
    pub fn hold(&mut self, seconds: f32) {
        let left = self.held.map_or(0.0, |held| held.remaining);
        self.held = Some(Timed { pace: 0.0, remaining: seconds.max(left) });
    }

    /// Slows the actor to `pace` for `seconds`, keeping what a slow already
    /// on it holds deeper or longer: a light slow never eases a deep one.
    pub fn slow(&mut self, pace: f32, seconds: f32) {
        let (held, left) = self.slow.map_or((1.0, 0.0), |slow| (slow.pace, slow.remaining));
        self.slow = Some(Timed { pace: pace.min(held), remaining: seconds.max(left) });
    }

    /// Counts the timed effects down by `dt` seconds, dropping spent ones
    pub fn tick(&mut self, dt: f32) {
        for slot in [&mut self.slow, &mut self.stride, &mut self.perfect_stride, &mut self.held] {
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
        if status.slow.is_some() || status.stride.is_some() || status.perfect_stride.is_some() || status.held.is_some() {
            status.tick(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_holds_the_deeper_and_the_longer_of_two() {
        let mut status = Status::default();
        status.slow(0.7, 1.0);
        status.slow(0.9, 3.0);
        assert_eq!(status.slow, Some(Timed { pace: 0.7, remaining: 3.0 }), "a light slow never eases a deep one, and lengthens it");
        status.tick(3.0);
        assert_eq!(status.pace(), 1.0, "it runs out");
    }

    #[test]
    fn a_perfect_stride_runs_its_user_faster_until_it_runs_out() {
        let mut status = Status { perfect_stride: Some(Timed { pace: 1.25, remaining: 1.0 }), ..default() };
        assert!(status.is_striding() && status.pace() > 1.0);
        status.tick(1.0);
        assert!(!status.is_striding() && status.pace() == 1.0, "spent, it runs at its own pace");
    }

    #[test]
    fn every_effect_adjusts_the_one_pace() {
        let slowed = Status { slow: Some(Timed { pace: 0.8, remaining: 1.0 }), ..default() };
        let stumbling = Status { stride: Some(Timed { pace: 0.9, remaining: 1.0 }), ..default() };
        let both = Status { slow: slowed.slow, stride: stumbling.stride, ..default() };
        assert!(slowed.pace() < 1.0 && stumbling.pace() < 1.0);
        assert!((both.pace() - slowed.pace() * stumbling.pace()).abs() < 1e-6, "effects multiply");
        let burdened = Status { burden: true, ..both };
        assert!(burdened.pace() < both.pace());
        assert_eq!(Status::pace_of(None), 1.0);
    }

    #[test]
    fn a_broken_stride_slows_with_the_rest() {
        let slowed = Status { slow: Some(Timed { pace: 0.5, remaining: 1.0 }), ..default() };
        let stumbling = Status { stride: Some(Timed { pace: 0.7, remaining: 1.0 }), ..slowed };
        assert!(stumbling.pace() < slowed.pace());
        let mut spent = stumbling;
        spent.tick(1.5);
        assert_eq!(spent.stride, None);
    }

    #[test]
    fn a_hold_stops_the_actor_until_it_runs_out_and_a_shorter_one_never_cuts_it() {
        let mut status = Status::default();
        assert!(!status.is_held() && !Status::holds(None));
        status.hold(1.0);
        assert!(status.is_held());
        assert_eq!(status.pace(), 0.0, "held, it moves at no pace");
        status.hold(0.25);
        status.tick(0.5);
        assert!(status.is_held(), "the longer hold stands");
        status.tick(0.6);
        assert!(!status.is_held());
        assert_eq!(status.pace(), 1.0);
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
}
