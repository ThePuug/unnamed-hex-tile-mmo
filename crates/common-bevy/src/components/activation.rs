//! A skill under way and a skill recharging: the two clocks every skill
//! runs on beside its endurance. The server starts each and sends it once,
//! whole, the moment it began on the game clock and its length
//! (`Event::Activate`, `Event::Recharge`); both sides count from there.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{message::AbilityType, moment::Moment};

/// One use of a skill from the moment it began: what, on whom, and how
/// long until it lands. A skill that takes hold at once has no length.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Activation {
    pub ability: AbilityType,
    pub target: Option<Entity>,
    /// The moment it began, on the game clock
    pub began: Moment,
    pub length: Duration,
}

impl Activation {
    /// The moment it lands
    pub fn ends(&self) -> Moment {
        self.began + self.length
    }

    /// How long until it lands at `now`, nothing once it has
    pub fn left(&self, now: Moment) -> Duration {
        now.until(self.ends())
    }

    /// How far through it stands at `now`, 0 as it begins to 1 as it lands
    pub fn progress(&self, now: Moment) -> f32 {
        if self.length.is_zero() {
            return 1.0;
        }
        (now.since(self.began).as_secs_f32() / self.length.as_secs_f32()).clamp(0.0, 1.0)
    }
}

/// The skill an actor is activating, None for one activating nothing.
/// Every actor holds one, so starting and ending a skill changes it in
/// place and whatever asks next in the same frame sees it.
#[derive(Clone, Component, Copy, Debug, Default, PartialEq)]
pub struct Activating(pub Option<Activation>);

/// One skill's recharge: the moment it began, on the game clock, and how
/// long it runs.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Recharge {
    pub began: Moment,
    pub length: Duration,
}

impl Recharge {
    /// How long until the skill is ready at `now`, nothing once it is
    pub fn left(&self, now: Moment) -> Duration {
        now.until(self.began + self.length)
    }
}

/// Each of an actor's skills recharging, the last recharge each began. A
/// skill with none, or whose recharge has run, is ready.
#[derive(Clone, Component, Debug, Default)]
pub struct Recharges(HashMap<AbilityType, Recharge>);

impl Recharges {
    /// Starts `ability`'s recharge, in place of any it was in
    pub fn start(&mut self, ability: AbilityType, recharge: Recharge) {
        self.0.insert(ability, recharge);
    }

    /// How long until `ability` is ready at `now`, nothing once it is
    pub fn left(&self, ability: AbilityType, now: Moment) -> Duration {
        self.0.get(&ability).map_or(Duration::ZERO, |recharge| recharge.left(now))
    }

    pub fn is_ready(&self, ability: AbilityType, now: Moment) -> bool {
        self.left(ability, now).is_zero()
    }

    /// `ability`'s recharge as it last began, run out or not
    pub fn of(&self, ability: AbilityType) -> Option<Recharge> {
        self.0.get(&ability).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recharge_locks_its_own_skill_until_it_runs_out() {
        let at = Moment::from_millis;
        let mut recharges = Recharges::default();
        assert!(recharges.is_ready(AbilityType::Frenzy, at(0)), "a skill never used is ready");
        recharges.start(AbilityType::Frenzy, Recharge { began: at(1_000), length: Duration::from_secs(3) });
        assert!(!recharges.is_ready(AbilityType::Frenzy, at(2_000)));
        assert!(recharges.is_ready(AbilityType::Overpower, at(2_000)), "and no other skill with it");
        assert!(recharges.left(AbilityType::Frenzy, at(3_000)) < recharges.left(AbilityType::Frenzy, at(2_000)), "it counts down");
        assert!(recharges.is_ready(AbilityType::Frenzy, at(4_000)));
    }

    #[test]
    fn an_activation_lands_its_length_after_it_began() {
        let activation = Activation { ability: AbilityType::Overpower, target: None, began: Moment::from_millis(500), length: Duration::from_millis(1_000) };
        assert_eq!(activation.ends(), Moment::from_millis(1_500));
        assert!(activation.progress(Moment::from_millis(1_000)) > activation.progress(Moment::from_millis(600)));
        assert_eq!(activation.left(Moment::from_millis(2_000)), Duration::ZERO, "nothing left once it has landed");
    }
}
