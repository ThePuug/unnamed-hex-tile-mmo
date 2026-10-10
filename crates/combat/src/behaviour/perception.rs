//! What an NPC knows of the fight, and how late it knows it.
//!
//! An NPC sees what a player in its place is shown and nothing more, and
//! each change reaches its decisions a reaction delay after it happens,
//! drawn afresh for each change from its [`Skill`]. Its own state it knows
//! at once, as a player knows their own bars. A threat it sees it judges
//! landing a little off when it does, as a player times a note by eye. A
//! change's delay and a threat's misjudgement are [`Dice`] rolls for the
//! change itself, so a threat seen once stays seen and judged alike, and
//! no draw needs keeping.

use std::{collections::VecDeque, hash::Hash, time::Duration};

use bevy::prelude::*;
use common_bevy::{components::reaction_queue::QueuedThreat, moment::Moment};

use super::skills::Foe;
use crate::dice::Dice;

/// How well an NPC carries out its decisions: how late each change
/// reaches it, how far its judgement strays, a random spread on every
/// score as a share of it, and how far either way it misjudges when a
/// threat lands.
#[derive(Clone, Component, Copy, Debug)]
pub struct Skill {
    pub fastest: Duration,
    pub slowest: Duration,
    pub error: f32,
    pub misjudge: Duration,
}

impl Skill {
    /// A quick human's reactions and a steady hand
    pub const SHARP: Skill = Skill {
        fastest: Duration::from_millis(170),
        slowest: Duration::from_millis(270),
        error: 0.05,
        misjudge: Duration::from_millis(50),
    };

    /// A steady player's reactions, often a little off
    pub const STEADY: Skill = Skill {
        fastest: Duration::from_millis(300),
        slowest: Duration::from_millis(450),
        error: 0.15,
        misjudge: Duration::from_millis(120),
    };

    /// A slow, careless one
    pub const SLOPPY: Skill = Skill {
        fastest: Duration::from_millis(500),
        slowest: Duration::from_millis(800),
        error: 0.3,
        misjudge: Duration::from_millis(250),
    };

    /// The skill `text` names: `sharp`, `steady` or `sloppy`, or
    /// `fastest-slowest/error/misjudge`, milliseconds, a share and
    /// milliseconds
    pub fn named(text: &str) -> Result<Skill, String> {
        match text {
            "sharp" => return Ok(Self::SHARP),
            "steady" => return Ok(Self::STEADY),
            "sloppy" => return Ok(Self::SLOPPY),
            _ => {}
        }
        let wrong = || format!("a skill is sharp, steady, sloppy or fastest-slowest/error/misjudge, not {text}");
        let (delays, rest) = text.split_once('/').ok_or_else(wrong)?;
        let (error, misjudge) = rest.split_once('/').ok_or_else(wrong)?;
        let (fastest, slowest) = delays.split_once('-').ok_or_else(wrong)?;
        let millis = |ms: &str| ms.parse::<u64>().map(Duration::from_millis).map_err(|_| wrong());
        Ok(Skill { fastest: millis(fastest)?, slowest: millis(slowest)?, error: error.parse().map_err(|_| wrong())?, misjudge: millis(misjudge)? })
    }

    /// The delay the change `key` reaches it after
    pub fn delay(&self, dice: &Dice, key: impl Hash) -> Duration {
        self.fastest + (self.slowest.saturating_sub(self.fastest)).mul_f32(dice.roll(("delay", key)).share())
    }

    /// Whether `threat`, in the queue of the NPC `ent`, has reached it by
    /// `now`, the game's clock
    pub fn sees(&self, dice: &Dice, ent: Entity, threat: &QueuedThreat, now: Moment) -> bool {
        threat.inserted_at + self.delay(dice, (ent, threat.source, threat.inserted_at)) <= now
    }

    /// When the NPC `ent` judges `threat`, in its queue, lands: off by up
    /// to its `misjudge` either way
    pub fn judged(&self, dice: &Dice, ent: Entity, threat: &QueuedThreat) -> Moment {
        let off = self.misjudge.mul_f32(dice.roll(("misjudge", ent, threat.source, threat.inserted_at)).share() * 2.0);
        threat.lands_at() + off - self.misjudge
    }
}

impl Default for Skill {
    fn default() -> Self {
        Self::SHARP
    }
}

/// What an NPC has seen of its target: each change as it stood when it
/// happened, newest last, back to the one it perceives; and the furthest
/// it has seen its target strike it from, which a new target starts over.
#[derive(Clone, Component, Debug, Default)]
pub struct Sight {
    seen: VecDeque<(Moment, Entity, Foe)>,
    reach: i32,
}

impl Sight {
    /// It has seen its target strike it from `distance` tiles off
    pub fn saw_strike(&mut self, distance: i32) {
        self.reach = self.reach.max(distance);
    }

