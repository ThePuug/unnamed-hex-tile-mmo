use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The status effects that slow or hold an actor, one of each kind, a
/// fresh one replacing the one it lands on. Each is a share of its speed,
/// and its pace is all of them together: every caller of the physics takes
/// it through [`Status::pace_of`], and it scales the walk and the turn
/// alike, so the server, the owner's prediction and every remote simulation
/// agree. At no pace, a root or a hold, an actor neither walks nor turns; a
/// hold stops more: a held actor does not jump or swing either
/// ([`Status::holds`]).
///
/// The server sends the whole of it whenever an ability changes it, and
/// both sides count its timed effects down, so a client moves an actor at
/// the pace the server does without waiting on word that an effect ended.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Status {
    /// Slowed, by Intimidation: held to `pace` of its speed for `remaining`
    /// seconds ([`Status::slow`]). The server works `pace` out from its
    /// tuning and sends it, so a client holds none of the tuning.
    pub slow: Option<Timed>,
    /// Rooted, a slow to nothing: it can neither move nor turn, and swings
    /// and recovers as ever ([`Status::root`])
    pub root: Option<Timed>,
    /// Overcommitted: the seconds each stack has left, a stack put on by
    /// every attack the actor made at a foe with Patience; 0 is no stack
    /// ([`Status::overcommit`])
    pub overcommitted: [f32; OVERCOMMIT_STACKS],
    /// A strike across the striker's own line, its stride broken
    pub stride: Option<Timed>,
    /// A Perfect Stride under way: its strikes across its own line break no
    /// stride, and it runs at `pace` of its speed, above whole
    pub perfect_stride: Option<Timed>,
    /// Held in place, by a stun or by the stagger of a Kick ([`Status::hold`])
    pub held: Option<Timed>,
    /// Carrying past the bag's burden limit
    pub burden: bool,
}

/// The most stacks of Overcommitted an actor carries
pub const OVERCOMMIT_STACKS: usize = 10;

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
        Timed::pace(self.slow) * Timed::pace(self.root) * Timed::pace(self.stride) * Timed::pace(self.perfect_stride) * Timed::pace(self.held) * burden
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

    /// How many stacks of Overcommitted the actor carries now
    pub fn overcommits(&self) -> usize {
        self.overcommitted.iter().filter(|&&left| left > 0.0).count()
    }

    /// Puts a stack of Overcommitted lasting `seconds` on the actor, on its
    /// own: in an empty place, or in place of the stack with least left
    pub fn overcommit(&mut self, seconds: f32) {
        if let Some(place) = self.overcommitted.iter_mut().min_by(|a, b| a.total_cmp(b)) {
            *place = place.max(seconds);
        }
    }

    /// Whether the actor is slowed or rooted now
    pub fn is_slowed(&self) -> bool {
        [self.slow, self.root].iter().flatten().any(|timed| timed.remaining > 0.0)
    }

    /// Roots the actor for `seconds`, or for what a root already on it has
    /// left where that is longer
    pub fn root(&mut self, seconds: f32) {
        let left = self.root.map_or(0.0, |root| root.remaining);
        self.root = Some(Timed { pace: 0.0, remaining: seconds.max(left) });
    }

    /// Slows the actor to `pace` for `seconds`, keeping what a slow already
    /// on it holds deeper or longer: a light slow never eases a deep one.
    pub fn slow(&mut self, pace: f32, seconds: f32) {
        let (held, left) = self.slow.map_or((1.0, 0.0), |slow| (slow.pace, slow.remaining));
        self.slow = Some(Timed { pace: pace.min(held), remaining: seconds.max(left) });
    }

    /// Counts the timed effects and Overcommitted stacks down by `dt`
    /// seconds, dropping spent ones. Returns whether any was counting.
    pub fn tick(&mut self, dt: f32) -> bool {
        let mut counted = false;
        for left in self.overcommitted.iter_mut().filter(|left| **left > 0.0) {
            *left = (*left - dt).max(0.0);
            counted = true;
        }
        for slot in [&mut self.slow, &mut self.root, &mut self.stride, &mut self.perfect_stride, &mut self.held] {
            if let Some(timed) = slot {
                timed.remaining -= dt;
                if timed.remaining <= 0.0 {
                    *slot = None;
                }
                counted = true;
            }
        }
        counted
    }
}

/// Counts every actor's timed effects down, on the server and on each
/// client alike.
pub fn tick_status(mut query: Query<&mut Status>, time: Res<Time>) {
    let dt = time.delta_secs();
    for mut status in &mut query {
        // An idle status is left unchanged, so what waits on a change to
        // one is not woken every tick
        if status.bypass_change_detection().tick(dt) {
            status.set_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_wears_off_on_its_own() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_systems(Update, tick_status);
        let mut alone = Status::default();
        alone.root(1.0);
        let rooted = app.world_mut().spawn(alone).id();
        let mut stacked = Status::default();
        stacked.overcommitted[0] = 1.0;
        let overcommitted = app.world_mut().spawn(stacked).id();
        for _ in 0..3 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
            app.update();
        }
        assert!(app.world().get::<Status>(rooted).unwrap().root.is_none(), "a root with nothing beside it");
        assert_eq!(app.world().get::<Status>(overcommitted).unwrap().overcommits(), 0, "and a stack");
    }

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
    fn each_stack_of_overcommitted_runs_out_on_its_own_and_ten_is_the_most() {
        let mut status = Status::default();
        status.overcommit(5.0);
        status.tick(2.0);
        status.overcommit(5.0);
        assert_eq!(status.overcommits(), 2);
        status.tick(3.0);
        assert_eq!(status.overcommits(), 1, "the first runs out, and the second was never refreshed by it");
        (0..OVERCOMMIT_STACKS + 3).for_each(|_| status.overcommit(5.0));
        assert_eq!(status.overcommits(), OVERCOMMIT_STACKS);
        status.tick(5.0);
        assert_eq!(status.overcommits(), 0);
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
