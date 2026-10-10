use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::AbilityType;

/// The recovery an ability leaves its user in: no other ability until it
/// runs out, but the combo it offers and what Ferocity and Preparation fire
/// early (`combos::may_use`). The combo it offers is its own; the chain it
/// is part of runs on through the recoveries of the skills that continue
/// it, and what the chain owes is paid as one more recovery once the last
/// runs out (`tick`).
///
/// The server starts and changes every recovery and sends the whole of it
/// (`message::Component::Recovery`); server and client count it down alike
/// between (`recovery::global_recovery_system`), so a client starts none of
/// its own.
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GlobalRecovery {
    /// Seconds until every ability unlocks
    pub remaining: f32,
    /// Seconds the whole recovery runs
    pub duration: f32,
    /// Focus of the opponent, which contests Fitness
    pub target_focus: u16,
    /// Level of the opponent, for the level edge in Fitness's contest;
    /// None with no opponent, which gives no edge
    pub target_level: Option<u32>,
    /// The combo this recovery offers, none where the ability leads on to
    /// nothing or its contest leaves no time to take it in
    pub combo: Option<Combo>,
    /// The chain this recovery is part of
    pub chain: Chain,
}

/// A chain of skills: opened by one taken in its own time, out of recovery,
/// and continued by each skill used inside its recoveries
/// (`combos::recovery_after`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    /// Seconds it owes, a share of what each skill fired early skipped,
    /// paid once its last recovery runs out
    pub owed: f32,
    /// Combos Ferocity fired early in it
    pub early_combos: u8,
    /// Reactions Preparation fired early in it
    pub early_reactions: u8,
    /// A strike taken in its own time stands in it, which lets Preparation
    /// fire reactions early
    pub struck: bool,
}

impl GlobalRecovery {
    pub fn new(duration: f32) -> Self {
        Self {
            remaining: duration,
            duration,
            target_focus: 0,
            target_level: None,
            combo: None,
            chain: Chain::default(),
        }
    }

    /// Contests this recovery's Fitness against `opponent`'s Focus and
    /// level: whoever it was used against, or whoever imposed it. With no
    /// opponent it runs uncontested, the whole Fitness against none.
    pub fn against(mut self, opponent: Option<&crate::components::ActorAttributes>) -> Self {
        if let Some(opponent) = opponent {
            self.target_focus = opponent.focus();
            self.target_level = Some(opponent.total_level());
        }
        self
    }

    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// Counts the recovery down by `delta` seconds. Run out with time its
    /// chain owes, it becomes a recovery of that time, offering nothing,
    /// and the chain ends.
    pub fn tick(&mut self, delta: f32) {
        self.remaining = (self.remaining - delta).max(0.0);
        if self.remaining <= 0.0 && self.chain.owed > 0.0 {
            *self = Self { remaining: self.chain.owed, duration: self.chain.owed, combo: None, chain: Chain::default(), ..*self };
        }
    }

    /// Pushes the recovery back by `pushback_amount` of its duration, to at
    /// most twice that, so no run of blows keeps an actor recovering for good.
    pub fn apply_pushback(&mut self, pushback_amount: f32) {
        let extension = self.duration * pushback_amount;
        self.remaining = (self.remaining + extension).min(self.duration * 2.0);
    }
}

/// The combo a recovery offers: `ability` may be used once the recovery has
/// `unlock_at` seconds left.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Combo {
    pub ability: AbilityType,
    pub unlock_at: f32,
}

impl Combo {
    pub fn is_unlocked(&self, recovery_remaining: f32) -> bool {
        recovery_remaining <= self.unlock_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recovery_counts_down_to_nothing_and_no_further() {
        let mut recovery = GlobalRecovery::new(1.0);
        assert!(recovery.is_active());
        recovery.tick(0.3);
        assert!((recovery.remaining - 0.7).abs() < 0.001);
        recovery.tick(1.0);
        assert_eq!(recovery.remaining, 0.0);
        assert!(!recovery.is_active());
    }

    #[test]
    fn what_a_chain_owes_is_paid_once_its_last_recovery_runs_out_offering_nothing() {
        let mut recovery = GlobalRecovery {
            combo: Some(Combo { ability: AbilityType::Frenzy, unlock_at: 0.5 }),
            chain: Chain { owed: 0.125, early_reactions: 1, struck: true, ..Chain::default() },
            ..GlobalRecovery::new(1.0)
        };
        recovery.tick(1.0);
        assert!(recovery.is_active(), "the debt becomes a recovery of its own");
        assert!((recovery.remaining - 0.125).abs() < 1e-6 && (recovery.duration - 0.125).abs() < 1e-6);
        assert!(recovery.combo.is_none(), "offering nothing");
        assert_eq!(recovery.chain, Chain::default(), "and the chain ends");
        recovery.tick(0.125);
        assert!(!recovery.is_active(), "then it is over");
    }

    #[test]
    fn a_pushback_extends_the_recovery_by_a_share_of_its_duration() {
        let mut recovery = GlobalRecovery::new(2.0);
        recovery.tick(1.0);
        recovery.apply_pushback(0.25);
        assert!((recovery.remaining - 1.5).abs() < 0.001, "half a second on a two-second recovery: {}", recovery.remaining);

        let mut spent = GlobalRecovery::new(1.0);
        spent.tick(1.5);
        spent.apply_pushback(0.25);
        assert!((spent.remaining - 0.25).abs() < 0.001, "from nothing left: {}", spent.remaining);
    }

    #[test]
    fn pushbacks_stop_at_twice_the_duration() {
        let mut recovery = GlobalRecovery::new(1.0);
        for _ in 0..3 {
            recovery.apply_pushback(0.5);
        }
        assert!((recovery.remaining - 2.0).abs() < 0.001, "{}", recovery.remaining);
    }
}