    /// The furthest it has seen its target strike it from, 0 where it has
    /// seen none
    pub fn reach(&self) -> i32 {
        self.reach
    }

    /// Takes in how its target `target` stands at `now`, and returns how
    /// the NPC `ent` perceives it: the newest change whose delay has run.
    /// A new target is unseen until its first change reaches it.
    pub fn look(&mut self, dice: &Dice, ent: Entity, skill: &Skill, now: Moment, target: Option<(Entity, Foe)>) -> Option<Foe> {
        let Some((target, foe)) = target else {
            self.seen.clear();
            self.reach = 0;
            return None;
        };
        if self.seen.back().is_some_and(|&(_, seen, _)| seen != target) {
            self.seen.clear();
            self.reach = 0;
        }
        if self.seen.back().is_none_or(|&(_, _, last)| last != foe) {
            self.seen.push_back((now, target, foe));
        }
        let reached = |&(at, _, _): &(Moment, Entity, Foe)| at + skill.delay(dice, (ent, at)) <= now;
        let newest = self.seen.iter().rposition(reached)?;
        self.seen.drain(..newest);
        self.seen.front().map(|&(_, _, foe)| foe)
    }

    /// How it last perceived `target`, as [`Sight::look`] returned it;
    /// nothing while `target` is unseen
    pub fn seen(&self, target: Entity) -> Option<Foe> {
        self.seen.front().filter(|&&(_, seen, _)| seen == target).map(|&(_, _, foe)| foe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DICE: Dice = Dice::seeded(0);

    fn foe(distance: i32) -> Foe {
        Foe { at: Default::default(), heading: None, distance, health: 600.0, in_arc: true, flanked: false, patient: 0.0, since_skill: None, status: Default::default() }
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn at(millis: u64) -> Moment {
        Moment::from_millis(millis)
    }

    #[test]
    fn a_skill_is_named_or_spelled_out() {
        assert_eq!(Skill::named("steady").unwrap().error, Skill::STEADY.error);
        let spelled = Skill::named("200-300/0.1/80").unwrap();
        assert_eq!((spelled.fastest, spelled.slowest, spelled.error, spelled.misjudge), (ms(200), ms(300), 0.1, ms(80)));
        assert!(Skill::named("quick").is_err());
    }

    #[test]
    fn it_learns_how_far_its_target_strikes_from_and_a_new_target_starts_over() {
        let (ent, target, other) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let mut sight = Sight::default();
        sight.look(&DICE, ent, &Skill::SHARP, at(0), Some((target, foe(12))));
        sight.saw_strike(12);
        sight.saw_strike(4);
        assert_eq!(sight.reach(), 12, "the furthest it has seen");
        sight.look(&DICE, ent, &Skill::SHARP, at(10), Some((other, foe(3))));
        assert_eq!(sight.reach(), 0, "a new target is unknown");
    }

    #[test]
    fn every_delay_falls_within_its_skill() {
        let skill = Skill::SHARP;
        for key in 0..500 {
            let delay = skill.delay(&DICE, key);
            assert!(delay >= skill.fastest && delay <= skill.slowest);
        }
    }

    #[test]
    fn a_landing_is_misjudged_within_its_skill_either_way() {
        let ent = Entity::from_raw_u32(1).unwrap();
        let skill = Skill::STEADY;
        let judged: Vec<i64> = (0..500u64).map(|key| {
            let threat = QueuedThreat {
                source: Entity::from_raw_u32(2).unwrap(),
                damage: 10.0,
                inserted_at: at(key * 10),
                timer_duration: ms(3000),
                ability: None,
                dot: 0.0,
                ticked: 0,
                stride: 0.0,
            };
            let (judged, lands) = (skill.judged(&DICE, ent, &threat), threat.lands_at());
            judged.since(lands).as_millis() as i64 - lands.since(judged).as_millis() as i64
        }).collect();
        let reach = skill.misjudge.as_millis() as i64;
        assert!(judged.iter().all(|off| off.abs() <= reach));
        assert!(judged.iter().any(|&off| off < 0) && judged.iter().any(|&off| off > 0), "early and late both");
    }

    #[test]
    fn a_change_reaches_it_only_after_its_delay_and_the_newest_one_reached_wins() {
        let (ent, target) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let skill = Skill::SHARP;
        let mut sight = Sight::default();
        assert_eq!(sight.look(&DICE, ent, &skill, at(0), Some((target, foe(5)))), None, "nothing has reached it yet");
        assert_eq!(sight.look(&DICE, ent, &skill, at(300), Some((target, foe(4)))), Some(foe(5)), "the first change has, the second not");
        assert_eq!(sight.look(&DICE, ent, &skill, at(600), Some((target, foe(4)))), Some(foe(4)));
        let other = Entity::from_raw_u32(3).unwrap();
        assert_eq!(sight.look(&DICE, ent, &skill, at(610), Some((other, foe(1)))), None, "a new target starts unseen");
    }
}
