//! Every roll combat makes — damage, crits, the foe an NPC takes up, how
//! far its judgement strays, how late it sees — comes from the [`Dice`]: a
//! hash of the world's seed and what the roll is for, so two worlds seeded
//! alike roll alike.
//!
//! A roll a fighter makes again and again is the next of its own sequence
//! for that purpose ([`Rolls`]): its fifth blow rolls its fifth damage roll
//! whenever it lands and whatever else has rolled meanwhile. Two fights
//! that part over one decision still share their dice after it, which is
//! what lets the arena weigh two settings on the same fights. A roll that
//! must come out the same each time it is asked — a change's perception
//! delay — is keyed by the change instead ([`Dice::roll`]).

use std::{collections::HashMap, hash::{DefaultHasher, Hash, Hasher}};

use bevy::prelude::*;

/// The world's seed. The live server seeds it from the OS.
#[derive(Resource, Clone, Copy)]
pub struct Dice(u64);

impl Default for Dice {
    fn default() -> Self {
        Self(rand::random())
    }
}

impl Dice {
    pub const fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    /// The roll for `key`, the same each time it is asked
    pub fn roll(&self, key: impl Hash) -> Roll {
        Roll((hash((self.0, key)) >> 40) as f32 / (1u64 << 24) as f32)
    }

    /// The next roll of the sequence `rolls` keeps for `key`. The key names
    /// its fighter as well as its purpose, or two fighters roll alike.
    pub fn draw(&self, rolls: &mut Rolls, key: impl Hash) -> Roll {
        let key = hash(key);
        let made = rolls.0.entry(key).or_default();
        *made += 1;
        self.roll((key, *made))
    }
}

fn hash(key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// How many rolls a fighter has made for each purpose. Every actor with
/// attributes carries one (`CombatPlugin`), and every NPC that chases
/// (`BehaviourPlugin`).
#[derive(Component, Clone, Debug, Default)]
pub struct Rolls(HashMap<u64, u32>);

/// A roll, a share in `0..1`
#[derive(Clone, Copy, Debug)]
pub struct Roll(f32);

impl Roll {
    pub fn share(self) -> f32 {
        self.0
    }

    /// Spread over `-1..1`
    pub fn signed(self) -> f32 {
        self.0 * 2.0 - 1.0
    }

    /// One of `count`, `count` past 0
    pub fn pick(self, count: usize) -> usize {
        ((self.0 * count as f32) as usize).min(count - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_rolls_the_same_and_a_sequence_moves_on() {
        let dice = Dice::seeded(3);
        assert_eq!(dice.roll("a").share(), dice.roll("a").share());
        let (mut one, mut other) = (Rolls::default(), Rolls::default());
        let first = dice.draw(&mut one, "spread").share();
        assert_ne!(first, dice.draw(&mut one, "spread").share(), "the next of the sequence");
        dice.draw(&mut other, "crit");
        assert_eq!(first, dice.draw(&mut other, "spread").share(), "another purpose's rolls never shift it");
    }
}
