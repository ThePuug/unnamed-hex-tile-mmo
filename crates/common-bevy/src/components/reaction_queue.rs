use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

/// A single threat in the reaction queue
/// Represents incoming damage that has not yet been applied

/// # CRITICAL INVARIANT (INV-003)
/// **DO NOT MANUALLY CONSTRUCT** this struct. ALWAYS use `queue::create_threat()`.
/// Manual construction bypasses threat timer consistency checks and will break
/// reaction window predictability.

/// Reading fields is fine. Construction must go through the helper.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct QueuedThreat {
    /// The entity that caused this threat (attacker)
    pub source: Entity,
    /// Base damage amount (before modifiers)
    pub damage: f32,
    /// Time when this threat was inserted (from Time::elapsed())
    pub inserted_at: Duration,
    /// How long this threat has before it resolves
    pub timer_duration: Duration,
    /// The ability that caused this threat, if any: how it shows and lands
    pub ability: Option<crate::message::AbilityType>,
    /// Damage each tick of the damage over time (DoT) this threat applies
    /// while it stands in the queue: a wound. Zero for a blow. A wound's DoT
    /// ticks once every [`DOT_TICK`] outside the queue, short of its landing,
    /// and the wound lands for the ticks it has not dealt, so clearing it
    /// stops the DoT and taking it takes the rest.
    pub dot: f32,
    /// DoT ticks this wound has dealt. Only the server counts them.
    pub ticked: u8,
    /// Share of its speed the target is slowed out of as the threat lands:
    /// Intimidation's bank struck back, binding the target. Zero for most threats;
    /// a reaction that clears the threat clears it too.
    pub bind: f32,
}

/// The lane a threat runs in, in the order the queue keeps them: every
/// blow ahead of every wound, every wound ahead of every auto-attack.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Lane {
    Blow,
    Wound,
    AutoAttack,
}

/// How often a wound's DoT ticks
pub const DOT_TICK: Duration = Duration::from_secs(1);

impl QueuedThreat {
    /// This threat dazing its target out of `bind` of its pace as it lands
    pub fn binding(self, bind: f32) -> Self {
        Self { bind, ..self }
    }

    /// A wound: its DoT ticks while it stands, queued behind every blow and
    /// ahead of every auto-attack, and shown in the window like a blow.
    pub fn is_wound(&self) -> bool {
        self.dot > 0.0
    }

    /// The ticks a wound's DoT deals in all: one per [`DOT_TICK`] short of
    /// its landing. None for a blow.
    pub fn tick_count(&self) -> u8 {
        if !self.is_wound() {
            return 0;
        }
        (self.timer_duration.as_nanos().saturating_sub(1) / DOT_TICK.as_nanos()).min(u8::MAX as u128) as u8
    }

    /// The ticks a wound has come due for by `now`.
    pub fn ticks_due(&self, now: Duration) -> u8 {
        let elapsed = now.saturating_sub(self.inserted_at).as_nanos() / DOT_TICK.as_nanos();
        elapsed.min(self.tick_count() as u128) as u8
    }

    /// What a wound's DoT has yet to deal: all it lands for. Nothing for a blow.
    pub fn dot_left(&self) -> f32 {
        self.dot * self.tick_count().saturating_sub(self.ticked) as f32
    }

    /// An auto-attack: steady pressure, queued behind every ability threat.
    pub fn is_pressure(&self) -> bool {
        self.ability == Some(crate::message::AbilityType::AutoAttack)
    }

    pub fn lane(&self) -> Lane {
        if self.is_pressure() {
            Lane::AutoAttack
        } else if self.is_wound() {
            Lane::Wound
        } else {
            Lane::Blow
        }
    }

    /// When the threat lands, on the clock of `inserted_at`
    pub fn lands_at(&self) -> Duration {
        self.inserted_at + self.timer_duration
    }
}

/// The threats on their way to an actor, none yet landed: in the order of
/// their [`Lane`], and within a lane the soonest to land first. Only
/// `queue::insert_threat` keeps that order. It holds any number, and the
/// actor sees every one.
#[derive(Clone, Component, Debug, Default, Deserialize, Serialize)]
pub struct ReactionQueue {
    pub threats: VecDeque<QueuedThreat>,
}

impl ReactionQueue {
    pub fn is_empty(&self) -> bool {
        self.threats.is_empty()
    }

    /// Whether a reaction reaching `span` behind the front threat takes
    /// `threat`: the front one, and every threat in any lane that lands
    /// within `span` after it does. One landing sooner than the front, an
    /// auto-attack ahead of a blow, is left, so a reaction reaches
    /// auto-attacks only as overflow.
    pub fn sweeps(&self, threat: &QueuedThreat, span: Duration) -> bool {
        self.threats.front().is_some_and(|front| {
            let (first, at) = (front.lands_at(), threat.lands_at());
            at >= first && at <= first + span
        })
    }

    /// The threats a reaction reaching `span` takes ([`ReactionQueue::sweeps`])
    pub fn swept(&self, span: Duration) -> impl Iterator<Item = &QueuedThreat> {
        self.threats.iter().filter(move |threat| self.sweeps(threat, span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threat(ability: Option<crate::message::AbilityType>, dot: f32) -> QueuedThreat {
        QueuedThreat {
            source: Entity::from_raw_u32(0).unwrap(),
            damage: 10.0,
            inserted_at: Duration::ZERO,
            timer_duration: Duration::from_secs(1),
            ability,
            dot,
            ticked: 0,
            bind: 0.0,
        }
    }

    #[test]
    fn a_reaction_takes_the_front_and_what_lands_within_its_span_behind_it() {
        use crate::message::AbilityType::{AutoAttack, Frenzy};
        let landing = |ability, secs: u64, millis: u64| QueuedThreat {
            timer_duration: Duration::from_secs(secs) + Duration::from_millis(millis),
            ..threat(Some(ability), 0.0)
        };
        // In queue order: the blows, then the auto-attacks
        let queue = ReactionQueue {
            threats: [landing(Frenzy, 3, 0), landing(Frenzy, 5, 0), landing(AutoAttack, 2, 0), landing(AutoAttack, 3, 200)].into(),
        };
        let taken = |span: u64| queue.swept(Duration::from_millis(span)).map(|t| t.timer_duration.as_millis()).collect::<Vec<_>>();
        assert_eq!(taken(0), vec![3000], "the front alone");
        assert_eq!(taken(250), vec![3000, 3200], "and an auto-attack landing just behind it, whatever its lane");
        assert_eq!(taken(2000), vec![3000, 5000, 3200], "a longer span reaches the next blow; the auto-attack landing sooner is left");
        assert_eq!(ReactionQueue::default().swept(Duration::from_secs(9)).count(), 0, "nothing queued, nothing taken");
    }

    #[test]
    fn lanes_order_blows_then_wounds_then_auto_attacks() {
        let blow = threat(Some(crate::message::AbilityType::Frenzy), 0.0);
        let wound = threat(Some(crate::message::AbilityType::Frenzy), 5.0);
        let auto = threat(Some(crate::message::AbilityType::AutoAttack), 0.0);
        assert!(blow.lane() < wound.lane() && wound.lane() < auto.lane());
    }
}
