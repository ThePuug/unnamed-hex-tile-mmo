//! What an NPC knows of the fight, and how late it knows it.
//!
//! An NPC sees what a player in its place is shown and nothing more, and
//! each change reaches its decisions a reaction delay after it happens,
//! drawn afresh for each change from its [`Skill`]. Its own state it knows
//! at once, as a player knows their own bars. What a change's delay is
//! comes from hashing the change itself, so a threat seen once stays seen
//! and no draw needs keeping.

use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    time::Duration,
};

use bevy::prelude::*;
use common_bevy::components::reaction_queue::QueuedThreat;

use super::skills::Foe;

/// How well an NPC carries out its decisions: how late each change
/// reaches it, and how far its judgement strays, a random spread on every
/// score as a share of it.
#[derive(Clone, Component, Copy, Debug)]
pub struct Skill {
    pub fastest: Duration,
    pub slowest: Duration,
    pub error: f32,
}

impl Skill {
    /// A quick human's reactions and a steady hand
    pub const SHARP: Skill = Skill {
        fastest: Duration::from_millis(170),
        slowest: Duration::from_millis(270),
        error: 0.05,
    };

    /// The delay the change hashed from `key` reaches it after
    pub fn delay(&self, key: impl Hash) -> Duration {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let share = (hasher.finish() % 10_000) as f32 / 10_000.0;
        self.fastest + (self.slowest.saturating_sub(self.fastest)).mul_f32(share)
    }

    /// Whether `threat`, in the queue of the NPC `ent`, has reached it by
    /// `now`, the game's clock
    pub fn sees(&self, ent: Entity, threat: &QueuedThreat, now: Duration) -> bool {
        threat.inserted_at + self.delay((ent, threat.source, threat.inserted_at)) <= now
    }
}

impl Default for Skill {
    fn default() -> Self {
        Self::SHARP
    }
}

/// What an NPC has seen of its target: each change as it stood when it
/// happened, newest last, back to the one it perceives.
#[derive(Clone, Component, Debug, Default)]
pub struct Sight {
    seen: VecDeque<(Duration, Entity, Foe)>,
}

impl Sight {
    /// Takes in how its target `target` stands at `now`, and returns how
    /// the NPC `ent` perceives it: the newest change whose delay has run.
    /// A new target is unseen until its first change reaches it.
    pub fn look(&mut self, ent: Entity, skill: &Skill, now: Duration, target: Option<(Entity, Foe)>) -> Option<Foe> {
        let Some((target, foe)) = target else {
            self.seen.clear();
            return None;
        };
        if self.seen.back().is_some_and(|&(_, seen, _)| seen != target) {
            self.seen.clear();
        }
        if self.seen.back().is_none_or(|&(_, _, last)| last != foe) {
            self.seen.push_back((now, target, foe));
        }
        let reached = |&(at, _, _): &(Duration, Entity, Foe)| at + skill.delay((ent, at)) <= now;
        let newest = self.seen.iter().rposition(reached)?;
        self.seen.drain(..newest);
        self.seen.front().map(|&(_, _, foe)| foe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foe(distance: i32) -> Foe {
        Foe { distance, in_arc: true, across: false, recovering: 0.0 }
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn every_delay_falls_within_its_skill() {
        let skill = Skill::SHARP;
        for key in 0..500 {
            let delay = skill.delay(key);
            assert!(delay >= skill.fastest && delay <= skill.slowest);
        }
    }

    #[test]
    fn a_change_reaches_it_only_after_its_delay_and_the_newest_one_reached_wins() {
        let (ent, target) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let skill = Skill::SHARP;
        let mut sight = Sight::default();
        assert_eq!(sight.look(ent, &skill, ms(0), Some((target, foe(5)))), None, "nothing has reached it yet");
        assert_eq!(sight.look(ent, &skill, ms(300), Some((target, foe(4)))), Some(foe(5)), "the first change has, the second not");
        assert_eq!(sight.look(ent, &skill, ms(600), Some((target, foe(4)))), Some(foe(4)));
        let other = Entity::from_raw_u32(3).unwrap();
        assert_eq!(sight.look(ent, &skill, ms(610), Some((other, foe(1)))), None, "a new target starts unseen");
    }
}
