use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::AbilityType;

/// The timed effects on an actor: what slows or holds it, the block it
/// has up, and the one stance it holds. Each slow or hold is a share of
/// its speed, and its pace is all of them together: every caller of the
/// physics takes it through [`Status::pace_of`] and
/// [`Status::turn_pace_of`], so the server, the owner's prediction and
/// every remote simulation agree. Every effect scales the walk and the
/// turn alike but the horse, which runs faster and turns slower. At no
/// pace, a root or a hold, an actor neither walks nor turns; a hold stops
/// more: a held actor does not jump, swing or use a skill either
/// ([`Status::holds`]).
///
/// The server sends the whole of it whenever an ability changes it, and
/// both sides count its timed effects down, so a client moves an actor at
/// the pace the server does without waiting on word that an effect ended.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Status {
    /// Slowed: held to `pace` of its speed for `remaining` seconds
    /// ([`Status::slow`]). The server works `pace` out from its tuning and
    /// sends it, so a client holds none of the tuning.
    pub slow: Option<Timed>,
    /// Rooted, a slow to nothing: it can neither move nor turn, and swings
    /// and activates as ever ([`Status::root`])
    pub root: Option<Timed>,
    /// Held in place, by a stun or for as long as a drag slides it
    /// ([`Status::hold`])
    pub held: Option<Timed>,
    /// Carrying past the bag's burden limit
    pub burden: bool,
    /// On its horse, which carries it at [`MOUNTED_PACE`] of its speed and
    /// turns at [`MOUNTED_TURN`] of its turn, until it fights, uses a skill
    /// or gathers
    pub mounted: bool,
    /// Pinned: no skill of its own that is only a move, for `remaining`
    /// seconds ([`Status::pin`]); its `pace` is unused
    pub pinned: Option<Timed>,
    /// A block up: the attacks it still stops, whole
    pub block: Option<Block>,
    /// A Perfect Stride under way: it runs at `pace` of its speed, above
    /// whole
    pub perfect_stride: Option<Timed>,
    /// A Bloodlust under way: `pace` of the damage its attacks land comes
    /// back to it as health, for `remaining` seconds
    pub bloodlust: Option<Timed>,
    /// A Brace under way: no hold, slow, root or pin a foe lays on it
    /// takes, and no drag moves it, for `remaining` seconds
    /// ([`Status::is_braced`]); its `pace` is unused
    pub braced: Option<Timed>,
    /// A Surge under way: each tile it moves adds `pace` more to its next
    /// skill strike, for `remaining` seconds or until that strike spends it
    pub surge: Option<Timed>,
    /// A Vigil under way: every attack from outside its arc is blocked, for
    /// `remaining` seconds ([`Status::is_vigilant`]); its `pace` is unused
    pub vigil: Option<Timed>,
    /// A Lurk under way: no foe beyond its reach sees it, for `remaining`
    /// seconds or until it attacks (`targeting::unseen`), and a skill
    /// struck out of it lands `pace` harder
    pub lurk: Option<Timed>,
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

/// A block a skill put up: it stops the next `attacks` attacks on its user
/// within `remaining` seconds, each whole, with all it carried.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Block {
    /// The skill that put it up, which says what a blocked attack's
    /// striker takes
    pub ability: AbilityType,
    pub attacks: u8,
    pub remaining: f32,
    /// How much of what its skill does to a blocked attack's striker its
    /// user's line gives: seconds of a Clash's stun, the share of the blow
    /// a Counter strikes back for
    pub power: f32,
}

/// The stances, one skill each: an actor holds one at a time
/// ([`Status::take`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stance {
    PerfectStride,
    Bloodlust,
    Brace,
    Surge,
    Vigil,
    Lurk,
}

impl Stance {
    pub const ALL: [Stance; 6] = [Stance::PerfectStride, Stance::Bloodlust, Stance::Brace, Stance::Surge, Stance::Vigil, Stance::Lurk];

