use crate::common::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::common::components::ActorAttributes;
use crate::common::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;

#[cfg(test)]
use crate::common::components::reaction_queue::DamageType;

/// Base timer constant (flat, no attribute scaling)
/// Attribute scaling handled by cunning_extension, level scaling by gap_multiplier.
/// This is the baseline reaction window before any modifiers (3 seconds).
/// Gap and cunning can extend this up to 10 seconds total.
pub fn base_timer() -> Duration {
    Duration::from_secs_f32(3.0)
}

/// Calculate reaction window bonus from level gap (no individual cap)
/// Uses INVERSE of cunning's sqrt formula (squared) for symmetric diminishing returns.
/// Combined with cunning_extension, the total bonus is capped at 7s.
///
/// - At equal levels: 0s bonus
/// - At extreme gaps: ~5-6s bonus (before combined cap)
/// - No penalty when outleveled (floor at 0)
pub fn gap_bonus(defender_level: u32, attacker_level: u32) -> Duration {
    let gap = defender_level.saturating_sub(attacker_level) as f32;
    const GAP_NORMALIZER: f32 = 50.0;  // 50-level gap ≈ max effect

    // Squared for inverse sqrt curve (fast growth that slows)
    // Scale to give ~6s at max gap (before combined cap)
    let normalized = (gap / GAP_NORMALIZER).min(1.0);
    let bonus_secs = normalized.powi(2) * 6.0;

    Duration::from_secs_f32(bonus_secs)
}

/// Calculate Cunning-based reaction window bonus (no individual cap)
/// Defender's cunning extends the time available to react to threats.
/// Weaker than gap_bonus - cunning is a secondary advantage.
/// Combined with gap_bonus, the total bonus is capped at 7s.
///
/// Formula: scales to give ~3s at max cunning (before combined cap)
pub fn cunning_bonus(cunning: u16, attacker_finesse: u16) -> Duration {
    use crate::common::systems::combat::damage::contest_modifier;

    const MS_PER_CUNNING: f32 = 10.0; // 10ms per cunning point (weaker than level gap)

    // Calculate base extension from raw stat
    let base_extension_ms = (cunning as f32) * MS_PER_CUNNING;

    // Apply contest modifier directly to benefit
    let contest_mod = contest_modifier(cunning, attacker_finesse);
    let contested_extension_ms = base_extension_ms * contest_mod;

    Duration::from_secs_f32(contested_extension_ms / 1000.0)
}

