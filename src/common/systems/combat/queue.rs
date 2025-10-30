use crate::common::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::common::components::ActorAttributes;
use crate::common::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;

#[cfg(test)]
use crate::common::components::reaction_queue::DamageType;

/// Calculate queue capacity based on Focus attribute
/// Formula: base_capacity + floor(focus / 33.0)
/// - Focus = 0: 1 slot (base=1, bonus=0)
/// - Focus = 33: 2 slots (base=1, bonus=1)
/// - Focus = 66: 3 slots (base=1, bonus=2)
/// - Focus = 99: 4 slots (base=1, bonus=3)
/// - Focus = 150: 5 slots (base=1, bonus=4)
pub fn calculate_queue_capacity(attrs: &ActorAttributes) -> usize {
    // Use focus() which returns u8 (0-150 range)
    let focus = attrs.focus() as usize;

    let base_capacity = 1;
    let bonus = focus / 33;
    (base_capacity + bonus).min(10) // Cap at 10 for sanity
}

/// Calculate timer duration based on Instinct attribute
/// Formula: base_window * (1.0 + instinct / 200.0)
/// - Instinct = 0: 1.0s (base * 1.0)
/// - Instinct = 50: 1.25s (base * 1.25)
/// - Instinct = 100: 1.5s (base * 1.5)
/// - Instinct = 150: 1.75s (base * 1.75)
/// Minimum 250ms to prevent instant resolution
pub fn calculate_timer_duration(attrs: &ActorAttributes) -> Duration {
    // Use instinct() which returns u8 (0-150 range)
    let instinct = attrs.instinct() as f32;

    let base_window = 1.0;
    let multiplier = 1.0 + (instinct / 200.0); // 0: 1.0x, 100: 1.5x, 150: 1.75x
    let duration_secs = base_window * multiplier;

    Duration::from_secs_f32(duration_secs).max(Duration::from_millis(250))
}

/// Insert a threat into the queue
/// Returns Some(overflow_threat) if queue was full and oldest threat was evicted
/// Returns None if threat was inserted without overflow
pub fn insert_threat(
    queue: &mut ReactionQueue,
    threat: QueuedThreat,
    _now: Duration,
) -> Option<QueuedThreat> {
    if queue.is_full() {
        // Queue is full, pop oldest threat
        let overflow = queue.threats.pop_front();
        queue.threats.push_back(threat);
        overflow
    } else {
        // Queue has capacity, just push
        queue.threats.push_back(threat);
        None
    }
}

/// Check for expired threats in the queue
/// Returns a vector of threats that have expired (timer reached zero)
/// Does NOT remove threats from queue - caller decides when to remove
pub fn check_expired_threats(queue: &ReactionQueue, now: Duration) -> Vec<QueuedThreat> {
    queue
        .threats
        .iter()
        .filter(|threat| now >= threat.inserted_at + threat.timer_duration)
        .cloned()
        .collect()
}