    /// The stance `ability` takes, for a stance's skill
    pub fn of(ability: AbilityType) -> Option<Stance> {
        match ability {
            AbilityType::PerfectStride => Some(Stance::PerfectStride),
            AbilityType::Bloodlust => Some(Stance::Bloodlust),
            AbilityType::Brace => Some(Stance::Brace),
            AbilityType::Surge => Some(Stance::Surge),
            AbilityType::Vigil => Some(Stance::Vigil),
            AbilityType::Lurk => Some(Stance::Lurk),
            _ => None,
        }
    }
}

/// The share of its speed an overburdened actor keeps: slow enough that its
/// gait plays the walk, which the actors' walk stride sets.
pub const BURDENED_PACE: f32 = 0.2;

/// The share of its speed a mounted actor moves at, its horse's run against
/// its own.
pub const MOUNTED_PACE: f32 = 2.0;

/// The share of its turn a mounted actor keeps: a horse at the run turns
/// slower than its rider afoot, so it rides a wider circle than it walks.
pub const MOUNTED_TURN: f32 = 0.6;

impl Status {
    /// The share of its speed the actor moves at under every effect on it
    pub fn pace(&self) -> f32 {
        self.effects() * if self.mounted { MOUNTED_PACE } else { 1.0 }
    }

    /// The pace of an actor with `status`, whole with none
    pub fn pace_of(status: Option<&Status>) -> f32 {
        status.map_or(1.0, Status::pace)
    }

    /// The share of its turn the actor keeps under every effect on it
    pub fn turn_pace(&self) -> f32 {
        self.effects() * if self.mounted { MOUNTED_TURN } else { 1.0 }
    }

    /// The turn pace of an actor with `status`, whole with none
    pub fn turn_pace_of(status: Option<&Status>) -> f32 {
        status.map_or(1.0, Status::turn_pace)
    }

    /// Every effect that scales the walk and the turn alike
    fn effects(&self) -> f32 {
        let burden = if self.burden { BURDENED_PACE } else { 1.0 };
        Timed::pace(self.slow) * Timed::pace(self.root) * Timed::pace(self.perfect_stride) * Timed::pace(self.held) * burden
    }

    /// The place `stance` is kept in
    fn stance_mut(&mut self, stance: Stance) -> &mut Option<Timed> {
        match stance {
            Stance::PerfectStride => &mut self.perfect_stride,
            Stance::Bloodlust => &mut self.bloodlust,
            Stance::Brace => &mut self.braced,
            Stance::Surge => &mut self.surge,
            Stance::Vigil => &mut self.vigil,
            Stance::Lurk => &mut self.lurk,
        }
    }

    /// What is left of `stance` on it, if it holds it now
    pub fn stance(&self, stance: Stance) -> Option<Timed> {
        let slot = match stance {
            Stance::PerfectStride => self.perfect_stride,
            Stance::Bloodlust => self.bloodlust,
            Stance::Brace => self.braced,
            Stance::Surge => self.surge,
            Stance::Vigil => self.vigil,
            Stance::Lurk => self.lurk,
        };
        slot.filter(|timed| timed.remaining > 0.0)
    }

    /// The stance it holds now, if any
    pub fn held_stance(&self) -> Option<Stance> {
        Stance::ALL.into_iter().find(|&stance| self.stance(stance).is_some())
    }

    /// Takes `stance` as `timed` has it, ending the one it held: an actor
    /// holds one stance at a time
    pub fn take(&mut self, stance: Stance, timed: Timed) {
        for other in Stance::ALL {
            *self.stance_mut(other) = None;
        }
        *self.stance_mut(stance) = Some(timed);
    }

    /// Whether a Perfect Stride holds now
    pub fn is_striding(&self) -> bool {
        self.stance(Stance::PerfectStride).is_some()
    }

    /// Whether the actor is held in place now
    pub fn is_held(&self) -> bool {
        self.held.is_some_and(|held| held.remaining > 0.0)
    }