/// Create a threat with proper timer calculation (INVARIANT: INV-003)
///
/// **CRITICAL INVARIANT (INV-003):** All threats from the same source to the same target
/// MUST have identical timer durations, regardless of which ability created them.
/// This ensures consistent reaction windows and prevents ability-specific timing quirks.
///
/// Timer calculation (2 independent components):
/// 1. **Level scaling**: base × gap_multiplier(level_diff) — absolute power difference
/// 2. **Stat scaling**: cunning_extension(cunning vs finesse) — relative stat contest
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
    // Two-component timer calculation with combined cap (INV-003)
    // Base: 3s flat
    // Bonuses: gap + cunning, capped at 7s combined
    // Final range: 3s to 10s

    let gap_bonus = gap_bonus(target_attrs.total_level(), source_attrs.total_level());
    let cunning_bonus = cunning_bonus(target_attrs.cunning(), source_attrs.finesse());

    // Cap the combined bonuses at 7 seconds
    const MAX_COMBINED_BONUS_MS: u128 = 7000;
    let total_bonus_ms = (gap_bonus.as_millis() + cunning_bonus.as_millis())
        .min(MAX_COMBINED_BONUS_MS);

    let timer_duration = base_timer() + Duration::from_millis(total_bonus_ms as u64);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_size_zero_investment() {
        // No investment → total_budget=0 → T0 → 1 slot
        let attrs = ActorAttributes::default();
        assert_eq!(attrs.window_size(), 1);
    }

    #[test]
    fn test_window_size_full_focus_commitment() {
        // All investment in focus → focus/total_budget = 100% → T3 → 4 slots
        let attrs = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        assert_eq!(attrs.window_size(), 4);
    }

    #[test]
    fn test_window_size_tier_boundaries() {
        // Use CommitmentTier thresholds: T0 <30%, T1 ≥30%, T2 ≥45%, T3 ≥60%
        // total_budget = sum of all 6 derived values

        // 29% → T0 → 1 slot
        // might=-7 (70), focus=2 (20) → budget=90, 20/90=22% → T0
        let t0 = ActorAttributes::new(-7, 0, 0, 2, 0, 0, 0, 0, 0);
        assert_eq!(t0.window_size(), 1, "T0 should give 1 slot");

        // 30% → T1 → 2 slots
        // might=-6 (60), focus=3 (30) → budget=90, 30/90=33% → T1
        let t1 = ActorAttributes::new(-6, 0, 0, 3, 0, 0, 0, 0, 0);
        assert_eq!(t1.window_size(), 2, "T1 should give 2 slots");

        // 45% → T2 → 3 slots
        // might=-5 (50), focus=5 (50) → budget=100, 50/100=50% → T2
        let t2 = ActorAttributes::new(-5, 0, 0, 5, 0, 0, 0, 0, 0);
        assert_eq!(t2.window_size(), 3, "T2 should give 3 slots");

        // 60% → T3 → 4 slots
        // might=-3 (30), focus=6 (60) → budget=90, 60/90=67% → T3
        let t3 = ActorAttributes::new(-3, 0, 0, 6, 0, 0, 0, 0, 0);
        assert_eq!(t3.window_size(), 4, "T3 should give 4 slots");
    }

    // ===== CADENCE INTERVAL TESTS =====

    #[test]
    fn test_cadence_interval_tiers() {
        // All builds use 10 points total so max_possible = 100

        // T0: no presence investment → 3000ms
        let t0 = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0); // 10 in might, 0 in presence
        assert_eq!(t0.cadence_interval(), Duration::from_millis(3000));

        // T1: presence 30-44% → 2500ms
        // 7 might + 3 presence: presence=30, max=100 → 30% → T1
        let t1 = ActorAttributes::new(-7, 0, 0, 0, 0, 0, 3, 0, 0);
        assert_eq!(t1.cadence_interval(), Duration::from_millis(2500));

        // T2: presence 45-60% → 2000ms
        // 5 might + 5 presence: presence=50, max=100 → 50% → T2
        let t2 = ActorAttributes::new(-5, 0, 0, 0, 0, 0, 5, 0, 0);
        assert_eq!(t2.cadence_interval(), Duration::from_millis(2000));

        // T3: presence >60% → 1500ms
        // 3 might + 7 presence: presence=70, max=100 → 70% → T3
        let t3 = ActorAttributes::new(-3, 0, 0, 0, 0, 0, 7, 0, 0);
        assert_eq!(t3.cadence_interval(), Duration::from_millis(1500));
    }

    #[test]
    fn test_cadence_monotonically_decreasing() {
        // Higher tier → shorter interval
        let t0 = ActorAttributes::default().cadence_interval();
        let t1 = ActorAttributes::new(-6, 0, 0, 0, 0, 0, 3, 0, 0).cadence_interval();
        let t2 = ActorAttributes::new(-5, 0, 0, 0, 0, 0, 5, 0, 0).cadence_interval();
        let t3 = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0).cadence_interval();
        assert!(t0 > t1, "T0 should be slower than T1");
        assert!(t1 > t2, "T1 should be slower than T2");
        assert!(t2 > t3, "T2 should be slower than T3");
    }

    // ===== EVASION CHANCE TESTS =====

    #[test]
    fn test_evasion_chance_tiers() {
        // T0: no grace investment → 0%
        let t0 = ActorAttributes::default();
        assert_eq!(t0.evasion_chance(), 0.0);

        // T1: grace ~33% of budget → 10%
        // vitality=-6 (60), grace=3 (30) → budget=90, 30/90=33% → T1
        let t1 = ActorAttributes::new(3, 0, 0, -6, 0, 0, 0, 0, 0);
        assert_eq!(t1.evasion_chance(), 0.10);

        // T2: grace ~50% of budget → 20%
        let t2 = ActorAttributes::new(5, 0, 0, -5, 0, 0, 0, 0, 0);
        assert_eq!(t2.evasion_chance(), 0.20);

        // T3: grace 100% of budget → 30%
        let t3 = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(t3.evasion_chance(), 0.30);
    }

    #[test]
    fn test_evasion_monotonically_increasing() {
        // Higher tier → higher dodge chance
        let t0 = ActorAttributes::default().evasion_chance();
        let t1 = ActorAttributes::new(3, 0, 0, -6, 0, 0, 0, 0, 0).evasion_chance();
        let t2 = ActorAttributes::new(5, 0, 0, -5, 0, 0, 0, 0, 0).evasion_chance();
        let t3 = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0).evasion_chance();
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
            precision_mod: 1.0,
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
            precision_mod: 1.0,
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
            precision_mod: 1.0,
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
            precision_mod: 1.0,
        };

        // Threat 2: inserted at 0.5s, expires at 1.5s
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_millis(500),
            timer_duration: Duration::from_secs(1),
            ability: None,
            precision_mod: 1.0,
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
                precision_mod: 1.0,
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
                precision_mod: 1.0,
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
            precision_mod: 1.0,
        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 15.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(1),
            timer_duration: Duration::from_secs(1),
            ability: None,
            precision_mod: 1.0,
        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 20.0,
            damage_type: DamageType::Physical,
            inserted_at: Duration::from_secs(2),
            timer_duration: Duration::from_secs(1),
            ability: None,
            precision_mod: 1.0,
        });
        queue.threats.push_back(QueuedThreat {
            source: entity,
            damage: 25.0,
            damage_type: DamageType::Magic,
            inserted_at: Duration::from_secs(3),
            timer_duration: Duration::from_secs(1),
            ability: None,
            precision_mod: 1.0,
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
