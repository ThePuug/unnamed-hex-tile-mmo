use crate::common::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::common::components::{ActorAttributes, CommitmentTier};
use crate::common::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;

#[cfg(test)]
use crate::common::components::reaction_queue::DamageType;

/// Calculate queue capacity from Focus commitment tier.
///
/// T0 → 1 slot, T1 → 2 slots, T2 → 3 slots, T3 → 4 slots.
pub fn calculate_queue_capacity(attrs: &ActorAttributes) -> usize {
    match attrs.commitment_tier_for(attrs.focus()) {
        CommitmentTier::T0 => 1,
        CommitmentTier::T1 => 2,
        CommitmentTier::T2 => 3,
        CommitmentTier::T3 => 4,
    }
}

/// Calculate auto-attack interval from Presence commitment tier.
///
/// Higher presence commitment → faster attacks.
/// T0 → 2000ms, T1 → 1500ms, T2 → 1000ms, T3 → 750ms.
pub fn cadence_interval(attrs: &ActorAttributes) -> Duration {
    match attrs.commitment_tier_for(attrs.presence()) {
        CommitmentTier::T0 => Duration::from_millis(2000),
        CommitmentTier::T1 => Duration::from_millis(1500),
        CommitmentTier::T2 => Duration::from_millis(1000),
        CommitmentTier::T3 => Duration::from_millis(750),
    }
}

/// Calculate evasion (dodge) chance from Grace commitment tier.
///
/// Higher grace commitment → higher dodge chance at threat insertion.
/// T0 → 0%, T1 → 10%, T2 → 20%, T3 → 30%.
pub fn evasion_chance(attrs: &ActorAttributes) -> f32 {
    match attrs.commitment_tier_for(attrs.grace()) {
        CommitmentTier::T0 => 0.0,
        CommitmentTier::T1 => 0.10,
        CommitmentTier::T2 => 0.20,
        CommitmentTier::T3 => 0.30,
    }
}

/// Calculate timer duration based on Instinct attribute and level multiplier (ADR-020)
/// Linear formula: base_window * (1.0 + instinct / 1000.0)
/// Then scaled by reaction_level_multiplier for super-linear growth
///
/// Minimum 250ms to prevent instant resolution
pub fn calculate_timer_duration(attrs: &ActorAttributes) -> Duration {
    let instinct = attrs.instinct() as f32;

    let base_window = 1.0;
    let linear = base_window * (1.0 + instinct / 1000.0);
    let duration_secs = linear * attrs.reaction_level_multiplier();

    Duration::from_secs_f32(duration_secs).max(Duration::from_millis(250))
}