    /// Whether an actor with `status` is held in place now: it neither
    /// moves, turns, jumps, swings nor uses a skill.
    pub fn holds(status: Option<&Status>) -> bool {
        status.is_some_and(Status::is_held)
    }

    /// Whether a Lurk holds now: no foe beyond its reach sees it
    pub fn is_lurking(&self) -> bool {
        self.stance(Stance::Lurk).is_some()
    }

    /// Whether a Vigil holds now: every attack from outside its arc is
    /// blocked
    pub fn is_vigilant(&self) -> bool {
        self.stance(Stance::Vigil).is_some()
    }

    /// The share more each tile it has moved adds to its next skill strike
    /// now: a Surge's, none without one
    pub fn surging(&self) -> f32 {
        self.stance(Stance::Surge).map_or(0.0, |surge| surge.pace)
    }

    /// Whether a Brace holds now: nothing a foe lays on it holds or moves it
    pub fn is_braced(&self) -> bool {
        self.stance(Stance::Brace).is_some()
    }

    /// The share of the damage its attacks land that comes back to it as
    /// health now: a Bloodlust's, none without one
    pub fn lifesteal(&self) -> f32 {
        self.stance(Stance::Bloodlust).map_or(0.0, |bloodlust| bloodlust.pace)
    }

    /// The block it has up now, if it still stops an attack
    pub fn blocking(&self) -> Option<Block> {
        self.block.filter(|block| block.attacks > 0 && block.remaining > 0.0)
    }

    /// Stops one attack with the block it has up, and returns the block as
    /// it stood; None with none up, where the attack lands
    pub fn stop(&mut self) -> Option<Block> {
        let block = self.blocking()?;
        self.block = Some(Block { attacks: block.attacks - 1, ..block }).filter(|left| left.attacks > 0);
        Some(block)
    }

    /// Holds the actor for `seconds`, or for what a hold already on it has
    /// left where that is longer: a short hold never cuts a long one. A
    /// braced actor takes none. Returns whether it took.
    pub fn hold(&mut self, seconds: f32) -> bool {
        if self.is_braced() {
            return false;
        }
        let left = self.held.map_or(0.0, |held| held.remaining);
        self.held = Some(Timed { pace: 0.0, remaining: seconds.max(left) });
        true
    }

    /// Whether the actor is slowed or rooted now
    pub fn is_slowed(&self) -> bool {
        [self.slow, self.root].iter().flatten().any(|timed| timed.remaining > 0.0)
    }

    /// Roots the actor for `seconds`, or for what a root already on it has
    /// left where that is longer. A braced actor takes none.
    pub fn root(&mut self, seconds: f32) {
        if self.is_braced() {
            return;
        }
        let left = self.root.map_or(0.0, |root| root.remaining);
        self.root = Some(Timed { pace: 0.0, remaining: seconds.max(left) });
    }

    /// Slows the actor to `pace` for `seconds`, keeping what a slow already
    /// on it holds deeper or longer: a light slow never eases a deep one. A
    /// braced actor takes none.
    pub fn slow(&mut self, pace: f32, seconds: f32) {
        if self.is_braced() {
            return;
        }
        let (held, left) = self.slow.map_or((1.0, 0.0), |slow| (slow.pace, slow.remaining));
        self.slow = Some(Timed { pace: pace.min(held), remaining: seconds.max(left) });
    }

    /// Whether it is pinned now: no skill that is only a move
    pub fn is_pinned(&self) -> bool {
        self.pinned.is_some_and(|pinned| pinned.remaining > 0.0)
    }

    /// Pins the actor for `seconds`, or for what a pin already on it has left
    /// where that is longer. A braced actor takes none.
    pub fn pin(&mut self, seconds: f32) {
        if self.is_braced() {
            return;
        }
        let left = self.pinned.map_or(0.0, |pinned| pinned.remaining);
        self.pinned = Some(Timed { pace: 1.0, remaining: seconds.max(left) });
    }

