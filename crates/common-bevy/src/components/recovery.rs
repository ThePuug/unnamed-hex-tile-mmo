use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::AbilityType;

/// The recovery an ability leaves its user in: no other ability until it
/// runs out, but the combo it offers and what Preparation lets through
/// (`combos::may_use`, `reacts_through`). The combo it offers and the burst
/// it is part of are its own, and end with it.
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
    /// Impact of the opponent, which contests Composure
    pub target_impact: u16,
    /// Level of the opponent, for the level edge in Composure's contest;
    /// None with no opponent, which gives no edge
    pub target_level: Option<u32>,
    /// Seconds of the recovery carried from the one before, which a combo
    /// taken before it unlocked left unpaid (`combos::recovery_after`). No
    /// combo this recovery offers unlocks through it.
    pub carried: f32,
    /// Reactions used through this recovery, Discipline's Preparation
    /// allowing its tier of them (`combos::reacts_through`)
    pub reactions: u8,
    /// The combo this recovery offers, none where the ability leads on to
    /// nothing or its contest leaves no time to take it in
    pub combo: Option<Combo>,
    /// The Ferocity burst this recovery is part of
    pub burst: Option<Burst>,
}

impl GlobalRecovery {
    pub fn new(duration: f32) -> Self {
        Self {
            remaining: duration,
            duration,
            target_impact: 0,
            target_level: None,
            carried: 0.0,
            reactions: 0,
            combo: None,
            burst: None,
        }
    }

    /// Contests this recovery's Composure against `opponent`'s Impact and
    /// level: whoever it was used against, or whoever imposed it. With no
    /// opponent it runs uncontested, the whole Composure against none.
    pub fn against(mut self, opponent: Option<&crate::components::ActorAttributes>) -> Self {
        if let Some(opponent) = opponent {
            self.target_impact = opponent.impact();
            self.target_level = Some(opponent.total_level());
        }
        self
    }

    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// Counts the recovery down by `delta` seconds
    pub fn tick(&mut self, delta: f32) {
        self.remaining = (self.remaining - delta).max(0.0);
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

/// A Ferocity burst under way: `window` seconds left of its opener's
/// recovery, inside which `steps` more combos may fire before they unlock.
/// Each recovery of the burst hands it to the next.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Burst {
    pub window: f32,
    pub steps: u8,
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
    fn a_combo_unlocks_once_the_recovery_has_run_down_to_it() {
        let combo = Combo { ability: AbilityType::Overpower, unlock_at: 0.5 };
        assert!(!combo.is_unlocked(1.0));
        assert!(combo.is_unlocked(0.5));
        assert!(combo.is_unlocked(0.0));
    }

    #[test]
    fn every_ability_but_the_auto_attack_leaves_its_user_recovering() {
        use AbilityType::*;
        let tuning = crate::tuning::tuning();
        assert_eq!(tuning.recovery(AutoAttack), 0.0, "an auto-attack runs on its own timer");
        for ability in [Lunge, Overpower, Counter, Kick, Rattle, Disengage, Volley, Flank] {
            assert!(tuning.recovery(ability) > 0.0, "{ability:?} leaves its user recovering");
        }
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
