use crate::common::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::common::components::ActorAttributes;
use crate::common::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;

#[cfg(test)]
use crate::common::components::reaction_queue::DamageType;

/// Reaction window base from level gap.
///
/// Pattern 2 (Baseline+Bonus): 3.0s × gap × (1.0 + 0.5 × contest_factor)
/// This function computes the gap part only (3.0s × gap_factor).
pub fn gap_window(defender_level: u32, attacker_level: u32) -> Duration {
    use crate::common::systems::combat::damage::gap_factor;
    Duration::from_secs_f32(3.0 * gap_factor(defender_level, attacker_level))
}

/// Create a threat with proper timer calculation (INVARIANT: INV-003)
///
/// **CRITICAL INVARIANT (INV-003):** All threats from the same source to the same target
/// MUST have identical timer durations, regardless of which ability created them.
/// This ensures consistent reaction windows and prevents ability-specific timing quirks.
///
/// Two-step timer calculation:
/// 1. **Gap window**: Level difference sets the base window (3s at equal, ~1s at 10 gap, ~0 at 20)
/// 2. **Reaction contest**: Cunning advantage extends window (up to +50% at max advantage)
///
/// # Arguments
/// * `source` - Attacker entity (source of threat)
/// * `target_attrs` - Defender's attributes (receives threat)
/// * `source_attrs` - Attacker's attributes (creates threat)
/// * `damage` - Final damage amount
/// * `damage_type` - Physical or Magic
/// * `ability` - Which ability created this threat
/// * `now` - Current game time
///
/// # Returns
/// Fully-formed QueuedThreat with correct timer duration
pub fn create_threat(
    source: bevy::prelude::Entity,
    target_attrs: &ActorAttributes,
    source_attrs: &ActorAttributes,
    damage: f32,
    damage_type: crate::common::components::reaction_queue::DamageType,
    ability: Option<crate::common::message::AbilityType>,
    now: Duration,
) -> crate::common::components::reaction_queue::QueuedThreat {
    use crate::common::systems::combat::damage::reaction_contest_factor;

    // Step 1: Level gap sets the base window
    let base_window = gap_window(target_attrs.total_level(), source_attrs.total_level());

    // Step 2: Cunning advantage (only improves, never reduces below base)
    let multiplier = reaction_contest_factor(target_attrs.cunning(), source_attrs.finesse());
    let window_ms = (base_window.as_secs_f32() * multiplier * 1000.0) as u64;
    let timer_duration = Duration::from_millis(window_ms);

    crate::common::components::reaction_queue::QueuedThreat {
        source,
        damage,
        damage_type,
        inserted_at: now,
        timer_duration,
        ability,
    }
}

