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

/// The lane a threat shows in on the highway, by its kind. Lanes are for
/// reading; a reaction takes what is in its band whatever lane it runs in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    /// A wound: its DoT ticks while it stands.
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

    /// An auto-attack: steady pressure.
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

    /// Whether a reaction at `at` reaching `span` takes this threat: it
    /// lands at `at` or within `span` after it. The band sits at the hit
    /// line, so a reaction is timed to what is about to land.
    pub fn in_band(&self, at: Duration, span: Duration) -> bool {
        (at..=at + span).contains(&self.lands_at())
    }
}

/// The threats on their way to an actor, none yet landed, the soonest to
/// land first. Only `queue::insert_threat` keeps that order. It holds any
/// number, and the actor sees every one.
#[derive(Clone, Component, Debug, Default, Deserialize, Serialize)]
pub struct ReactionQueue {
    pub threats: VecDeque<QueuedThreat>,
}

impl ReactionQueue {
    pub fn is_empty(&self) -> bool {
        self.threats.is_empty()
    }

    /// The threats a reaction at `at` reaching `span` takes
    /// ([`QueuedThreat::in_band`])
    pub fn swept(&self, at: Duration, span: Duration) -> impl Iterator<Item = &QueuedThreat> {
        self.threats.iter().filter(move |threat| threat.in_band(at, span))
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
    fn a_reaction_takes_what_lands_within_its_band_whatever_its_lane() {
        use crate::message::AbilityType::{AutoAttack, Frenzy};
        let landing = |ability, millis: u64| QueuedThreat {
            timer_duration: Duration::from_millis(millis),
            ..threat(Some(ability), 0.0)
        };
        let queue = ReactionQueue {
            threats: [landing(AutoAttack, 2000), landing(Frenzy, 2100), landing(AutoAttack, 2300), landing(Frenzy, 3000)].into(),
        };
        let taken = |at: u64, span: u64| queue.swept(Duration::from_millis(at), Duration::from_millis(span)).map(|t| t.timer_duration.as_millis()).collect::<Vec<_>>();
        assert_eq!(taken(1900, 250), vec![2000, 2100], "an auto-attack as much as a blow");
        assert_eq!(taken(2050, 250), vec![2100, 2300], "what has landed is past the band");
        assert_eq!(taken(2050, 1000), vec![2100, 2300, 3000], "a wider band takes more");
        assert!(taken(1000, 250).is_empty(), "pressed early, it takes nothing");
        assert_eq!(ReactionQueue::default().swept(Duration::ZERO, Duration::from_secs(9)).count(), 0, "nothing queued, nothing taken");
    }
}
