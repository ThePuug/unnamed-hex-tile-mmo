//! A player's bar: the eight skills it holds, any skill on any slot, none
//! twice. The server holds it and uses no skill of a player's that is not
//! on it; the player changes it out of combat (`Event::SetLoadout`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::AbilityType;

/// The skills a player holds, a slot each.
#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loadout(pub [AbilityType; 8]);

impl Loadout {
    /// What a new character holds
    pub const DEFAULT: Self = Self([
        AbilityType::Frenzy, AbilityType::Overpower, AbilityType::Punish, AbilityType::Backstab,
        AbilityType::Parry, AbilityType::Counter, AbilityType::Leap, AbilityType::PerfectStride,
    ]);

    /// Every skill a bar may hold, in the order [`AbilityType::ALL`] lists
    /// them: all but the auto-attack, which is no skill
    pub fn pool() -> impl Iterator<Item = AbilityType> {
        AbilityType::ALL.into_iter().filter(|&ability| ability != AbilityType::AutoAttack)
    }

    /// Whether `ability` is on it
    pub fn holds(&self, ability: AbilityType) -> bool {
        self.0.contains(&ability)
    }

    /// Whether every slot holds a skill of the pool, none twice
    pub fn fits(&self) -> bool {
        let pooled = self.0.iter().all(|&ability| ability != AbilityType::AutoAttack);
        let distinct = self.0.iter().enumerate().all(|(i, ability)| !self.0[..i].contains(ability));
        pooled && distinct
    }

    /// The bar with `ability` on slot `at`: where another slot held it, that
    /// slot takes what `at` held, so none is held twice
    pub fn placed(&self, ability: AbilityType, at: usize) -> Self {
        let mut bar = *self;
        if let Some(was) = self.0.iter().position(|&held| held == ability) {
            bar.0[was] = self.0[at];
        }
        bar.0[at] = ability;
        bar
    }
}

impl Default for Loadout {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_skill_fits_any_slot_but_none_twice_nor_the_auto_attack() {
        assert!(Loadout::DEFAULT.fits());
        let mut stances = Loadout::DEFAULT;
        stances.0[0] = AbilityType::Lurk;
        stances.0[4] = AbilityType::Vigil;
        assert!(stances.fits(), "stances on a strike's key and a reaction's");
        let mut twice = Loadout::DEFAULT;
        twice.0[1] = AbilityType::Frenzy;
        assert!(!twice.fits(), "one skill on two slots");
        let mut auto = Loadout::DEFAULT;
        auto.0[7] = AbilityType::AutoAttack;
        assert!(!auto.fits(), "the auto-attack is no skill");
    }

    #[test]
    fn placing_a_held_skill_swaps_it_with_the_slot_it_goes_to() {
        let fresh = Loadout::DEFAULT.placed(AbilityType::Kick, 2);
        assert_eq!(fresh.0[2], AbilityType::Kick);
        assert!(!fresh.holds(AbilityType::Punish), "a skill held nowhere takes the slot's place");
        let swapped = Loadout::DEFAULT.placed(AbilityType::Frenzy, 3);
        assert_eq!((swapped.0[0], swapped.0[3]), (AbilityType::Backstab, AbilityType::Frenzy));
        assert!(swapped.fits());
        assert_eq!(Loadout::DEFAULT.placed(AbilityType::Frenzy, 0), Loadout::DEFAULT, "on its own slot, nothing changes");
    }
}
