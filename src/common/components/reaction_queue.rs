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
    pub ability: Option<crate::common::message::AbilityType>,
}

/// Reaction queue component that holds incoming threats
/// - threats: Unbounded queue of incoming damage (oldest at front, newest at back)
/// - window_size: How many threats the player can see and interact with (derived from Focus)
///
/// ADR-030: Queue is unbounded. Window determines visibility, not capacity.
/// Threats behind the window still tick and resolve normally.
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

    /// Number of threats visible in the window (front of queue)
    pub fn visible_count(&self) -> usize {
        self.threats.len().min(self.window_size)
    }

    /// Number of threats behind the window (not yet visible)
    pub fn hidden_count(&self) -> usize {
        self.threats.len().saturating_sub(self.window_size)
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

    #[test]
    fn test_visible_and_hidden_counts() {
        let mut queue = ReactionQueue::new(2);
        let entity = Entity::from_raw_u32(0).unwrap();

        let make_threat = |damage: f32, secs: u64| QueuedThreat {
            source: entity,
            damage,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(1),
            ability: None,
        };

        // 1 threat, window=2: all visible
        queue.threats.push_back(make_threat(10.0, 0));
        assert_eq!(queue.visible_count(), 1);
        assert_eq!(queue.hidden_count(), 0);

        // 2 threats, window=2: all visible
        queue.threats.push_back(make_threat(15.0, 1));
        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.hidden_count(), 0);

        // 3 threats, window=2: 2 visible, 1 hidden
        queue.threats.push_back(make_threat(20.0, 2));
        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.hidden_count(), 1);

        // 5 threats, window=2: 2 visible, 3 hidden
        queue.threats.push_back(make_threat(25.0, 3));
        queue.threats.push_back(make_threat(30.0, 4));
        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.hidden_count(), 3);
    }
}
