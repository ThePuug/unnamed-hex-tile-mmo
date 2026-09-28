use crate::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::components::ActorAttributes;
use crate::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;

#[cfg(test)]
use crate::components::reaction_queue::DamageType;

/// Reaction window base from level gap.

/// Pattern 2 (Baseline+Bonus): 3.0s × gap × (1.0 + 0.5 × contest_factor)
/// This function computes the gap part only: 3.0s, less `window_per_level`
/// of it for each level the attacker stands above the defender, to nothing.
pub fn gap_window(defender_level: u32, attacker_level: u32, window_per_level: f32) -> Duration {
    let gap = attacker_level.saturating_sub(defender_level) as f32;
    Duration::from_secs_f32(3.0 * (1.0 - window_per_level * gap).max(0.0))
}

/// How long a threat from `source_attrs` waits in the queue of
/// `target_attrs` before it lands: the same for every threat between the
/// two (INV-003), whatever made it.
///
/// Two-step calculation:
/// 1. **Gap window**: Level difference sets the base window (3s at equal, less
///    `window_per_level` of it per level of gap, `WINDOW_PER_LEVEL` in the game)
/// 2. **Reaction contest**: Cunning advantage extends window (up to +50% at max advantage)
pub fn threat_window(target_attrs: &ActorAttributes, source_attrs: &ActorAttributes, window_per_level: f32) -> Duration {
    use crate::systems::combat::damage::reaction_contest_factor;

    // Step 1: Level gap sets the base window
    let base_window = gap_window(target_attrs.total_level(), source_attrs.total_level(), window_per_level);

    // Step 2: Cunning advantage (only improves, never reduces below base)
    let multiplier = reaction_contest_factor(target_attrs.cunning(), source_attrs.finesse());
    Duration::from_millis((base_window.as_secs_f32() * multiplier * 1000.0) as u64)
}

/// Create a threat with proper timer calculation (INVARIANT: INV-003)

/// **CRITICAL INVARIANT (INV-003):** All threats from the same source to the same target
/// MUST have identical timer durations, regardless of which ability created them.
/// This ensures consistent reaction windows and prevents ability-specific timing quirks.

/// The timer is [`threat_window`].

/// # Arguments
/// * `source` - Attacker entity (source of threat)
/// * `target_attrs` - Defender's attributes (receives threat)
/// * `source_attrs` - Attacker's attributes (creates threat)
/// * `damage` - Final damage amount
/// * `damage_type` - Physical or Magic
/// * `ability` - Which ability created this threat
/// * `now` - Current game time
/// * `dot` - Damage each DoT tick deals: a wound's, zero for a blow
/// * `window_per_level` - The share of its window an outleveled target loses per level of gap

/// # Returns
/// Fully-formed QueuedThreat with correct timer duration
pub fn create_threat(
    source: bevy::prelude::Entity,
    target_attrs: &ActorAttributes,
    source_attrs: &ActorAttributes,
    damage: f32,
    damage_type: crate::components::reaction_queue::DamageType,
    ability: Option<crate::message::AbilityType>,
    now: Duration,
    dot: f32,
    window_per_level: f32,
) -> crate::components::reaction_queue::QueuedThreat {
    crate::components::reaction_queue::QueuedThreat {
        source,
        damage,
        damage_type,
        inserted_at: now,
        timer_duration: threat_window(target_attrs, source_attrs, window_per_level),
        ability,
        dot,
        ticked: 0,
    }
}

/// Insert a threat into the queue (unbounded, no overflow eviction)
/// Queue is unbounded — threats always insert. Window size controls visibility only.
/// An auto-attack goes to the back; a wound after the last wound, ahead of
/// every auto-attack; a blow after the last blow, ahead of every wound, so
/// reactions reach wounds after blows and auto-attacks only as overflow. Server and client both insert through
/// here, so their queues hold the same order.
pub fn insert_threat(
    queue: &mut ReactionQueue,
    threat: crate::components::reaction_queue::QueuedThreat,
    _now: Duration,
) {
    let at = if threat.is_pressure() {
        queue.threats.len()
    } else if threat.is_wound() {
        queue.decision_count()
    } else {
        queue.blow_count()
    };
    queue.threats.insert(at, threat);
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
        ClearType::Threat { source, inserted_at } => queue
            .threats
            .iter()
            .position(|t| t.source == source && t.inserted_at == inserted_at)
            .and_then(|pos| queue.threats.remove(pos))
            .into_iter()
            .collect(),
    }
}