/// Insert a threat into the queue (ADR-030: unbounded, no overflow eviction)
/// Queue is unbounded — threats always insert. Window size controls visibility only.
pub fn insert_threat(
    queue: &mut ReactionQueue,
    threat: crate::common::components::reaction_queue::QueuedThreat,
    _now: Duration,
) {
    queue.threats.push_back(threat);
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
            // Drain first N threats (oldest)
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

/// Sync reaction queue window size when attributes change
///
/// This system ensures that ReactionQueue.window_size stays in sync with
/// ActorAttributes.window_size() after attribute changes (respecs, level ups, etc).
pub fn sync_queue_window_size(
    mut queue_query: Query<(&ActorAttributes, &mut ReactionQueue), Changed<ActorAttributes>>,
) {
    for (attrs, mut queue) in &mut queue_query {
        let new_window_size = attrs.window_size();
        if queue.window_size != new_window_size {
            queue.window_size = new_window_size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_threat_unbounded() {
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

        // Insert always succeeds, no overflow (ADR-030)
        insert_threat(&mut queue, make_threat(10.0, 0), Duration::from_secs(0));
        assert_eq!(queue.threats.len(), 1);

        insert_threat(&mut queue, make_threat(15.0, 1), Duration::from_secs(1));
        assert_eq!(queue.threats.len(), 2);

        // Beyond window_size: still inserts, just hidden
        insert_threat(&mut queue, make_threat(20.0, 2), Duration::from_secs(2));
        assert_eq!(queue.threats.len(), 3);
        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.hidden_count(), 1);

        // Insert more — all succeed
        insert_threat(&mut queue, make_threat(25.0, 3), Duration::from_secs(3));
        insert_threat(&mut queue, make_threat(30.0, 4), Duration::from_secs(4));
        assert_eq!(queue.threats.len(), 5);
        assert_eq!(queue.visible_count(), 2);
        assert_eq!(queue.hidden_count(), 3);
    }

    #[test]
    fn test_check_expired_threats_none_expired() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw_u32(0).unwrap();

        let threat = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,

        };

        queue.threats.push_back(threat);

        // Check at 0.5s - threat expires at 1.0s, so not expired yet
        let expired = check_expired_threats(&queue, Duration::from_millis(500));
        assert_eq!(expired.len(), 0);
        assert_eq!(queue.threats.len(), 1); // Threat still in queue
    }

    #[test]
    fn test_check_expired_threats_one_expired() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw_u32(0).unwrap();

        let threat = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,

        };

        queue.threats.push_back(threat.clone());

        // Check at 1.0s - threat should be expired
        let expired = check_expired_threats(&queue, Duration::from_secs(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].damage, 10.0);
        assert_eq!(queue.threats.len(), 1); // check_expired_threats doesn't remove
    }

    #[test]
    fn test_check_expired_threats_multiple() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw_u32(0).unwrap();

        // Threat 1: inserted at 0s, expires at 1s
        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,

        };

        // Threat 2: inserted at 0.5s, expires at 1.5s
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_millis(500),
            timer_duration: Duration::from_secs(1),
            ability: None,

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
        let entity = Entity::from_raw_u32(0).unwrap();

        // Add 3 threats
        for i in 0..3 {
            queue.threats.push_back(QueuedThreat {
                source: entity,
                damage: (i + 1) as f32 * 10.0,
                damage_type: DamageType::Physical,
                inserted_at: Duration::from_secs(i as u64),
                timer_duration: Duration::from_secs(1),
            ability: None,
    
            });
        }

        assert_eq!(queue.threats.len(), 3);

        // Clear all
        let cleared = clear_threats(&mut queue, ClearType::All);
        assert_eq!(cleared.len(), 3);
        assert_eq!(queue.threats.len(), 0);
    }

    #[test]
    fn test_clear_threats_first_n() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw_u32(0).unwrap();

        // Add 3 threats
        for i in 0..3 {
            queue.threats.push_back(QueuedThreat {
                source: entity,
                damage: (i + 1) as f32 * 10.0,
                damage_type: DamageType::Physical,
                inserted_at: Duration::from_secs(i as u64),
                timer_duration: Duration::from_secs(1),
            ability: None,
    
            });
        }

        // Clear first 2
        let cleared = clear_threats(&mut queue, ClearType::First(2));
        assert_eq!(cleared.len(), 2);
        assert_eq!(cleared[0].damage, 10.0); // First threat
        assert_eq!(cleared[1].damage, 20.0); // Second threat
        assert_eq!(queue.threats.len(), 1);
        assert_eq!(queue.threats[0].damage, 30.0); // Third threat remains
    }

    #[test]
    fn test_clear_threats_by_type() {
        let mut queue = ReactionQueue::new(4);
        let entity = Entity::from_raw_u32(0).unwrap();

        // Add mix of Physical and Magic threats
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,

        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
            ability: None,

        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 20.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(2),
            timer_duration: Duration::from_secs(1),
            ability: None,

        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 25.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(3),
            timer_duration: Duration::from_secs(1),
            ability: None,

        });

        assert_eq!(queue.threats.len(), 4);

        // Clear only Magic threats
        let cleared = clear_threats(&mut queue, ClearType::ByType(DamageType::Magic));
        assert_eq!(cleared.len(), 2);
        assert_eq!(cleared[0].damage, 15.0);
        assert_eq!(cleared[1].damage, 25.0);
        assert_eq!(queue.threats.len(), 2);
        assert_eq!(queue.threats[0].damage, 10.0); // Physical remains
        assert_eq!(queue.threats[1].damage, 20.0); // Physical remains
    }
}
