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
/// - threats: Queue of incoming damage (oldest at front, newest at back)
/// - capacity: Maximum number of threats that can be queued (derived from Focus attribute)
#[derive(Clone, Component, Debug, Default, Deserialize, Serialize)]
pub struct ReactionQueue {
    pub threats: VecDeque<QueuedThreat>,
    pub capacity: usize,
}

impl ReactionQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            threats: VecDeque::new(),
            capacity,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.threats.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.threats.len() >= self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reaction_queue_new() {
        let queue = ReactionQueue::new(3);
        assert_eq!(queue.capacity, 3);
        assert_eq!(queue.threats.len(), 0);
        assert!(queue.is_empty());
        assert!(!queue.is_full());
    }

    #[test]
    fn test_reaction_queue_is_full() {
        let mut queue = ReactionQueue::new(2);

        // Create dummy entity for testing
        let entity = Entity::from_raw_u32(0).unwrap();

        // Add first threat
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
        });
        assert_eq!(queue.threats.len(), 1);
        assert!(!queue.is_full());

        // Add second threat
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
            ability: None,
        });
        assert_eq!(queue.threats.len(), 2);
        assert!(queue.is_full());
    }
}