/// Clear threats from the queue based on clear type
/// Returns the cleared threats for logging/effects
pub fn clear_threats(queue: &mut ReactionQueue, clear_type: ClearType) -> Vec<QueuedThreat> {
    match clear_type {
        ClearType::All => {
            // Drain entire queue
            queue.threats.drain(..).collect()
        }
        ClearType::First(n) => {
            // Drain first N threats
            let count = n.min(queue.threats.len());
            queue.threats.drain(..count).collect()
        }
        ClearType::ByType(damage_type) => {
            // Remove threats matching damage type
            let mut cleared = Vec::new();
            let mut i = 0;
            while i < queue.threats.len() {
                if queue.threats[i].damage_type == damage_type {
                    cleared.push(queue.threats.remove(i).unwrap());
                } else {
                    i += 1;
                }
            }
            cleared
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test attributes
    fn create_test_attrs(focus_axis: i8, instinct_axis: i8) -> ActorAttributes {
        ActorAttributes::new(
            0, 0, 0,          // might_grace: axis, spectrum, shift
            focus_axis, 0, 0, // vitality_focus: axis, spectrum, shift
            instinct_axis, 0, 0, // instinct_presence: axis, spectrum, shift
        )
    }

    #[test]
    fn test_calculate_queue_capacity_zero_focus() {
        // Focus = 0 (axis on vitality side) should give 1 slot (base=1, bonus=0)
        let attrs = create_test_attrs(-100, 0);
        assert_eq!(calculate_queue_capacity(&attrs), 1);
    }

    #[test]
    fn test_calculate_queue_capacity_low_focus() {
        // Focus = 33 (axis = 33 on focus side) should give 2 slots (base=1, bonus=1)
        let attrs = create_test_attrs(33, 0);
        assert_eq!(calculate_queue_capacity(&attrs), 2);
    }

    #[test]
    fn test_calculate_queue_capacity_mid_focus() {
        // Focus = 66 (axis = 66 on focus side) should give 3 slots (base=1, bonus=2)
        let attrs = create_test_attrs(66, 0);
        assert_eq!(calculate_queue_capacity(&attrs), 3);
    }

    #[test]
    fn test_calculate_queue_capacity_high_focus() {
        // Focus = 100 (axis = 100 on focus side) should give 4 slots (base=1, bonus=3)
        let attrs = create_test_attrs(100, 0);
        assert_eq!(calculate_queue_capacity(&attrs), 4);
    }

    #[test]
    fn test_calculate_timer_duration_zero_instinct() {
        // Instinct = 0 (axis on presence side) should give 1.0s
        let attrs = create_test_attrs(0, 100);
        let duration = calculate_timer_duration(&attrs);
        assert_eq!(duration, Duration::from_secs(1));
    }

    #[test]
    fn test_calculate_timer_duration_mid_instinct() {
        // Instinct = 50 (axis = -50 on instinct side) should give 1.25s
        let attrs = create_test_attrs(0, -50);
        let duration = calculate_timer_duration(&attrs);
        assert_eq!(duration, Duration::from_millis(1250));
    }

    #[test]
    fn test_calculate_timer_duration_high_instinct() {
        // Instinct = 100 (axis = -100 on instinct side) should give 1.5s
        let attrs = create_test_attrs(0, -100);
        let duration = calculate_timer_duration(&attrs);
        assert_eq!(duration, Duration::from_millis(1500));
    }

    #[test]
    fn test_insert_threat_with_capacity() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw(0);

        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
        };

        // Insert into empty queue - should not overflow
        let overflow = insert_threat(&mut queue, threat1.clone(), Duration::from_secs(0));
        assert!(overflow.is_none());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_insert_threat_overflow() {
        let mut queue = ReactionQueue::new(2);
        let entity = Entity::from_raw(0);

        // Fill queue to capacity
        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
        };
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
        };

        insert_threat(&mut queue, threat1.clone(), Duration::from_secs(0));
        insert_threat(&mut queue, threat2.clone(), Duration::from_secs(1));
        assert_eq!(queue.len(), 2);
        assert!(queue.is_full());

        // Insert third threat - should overflow and return oldest (threat1)
        let threat3 = QueuedThreat {
            source: entity,
            damage: 20.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(2),
            timer_duration: Duration::from_secs(1),
        };

        let overflow = insert_threat(&mut queue, threat3.clone(), Duration::from_secs(2));
        assert!(overflow.is_some());
        assert_eq!(overflow.unwrap().damage, 10.0); // threat1 was oldest
        assert_eq!(queue.len(), 2); // Still at capacity
        assert_eq!(queue.threats[0].damage, 15.0); // threat2 is now oldest
        assert_eq!(queue.threats[1].damage, 20.0); // threat3 is newest
    }

    #[test]
    fn test_check_expired_threats_none_expired() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw(0);

        let threat = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
        };

        queue.threats.push_back(threat);

        // Check at 0.5s - threat expires at 1.0s, so not expired yet
        let expired = check_expired_threats(&queue, Duration::from_millis(500));
        assert_eq!(expired.len(), 0);
        assert_eq!(queue.len(), 1); // Threat still in queue
    }

    #[test]
    fn test_check_expired_threats_one_expired() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw(0);

        let threat = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
        };

        queue.threats.push_back(threat.clone());

        // Check at 1.0s - threat should be expired
        let expired = check_expired_threats(&queue, Duration::from_secs(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].damage, 10.0);
        assert_eq!(queue.len(), 1); // check_expired_threats doesn't remove
    }

    #[test]
    fn test_check_expired_threats_multiple() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw(0);

        // Threat 1: inserted at 0s, expires at 1s
        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
        };

        // Threat 2: inserted at 0.5s, expires at 1.5s
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_millis(500),
            timer_duration: Duration::from_secs(1),
        };

        queue.threats.push_back(threat1);
        queue.threats.push_back(threat2);

        // Check at 1.0s - only threat1 expired
        let expired = check_expired_threats(&queue, Duration::from_secs(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].damage, 10.0);

        // Check at 1.5s - both expired
        let expired = check_expired_threats(&queue, Duration::from_millis(1500));
        assert_eq!(expired.len(), 2);
    }

    #[test]
    fn test_clear_threats_all() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw(0);

        // Add 3 threats
        for i in 0..3 {
            queue.threats.push_back(QueuedThreat {
                source: entity,
                damage: (i + 1) as f32 * 10.0,
                damage_type: DamageType::Physical,
                inserted_at: Duration::from_secs(i as u64),
                timer_duration: Duration::from_secs(1),
            });
        }

        assert_eq!(queue.len(), 3);

        // Clear all
        let cleared = clear_threats(&mut queue, ClearType::All);
        assert_eq!(cleared.len(), 3);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_clear_threats_first_n() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw(0);

        // Add 3 threats
        for i in 0..3 {
            queue.threats.push_back(QueuedThreat {
                source: entity,
                damage: (i + 1) as f32 * 10.0,
                damage_type: DamageType::Physical,
                inserted_at: Duration::from_secs(i as u64),
                timer_duration: Duration::from_secs(1),
            });
        }

        // Clear first 2
        let cleared = clear_threats(&mut queue, ClearType::First(2));
        assert_eq!(cleared.len(), 2);
        assert_eq!(cleared[0].damage, 10.0); // First threat
        assert_eq!(cleared[1].damage, 20.0); // Second threat
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.threats[0].damage, 30.0); // Third threat remains
    }

    #[test]
    fn test_clear_threats_by_type() {
        let mut queue = ReactionQueue::new(4);
        let entity = Entity::from_raw(0);

        // Add mix of Physical and Magic threats
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 20.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(2),
            timer_duration: Duration::from_secs(1),
        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 25.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(3),
            timer_duration: Duration::from_secs(1),
        });

        assert_eq!(queue.len(), 4);

        // Clear only Magic threats
        let cleared = clear_threats(&mut queue, ClearType::ByType(DamageType::Magic));
        assert_eq!(cleared.len(), 2);
        assert_eq!(cleared[0].damage, 15.0);
        assert_eq!(cleared[1].damage, 25.0);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.threats[0].damage, 10.0); // Physical remains
        assert_eq!(queue.threats[1].damage, 20.0); // Physical remains
    }
}
