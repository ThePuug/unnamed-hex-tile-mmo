use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

/// Damage type enumeration for threats in the reaction queue
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DamageType {
    Physical,
    Magic,
}

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
    /// Type of damage (Physical or Magic)
    pub damage_type: DamageType,
    /// Time when this threat was inserted (from Time::elapsed())
    pub inserted_at: Duration,
    /// How long this threat has before it resolves
    pub timer_duration: Duration,
    /// Optional ability that caused this threat (for visual effects/telegraphs)
    pub ability: Option<crate::message::AbilityType>,
    /// Damage each tick of the damage over time (DoT) this threat applies
    /// while it stands in the queue: a wound. Zero for a blow. A wound's DoT
    /// ticks once every [`DOT_TICK`] outside the queue, short of its landing,
    /// and the wound lands for the ticks it has not dealt, so clearing it
    /// stops the DoT and taking it takes the rest.
    pub dot: f32,
    /// DoT ticks this wound has dealt. Only the server counts them.
    pub ticked: u8,
    /// Whether the threat has stood in its target's window. Once seen it
    /// stays visible, however far back what comes after pushes it; only
    /// [`ReactionQueue::reveal`] sets it.
    pub seen: bool,
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

/// Reaction queue component that holds incoming threats
/// - threats: Unbounded queue of incoming damage, ordered by [`Lane`], then
///   soonest to land first within a lane; only `queue::insert_threat` keeps
///   that order.
/// - window_size: How many threats from the front the player sees (derived
///   from Focus)

/// Queue is unbounded. Window determines visibility, not capacity: the front
/// `window_size` threats are seen, and stay seen. Threats never seen still
/// tick and resolve normally.
#[derive(Clone, Component, Debug, Default, Deserialize, Serialize)]
pub struct ReactionQueue {
    pub threats: VecDeque<QueuedThreat>,
    pub window_size: usize,
}

impl ReactionQueue {
    pub fn new(window_size: usize) -> Self {
        Self {
            threats: VecDeque::new(),
            window_size,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.threats.is_empty()
    }

    /// Mark the front `window_size` threats seen. Every change to the queue
    /// or its window calls it, so whatever reaches the window is seen.
    pub fn reveal(&mut self) {
        let window = self.window_size;
        for threat in self.threats.iter_mut().take(window) {
            threat.seen = true;
        }
    }

    /// Number of threats seen
    pub fn visible_count(&self) -> usize {
        self.threats.iter().filter(|t| t.seen).count()
    }

    /// Number of threats never seen
    pub fn hidden_count(&self) -> usize {
        self.threats.len() - self.visible_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reaction_queue_new() {
        let queue = ReactionQueue::new(3);
        assert_eq!(queue.window_size, 3);
        assert_eq!(queue.threats.len(), 0);
        assert!(queue.is_empty());
        assert_eq!(queue.visible_count(), 0);
        assert_eq!(queue.hidden_count(), 0);
    }

    fn threat(ability: Option<crate::message::AbilityType>, dot: f32) -> QueuedThreat {
        QueuedThreat {
            source: Entity::from_raw_u32(0).unwrap(),
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::ZERO,
            timer_duration: Duration::from_secs(1),
            ability,
            dot,
            ticked: 0,
            seen: false,
        }
    }

    #[test]
    fn the_window_reveals_the_front() {
        let mut queue = ReactionQueue::new(2);
        for _ in 0..5 {
            queue.threats.push_back(threat(None, 0.0));
        }
        queue.reveal();
        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.hidden_count(), 3);
        assert!(queue.threats[0].seen && queue.threats[1].seen && !queue.threats[2].seen);
    }

    #[test]
    fn auto_attacks_are_seen_like_any_threat() {
        let mut queue = ReactionQueue::new(2);
        queue.threats.push_back(threat(Some(crate::message::AbilityType::AutoAttack), 0.0));
        queue.reveal();
        assert_eq!(queue.visible_count(), 1);
    }

    #[test]
    fn a_seen_threat_stays_seen_when_pushed_back() {
        let mut queue = ReactionQueue::new(1);
        queue.threats.push_back(threat(None, 0.0));
        queue.reveal();
        queue.threats.push_front(threat(None, 0.0));
        queue.reveal();
        assert!(queue.threats[1].seen, "pushed out of the window, still seen");
        assert_eq!(queue.visible_count(), 2);
    }

    #[test]
    fn lanes_order_blows_then_wounds_then_auto_attacks() {
        let blow = threat(Some(crate::message::AbilityType::Lunge), 0.0);
        let wound = threat(Some(crate::message::AbilityType::Lunge), 5.0);
        let auto = threat(Some(crate::message::AbilityType::AutoAttack), 0.0);
        assert!(blow.lane() < wound.lane() && wound.lane() < auto.lane());
    }
}