    /// Counts the timed effects down by `dt` seconds, dropping spent ones.
    /// Returns whether any was counting.
    pub fn tick(&mut self, dt: f32) -> bool {
        let mut counted = false;
        for slot in [&mut self.slow, &mut self.root, &mut self.held, &mut self.pinned, &mut self.perfect_stride, &mut self.bloodlust, &mut self.braced, &mut self.surge, &mut self.vigil, &mut self.lurk] {
            if let Some(timed) = slot {
                timed.remaining -= dt;
                if timed.remaining <= 0.0 {
                    *slot = None;
                }
                counted = true;
            }
        }
        if let Some(block) = &mut self.block {
            block.remaining -= dt;
            if block.remaining <= 0.0 {
                self.block = None;
            }
            counted = true;
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
        let blocking = app.world_mut().spawn(Status { block: Some(Block { ability: AbilityType::Parry, attacks: 1, remaining: 1.0, power: 0.0 }), ..default() }).id();
        for _ in 0..3 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
            app.update();
        }
        assert!(app.world().get::<Status>(rooted).unwrap().root.is_none(), "a root with nothing beside it");
        assert!(app.world().get::<Status>(blocking).unwrap().block.is_none(), "and a block that stopped nothing");
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
        let striding = Status { perfect_stride: Some(Timed { pace: 1.2, remaining: 1.0 }), ..default() };
        let both = Status { slow: slowed.slow, perfect_stride: striding.perfect_stride, ..default() };
        assert!(slowed.pace() < 1.0 && striding.pace() > 1.0);
        assert!((both.pace() - slowed.pace() * striding.pace()).abs() < 1e-6, "effects multiply");
        let burdened = Status { burden: true, ..both };
        assert!(burdened.pace() < both.pace());
        assert_eq!(burdened.turn_pace(), burdened.pace(), "an effect slows the turn as it does the walk");
        let mounted = Status { mounted: true, ..both };
        assert!(mounted.pace() > both.pace(), "the horse runs faster");
        assert!(mounted.turn_pace() < both.turn_pace(), "and turns slower");
        assert_eq!(Status::pace_of(None), 1.0);
        assert_eq!(Status::turn_pace_of(None), 1.0);
    }

    #[test]
    fn a_hold_stops_the_actor_until_it_runs_out_and_a_shorter_one_never_cuts_it() {
        let mut status = Status::default();
        assert!(!status.is_held() && !Status::holds(None));
        assert!(status.hold(1.0));
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
    fn taking_a_stance_ends_the_one_before() {
        let mut status = Status::default();
        let timed = Timed { pace: 1.0, remaining: 5.0 };
        status.take(Stance::Bloodlust, timed);
        assert_eq!(status.held_stance(), Some(Stance::Bloodlust));
        status.take(Stance::Brace, timed);
        assert_eq!(status.held_stance(), Some(Stance::Brace), "the new one holds");
        assert_eq!(status.lifesteal(), 0.0, "and the one before is over");
        assert!(status.is_braced() && !status.hold(1.0), "a brace refuses a hold");
        status.tick(5.0);
        assert_eq!(status.held_stance(), None, "a stance runs out on its own");
    }

    #[test]
    fn a_block_stops_as_many_attacks_as_it_says_and_no_more() {
        let mut status = Status { block: Some(Block { ability: AbilityType::Parry, attacks: 1, remaining: 2.0, power: 0.0 }), ..default() };
        assert!(status.stop().is_some(), "the next attack is stopped");
        assert!(status.stop().is_none(), "and the one after lands");
        let mut late = Status { block: Some(Block { ability: AbilityType::Parry, attacks: 1, remaining: 2.0, power: 0.0 }), ..default() };
        late.tick(2.0);
        assert!(late.stop().is_none(), "run out, it stops nothing");
    }
}