/// Sync reaction queue window size when attributes change

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
            dot: 0.0,
            ticked: 0,

        };

        // Insert always succeeds, no overflow
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
    fn test_insert_threat_puts_abilities_ahead_of_auto_attacks() {
        use crate::message::AbilityType::{AutoAttack, Lunge, Overpower};
        let mut queue = ReactionQueue::new(1);
        let entity = Entity::from_raw_u32(0).unwrap();
        let make_threat = |ability, secs: u64| QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(1),
            ability: Some(ability),
            dot: 0.0,
            ticked: 0,
        };

        for (ability, secs) in [(AutoAttack, 0), (Lunge, 1), (AutoAttack, 2), (Overpower, 3)] {
            insert_threat(&mut queue, make_threat(ability, secs), Duration::from_secs(secs));
        }

        let order: Vec<_> = queue.threats.iter().map(|t| (t.ability.unwrap(), t.inserted_at.as_secs())).collect();
        assert_eq!(order, vec![(Lunge, 1), (Overpower, 3), (AutoAttack, 0), (AutoAttack, 2)]);
        assert_eq!(queue.visible_count(), 1);
        assert_eq!(queue.hidden_count(), 1);
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
            dot: 0.0,
            ticked: 0,

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
            dot: 0.0,
            ticked: 0,

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
            dot: 0.0,
            ticked: 0,

        };

        // Threat 2: inserted at 0.5s, expires at 1.5s
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_millis(500),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,

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
            dot: 0.0,
            ticked: 0,
    
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
            dot: 0.0,
            ticked: 0,
    
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
    fn test_clear_threats_names_one_threat_wherever_it_stands() {
        let mut queue = ReactionQueue::new(3);
        let a = Entity::from_raw_u32(0).unwrap();
        let b = Entity::from_raw_u32(1).unwrap();
        for (source, secs) in [(a, 0), (b, 0), (a, 1)] {
            queue.threats.push_back(QueuedThreat {
                source,
                damage: 10.0,
                damage_type: DamageType::Physical,
                inserted_at: Duration::from_secs(secs),
                timer_duration: Duration::from_secs(1),
                ability: None,
                dot: 0.0,
                ticked: 0,
            });
        }

        let cleared = clear_threats(&mut queue, ClearType::Threat { source: b, inserted_at: Duration::from_secs(0) });
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].source, b);
        assert_eq!(queue.threats.iter().map(|t| (t.source, t.inserted_at.as_secs())).collect::<Vec<_>>(), vec![(a, 0), (a, 1)]);

        let missing = clear_threats(&mut queue, ClearType::Threat { source: b, inserted_at: Duration::from_secs(0) });
        assert!(missing.is_empty());
        assert_eq!(queue.threats.len(), 2);
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
            dot: 0.0,
            ticked: 0,

        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,

        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 20.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(2),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,

        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 25.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(3),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,

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

    #[test]
    fn wounds_queue_behind_blows_and_ahead_of_auto_attacks() {
        let mut queue = ReactionQueue::new(3);
        let source = Entity::from_raw_u32(0).unwrap();
        let make = |ability, dot: f32, secs| QueuedThreat {
            source,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(3),
            ability,
            dot,
            ticked: 0,
        };
        use crate::message::AbilityType::{AutoAttack, Lunge};
        insert_threat(&mut queue, make(Some(AutoAttack), 0.0, 0), Duration::ZERO);
        insert_threat(&mut queue, make(Some(Lunge), 5.0, 1), Duration::ZERO);
        insert_threat(&mut queue, make(Some(Lunge), 0.0, 2), Duration::ZERO);
        insert_threat(&mut queue, make(Some(Lunge), 5.0, 3), Duration::ZERO);
        insert_threat(&mut queue, make(Some(Lunge), 0.0, 4), Duration::ZERO);
        let order: Vec<_> = queue.threats.iter().map(|t| t.inserted_at.as_secs()).collect();
        assert_eq!(order, vec![2, 4, 1, 3, 0], "blows, then wounds, then auto-attacks, each oldest first");
        assert_eq!(queue.decision_count(), 4, "wounds are reached like blows");
    }

    #[test]
    fn a_wound_ticks_before_it_lands_and_lands_for_what_it_has_not_dealt() {
        let wound = QueuedThreat {
            source: Entity::from_raw_u32(0).unwrap(),
            damage: 0.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(10),
            timer_duration: Duration::from_secs(3),
            ability: None,
            dot: 5.0,
            ticked: 0,
        };
        assert_eq!(wound.tick_count(), 2, "ticks fall short of the landing");
        assert_eq!(wound.ticks_due(Duration::from_millis(10_999)), 0);
        assert_eq!(wound.ticks_due(Duration::from_secs(11)), 1);
        assert_eq!(wound.ticks_due(Duration::from_secs(20)), 2);
        assert_eq!(wound.dot_left(), 10.0, "taken at once, it deals every tick");
        assert_eq!(QueuedThreat { ticked: 2, ..wound }.dot_left(), 0.0, "ticked out, it lands for nothing");
        assert_eq!(QueuedThreat { dot: 0.0, ..wound }.tick_count(), 0, "a blow never ticks");
    }
}