/// Calculate reaction window multiplier based on level gap (ADR-020)
/// Higher-level defenders get more time to react to lower-level threats
/// No penalty when outleveled (floor at 1.0), capped at max multiplier
pub fn gap_multiplier(defender_level: u32, attacker_level: u32) -> f32 {
    let gap = defender_level.saturating_sub(attacker_level) as f32;
    const WINDOW_SCALING_FACTOR: f32 = 0.15;
    const WINDOW_MAX_MULTIPLIER: f32 = 3.0;
    (1.0 + gap * WINDOW_SCALING_FACTOR).min(WINDOW_MAX_MULTIPLIER)
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
            // Drain first N threats (oldest)
            let count = n.min(queue.threats.len());
            queue.threats.drain(..count).collect()
        }
        ClearType::Last(n) => {
            // Drain last N threats (newest) - for reactive abilities like Knockback
            let len = queue.threats.len();
            let count = n.min(len);
            let start = len.saturating_sub(count);
            queue.threats.drain(start..).collect()
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

    #[test]
    fn test_queue_capacity_zero_investment() {
        // No investment → total_budget=0 → T0 → 1 slot
        let attrs = ActorAttributes::default();
        assert_eq!(calculate_queue_capacity(&attrs), 1);
    }

    #[test]
    fn test_queue_capacity_full_focus_commitment() {
        // All investment in focus → focus/total_budget = 100% → T3 → 4 slots
        let attrs = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        assert_eq!(calculate_queue_capacity(&attrs), 4);
    }

    #[test]
    fn test_queue_capacity_tier_boundaries() {
        // Use CommitmentTier thresholds: T0 <30%, T1 ≥30%, T2 ≥45%, T3 ≥60%
        // total_budget = sum of all 6 derived values

        // 29% → T0 → 1 slot
        // might=-6 (might=60), focus=3 (focus=30) → budget=90, focus/budget=33% → T1
        // Need <30%: might=-7 (70), focus=2 (20) → budget=90, 20/90=22% → T0
        let t0 = ActorAttributes::new(-7, 0, 0, 2, 0, 0, 0, 0, 0);
        assert_eq!(calculate_queue_capacity(&t0), 1, "T0 should give 1 slot");

        // 30% → T1 → 2 slots
        // might=-6 (60), focus=3 (30) → budget=90, 30/90=33% → T1
        let t1 = ActorAttributes::new(-6, 0, 0, 3, 0, 0, 0, 0, 0);
        assert_eq!(calculate_queue_capacity(&t1), 2, "T1 should give 2 slots");

        // 45% → T2 → 3 slots
        // might=-5 (50), focus=5 (50) → budget=100, 50/100=50% → T2
        let t2 = ActorAttributes::new(-5, 0, 0, 5, 0, 0, 0, 0, 0);
        assert_eq!(calculate_queue_capacity(&t2), 3, "T2 should give 3 slots");

        // 60% → T3 → 4 slots
        // might=-3 (30), focus=6 (60) → budget=90, 60/90=67% → T3
        let t3 = ActorAttributes::new(-3, 0, 0, 6, 0, 0, 0, 0, 0);
        assert_eq!(calculate_queue_capacity(&t3), 4, "T3 should give 4 slots");
    }

    // ===== CADENCE INTERVAL TESTS =====

    #[test]
    fn test_cadence_interval_tiers() {
        // T0: no presence investment → 2000ms
        let t0 = ActorAttributes::default();
        assert_eq!(cadence_interval(&t0), Duration::from_millis(2000));

        // T1: presence ~33% of budget → 1500ms
        // might=-6 (60), presence=3 (30) → budget=90, 30/90=33% → T1
        let t1 = ActorAttributes::new(-6, 0, 0, 0, 0, 0, 3, 0, 0);
        assert_eq!(cadence_interval(&t1), Duration::from_millis(1500));

        // T2: presence ~50% of budget → 1000ms
        let t2 = ActorAttributes::new(-5, 0, 0, 0, 0, 0, 5, 0, 0);
        assert_eq!(cadence_interval(&t2), Duration::from_millis(1000));

        // T3: presence 100% of budget → 750ms
        let t3 = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        assert_eq!(cadence_interval(&t3), Duration::from_millis(750));
    }

    #[test]
    fn test_cadence_monotonically_decreasing() {
        // Higher tier → shorter interval
        let t0 = cadence_interval(&ActorAttributes::default());
        let t1 = cadence_interval(&ActorAttributes::new(-6, 0, 0, 0, 0, 0, 3, 0, 0));
        let t2 = cadence_interval(&ActorAttributes::new(-5, 0, 0, 0, 0, 0, 5, 0, 0));
        let t3 = cadence_interval(&ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0));
        assert!(t0 > t1, "T0 should be slower than T1");
        assert!(t1 > t2, "T1 should be slower than T2");
        assert!(t2 > t3, "T2 should be slower than T3");
    }

    // ===== EVASION CHANCE TESTS =====

    #[test]
    fn test_evasion_chance_tiers() {
        // T0: no grace investment → 0%
        let t0 = ActorAttributes::default();
        assert_eq!(evasion_chance(&t0), 0.0);

        // T1: grace ~33% of budget → 10%
        // vitality=-6 (60), grace=3 (30) → budget=90, 30/90=33% → T1
        let t1 = ActorAttributes::new(3, 0, 0, -6, 0, 0, 0, 0, 0);
        assert_eq!(evasion_chance(&t1), 0.10);

        // T2: grace ~50% of budget → 20%
        let t2 = ActorAttributes::new(5, 0, 0, -5, 0, 0, 0, 0, 0);
        assert_eq!(evasion_chance(&t2), 0.20);

        // T3: grace 100% of budget → 30%
        let t3 = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(evasion_chance(&t3), 0.30);
    }

    #[test]
    fn test_evasion_monotonically_increasing() {
        // Higher tier → higher dodge chance
        let t0 = evasion_chance(&ActorAttributes::default());
        let t1 = evasion_chance(&ActorAttributes::new(3, 0, 0, -6, 0, 0, 0, 0, 0));
        let t2 = evasion_chance(&ActorAttributes::new(5, 0, 0, -5, 0, 0, 0, 0, 0));
        let t3 = evasion_chance(&ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0));
        assert!(t0 < t1, "T0 should have less evasion than T1");
        assert!(t1 < t2, "T1 should have less evasion than T2");
        assert!(t2 < t3, "T2 should have less evasion than T3");
    }

    #[test]
    fn test_timer_duration_minimum_floor() {
        // Timer duration should never drop below 250ms regardless of attributes
        let attrs = ActorAttributes::default();
        let duration = calculate_timer_duration(&attrs);
        assert!(
            duration >= Duration::from_millis(250),
            "Timer duration must be >= 250ms, got {:?}",
            duration
        );
    }

    #[test]
    fn test_timer_duration_increases_with_instinct() {
        // Higher instinct should give longer reaction windows
        let low_instinct = ActorAttributes::default(); // instinct 0, level 0
        let high_instinct = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0); // instinct investment
        assert!(
            calculate_timer_duration(&high_instinct) > calculate_timer_duration(&low_instinct),
            "Higher instinct should produce longer timer duration"
        );
    }

    #[test]
    fn test_timer_duration_level_zero_is_base() {
        // Level 0 entity: reaction multiplier = 1.0, so duration = base window
        let attrs = ActorAttributes::default();
        assert_eq!(attrs.total_level(), 0);
        let duration = calculate_timer_duration(&attrs);
        // Base window with zero instinct and level multiplier 1.0 should be exactly 1.0s
        assert_eq!(duration, Duration::from_secs(1));
    }

    // ===== GAP MULTIPLIER TESTS (ADR-020 Phase 3) =====
    // Property tests only — no specific formula values, survives balance tuning

    #[test]
    fn test_gap_multiplier_equal_levels() {
        // Equal levels should give exactly 1.0 (no bonus or penalty)
        assert_eq!(gap_multiplier(0, 0), 1.0);
        assert_eq!(gap_multiplier(5, 5), 1.0);
        assert_eq!(gap_multiplier(50, 50), 1.0);
    }

    #[test]
    fn test_gap_multiplier_no_penalty_when_outleveled() {
        // Fighting higher-level enemies should never reduce the window
        assert_eq!(gap_multiplier(0, 10), 1.0);
        assert_eq!(gap_multiplier(5, 20), 1.0);
    }

    #[test]
    fn test_gap_multiplier_increases_with_gap() {
        // Larger level gap should give larger multiplier
        let small_gap = gap_multiplier(10, 5);
        let large_gap = gap_multiplier(20, 5);
        assert!(small_gap > 1.0, "Positive gap should give > 1.0");
        assert!(large_gap > small_gap, "Larger gap should give bigger multiplier");
    }

    #[test]
    fn test_gap_multiplier_has_ceiling() {
        // Very large gaps should be capped
        let huge_gap = gap_multiplier(100, 0);
        assert!(huge_gap <= 3.0, "Gap multiplier should be capped, got {}", huge_gap);
    }

    #[test]
    fn test_insert_threat_with_capacity() {
        let mut queue = ReactionQueue::new(3);
        let entity = Entity::from_raw_u32(0).unwrap();

        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
        };

        // Insert into empty queue - should not overflow
        let overflow = insert_threat(&mut queue, threat1.clone(), Duration::from_secs(0));
        assert!(overflow.is_none());
        assert_eq!(queue.threats.len(), 1);
    }

    #[test]
    fn test_insert_threat_overflow() {
        let mut queue = ReactionQueue::new(2);
        let entity = Entity::from_raw_u32(0).unwrap();

        // Fill queue to capacity
        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
        };
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
            ability: None,
        };

        insert_threat(&mut queue, threat1.clone(), Duration::from_secs(0));
        insert_threat(&mut queue, threat2.clone(), Duration::from_secs(1));
        assert_eq!(queue.threats.len(), 2);
        assert!(queue.is_full());

        // Insert third threat - should overflow and return oldest (threat1)
        let threat3 = QueuedThreat {
            source: entity,
            damage: 20.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(2),
            timer_duration: Duration::from_secs(1),
            ability: None,
        };

        let overflow = insert_threat(&mut queue, threat3.clone(), Duration::from_secs(2));
        assert!(overflow.is_some());
        assert_eq!(overflow.unwrap().damage, 10.0); // threat1 was oldest
        assert_eq!(queue.threats.len(), 2); // Still at capacity
        assert_eq!(queue.threats[0].damage, 15.0); // threat2 is now oldest
        assert_eq!(queue.threats[1].damage, 20.0); // threat3 is newest
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
