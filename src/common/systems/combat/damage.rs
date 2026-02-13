//! Damage calculation functions for combat system
//!
//! This module provides the core damage calculation formulas for the combat system,
//! implementing a two-phase damage calculation model:
//!
//! **Phase 1 (at attack time):** Calculate outgoing damage with attacker's attributes
//! **Phase 2 (at resolution time):** Apply passive modifiers with defender's attributes
//!
//! See ADR-005 for architectural details.

use bevy::prelude::Entity;
use crate::common::components::reaction_queue::DamageType;
use crate::common::components::ActorAttributes;

/// Compute a relative stat contest modifier between attacker and defender.
///
/// Uses raw attribute values only (no level multiplier). Exponential curve with diminishing returns
/// and complete negation at equal stats.
///
/// Formula: if delta <= 0 then 0.0, else sqrt(delta / 300).clamp(0, 1)
///
/// Equal or disadvantaged stats completely negate the benefit:
/// - Disadvantage (delta < 0) → 0.0 (completely negated)
/// - Equal stats (delta = 0) → 0.0 (cancelled out)
/// - +100 advantage → 0.577 (57.7% of full benefit)
/// - +200 advantage → 0.816 (81.6% of full benefit)
/// - +300 advantage → 1.0 (100% full benefit, capped)
pub fn contest_modifier(attacker_stat: u16, defender_stat: u16) -> f32 {
    let delta = attacker_stat as f32 - defender_stat as f32;
    const MAX_DELTA: f32 = 300.0;

    // Any disadvantage or equal stats = complete negation
    if delta <= 0.0 {
        return 0.0;
    }

    // Normalize delta to [0, 1] range (capped at +300)
    let normalized = (delta / MAX_DELTA).min(1.0);

    // Apply square root for diminishing returns
    normalized.sqrt()
}

/// Calculate outgoing damage based on attacker's attributes
///
/// Since SOW-020 Phase 4, base_damage from abilities comes from meta-attributes
/// (e.g., force()) which already include attribute scaling and level multipliers.
/// This function now serves as a pass-through for future expansion (e.g., gear bonuses).
///
/// # Arguments
/// * `base_damage` - Base damage from ability (already scaled by meta-attributes)
/// * `attrs` - Attacker's attributes (currently unused, kept for future expansion)
/// * `damage_type` - Physical or Magic (currently unused, kept for future expansion)
///
/// # Returns
/// Base damage without additional scaling
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let damage = calculate_outgoing_damage(20.0, &attrs, DamageType::Physical);
/// ```
pub fn calculate_outgoing_damage(
    base_damage: f32,
    _attrs: &ActorAttributes,
    _damage_type: DamageType,
) -> f32 {
    // Base damage from meta-attributes (like force()) already includes:
    // - Attribute scaling (might, focus, etc.)
    // - Level multipliers
    // No additional scaling needed to avoid double-application
    base_damage
}


/// Calculate recovery pushback based on Impact vs Composure contest (SOW-021 Phase 2)
///
/// Formula: Contest reduces effective stat, then linear scaling 0% to 50%
/// - Effective Impact = impact × contest_modifier(impact, composure)
/// - Pushback = (effective_impact / 6.0) capped at 50%
///
/// # Arguments
/// * `attacker_impact` - Attacker's Impact stat (might)
/// * `defender_composure` - Defender's Composure stat (focus)
///
/// # Returns
/// Pushback percentage (e.g., 0.25 for 25%)
///
/// # Example
/// ```
/// let pushback = calculate_recovery_pushback(0, 0); // Returns 0.0 (no impact)
/// let pushback = calculate_recovery_pushback(100, 100); // Returns 0.167 (neutral contest)
/// let pushback = calculate_recovery_pushback(300, 0); // Returns 0.50 (50% capped)
/// ```
pub fn calculate_recovery_pushback(
    attacker_impact: u16,
    defender_composure: u16,
) -> f32 {
    const MAX_PUSHBACK: f32 = 0.50; // Cap at 50%
    const IMPACT_DIVISOR: f32 = 600.0; // 300 impact → 50% (0.50 as decimal)

    // Calculate base pushback from raw stat
    let base_pushback = (attacker_impact as f32) / IMPACT_DIVISOR;

    // Apply contest modifier directly to benefit
    let contest_mod = contest_modifier(attacker_impact, defender_composure);
    let contested_pushback = base_pushback * contest_mod;

    contested_pushback.min(MAX_PUSHBACK)
}

/// Scan for the strongest Dominance aura within range of target
///
/// Returns the highest Dominance stat within 5-hex radius
pub fn find_max_dominance_in_range(
    target: Entity,
    loc_query: &bevy::prelude::Query<&crate::common::components::Loc>,
    attrs_query: &bevy::prelude::Query<&ActorAttributes>,
) -> u16 {
    const RADIUS: i32 = 5;

    let Ok(target_loc) = loc_query.get(target) else {
        return 0;
    };

    let mut max_dominance = 0u16;

    for (loc, attrs) in loc_query.iter().zip(attrs_query.iter()) {
        let distance = target_loc.flat_distance(loc) as i32;
        if distance <= RADIUS {
            let dominance = attrs.dominance();
            if dominance > max_dominance {
                max_dominance = dominance;
            }
        }
    }

    max_dominance
}

/// Apply passive defensive modifiers to damage
///
/// Formula: Toughness contested by strongest Dominance aura in range
/// - effective_toughness = toughness × contest_modifier(toughness, max_dominance)
/// - Physical: mitigation = (effective_toughness / 330).min(75%)
/// - Magic: mitigation = (effective_focus / 330).min(75%)
/// - Final damage = outgoing * (1 - mitigation)
///
/// # Arguments
/// * `outgoing_damage` - Damage after attacker scaling
/// * `attrs` - Defender's attributes
/// * `damage_type` - Physical or Magic
/// * `max_dominance_in_range` - Strongest Dominance aura affecting defender (0 if none)
///
/// # Returns
/// Final damage after mitigation (clamped to 0 minimum)
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
    max_dominance_in_range: u16,
) -> f32 {
    let mitigation = match damage_type {
        DamageType::Physical => {
            let toughness = attrs.toughness();
            // Calculate base mitigation from raw stat
            let base_mitigation = (toughness as f32) / 330.0;
            // Apply contest modifier directly to benefit
            let contest_mod = contest_modifier(toughness, max_dominance_in_range);
            let contested_mitigation = base_mitigation * contest_mod;
            contested_mitigation.min(0.75)
        }
        DamageType::Magic => {
            let focus = attrs.focus();
            // Calculate base mitigation from raw stat
            let base_mitigation = (focus as f32) / 330.0;
            // Apply contest modifier directly to benefit
            let contest_mod = contest_modifier(focus, max_dominance_in_range);
            let contested_mitigation = base_mitigation * contest_mod;
            contested_mitigation.min(0.75)
        }
    };

    let final_damage = outgoing_damage * (1.0 - mitigation);
    final_damage.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== CONTEST MODIFIER TESTS =====

    #[test]
    fn test_contest_modifier_neutral() {
        // Equal stats = complete negation (0.0x)
        assert_eq!(contest_modifier(100, 100), 0.0);
        assert_eq!(contest_modifier(0, 0), 0.0);
        assert_eq!(contest_modifier(50, 50), 0.0);
    }

    #[test]
    fn test_contest_modifier_attacker_advantage() {
        // delta = 50, sqrt(50/300) = sqrt(0.167) = 0.408
        let m = contest_modifier(150, 100);
        assert!(m > 0.0, "Attacker advantage should produce >0.0, got {m}");
        assert!((m - 0.408).abs() < 0.01, "150 vs 100 → sqrt(50/300) = ~0.408, got {m}");
    }

    #[test]
    fn test_contest_modifier_defender_advantage() {
        // Any disadvantage = complete negation (0.0x)
        let m = contest_modifier(100, 150);
        assert_eq!(m, 0.0, "Defender advantage should produce 0.0, got {m}");
    }

    #[test]
    fn test_contest_modifier_clamped() {
        // +300 delta: sqrt(300/300) = 1.0 (max)
        assert_eq!(contest_modifier(300, 0), 1.0);
        // Any disadvantage = 0.0 (min)
        assert_eq!(contest_modifier(0, 300), 0.0);
        // Extreme advantage values
        assert_eq!(contest_modifier(u16::MAX, 0), 1.0);
        assert_eq!(contest_modifier(0, u16::MAX), 0.0);
    }

    #[test]
    fn test_contest_modifier_asymmetric() {
        // Advantage gives benefit (>0), disadvantage gives nothing (0)
        let pairs = [(100, 150), (0, 50), (200, 100), (80, 120)];
        for (a, b) in pairs {
            let forward = contest_modifier(a, b);
            let reverse = contest_modifier(b, a);
            if a > b {
                assert!(forward > 0.0, "f({a},{b}) should be >0 (advantage), got {forward}");
                assert_eq!(reverse, 0.0, "f({b},{a}) should be 0 (disadvantage), got {reverse}");
            } else if a < b {
                assert_eq!(forward, 0.0, "f({a},{b}) should be 0 (disadvantage), got {forward}");
                assert!(reverse > 0.0, "f({b},{a}) should be >0 (advantage), got {reverse}");
            } else {
                assert_eq!(forward, 0.0, "f({a},{b}) should be 0 (equal), got {forward}");
                assert_eq!(reverse, 0.0, "f({b},{a}) should be 0 (equal), got {reverse}");
            }
        }
    }

    #[test]
    fn test_contest_modifier_monotonic() {
        // Larger attacker stat → larger modifier
        let m1 = contest_modifier(100, 100); // delta 0
        let m2 = contest_modifier(120, 100); // delta 20
        let m3 = contest_modifier(150, 100); // delta 50
        let m4 = contest_modifier(200, 100); // delta 100 (capped)
        assert!(m1 < m2, "m1={m1} should be < m2={m2}");
        assert!(m2 < m3, "m2={m2} should be < m3={m3}");
        assert!(m3 <= m4, "m3={m3} should be <= m4={m4}");
    }

    // ===== RECOVERY PUSHBACK TESTS (SOW-021 Phase 2) =====

    #[test]
    fn test_pushback_neutral_contest() {
        // 100 Impact vs 100 Composure: equal stats = complete negation → 0%
        let pushback = calculate_recovery_pushback(100, 100);
        assert_eq!(pushback, 0.0, "Expected 0.0 (stats cancel), got {pushback}");
    }

    #[test]
    fn test_pushback_attacker_advantage() {
        // 200 Impact vs 100 Composure: base = 200/600 = 0.333, contest = sqrt(100/300) = 0.577, result = 0.192
        let pushback = calculate_recovery_pushback(200, 100);
        assert!((pushback - 0.192).abs() < 0.01, "Expected ~0.192, got {pushback}");
    }

    #[test]
    fn test_pushback_defender_advantage() {
        // 100 Impact vs 200 Composure: contest = 0.0 (defender advantage), result = 0.0
        let pushback = calculate_recovery_pushback(100, 200);
        assert_eq!(pushback, 0.0, "Expected 0.0 (negated), got {pushback}");
    }

    #[test]
    fn test_pushback_extreme_values() {
        // Maximum Impact vs 0 Composure: effective = MAX × 1.5 → 50% (capped)
        let max_pushback = calculate_recovery_pushback(u16::MAX, 0);
        assert!((max_pushback - 0.50).abs() < 0.01, "Expected 0.50 (capped), got {max_pushback}");

        // 0 Impact: effective = 0 × anything = 0 → 0%
        let min_pushback = calculate_recovery_pushback(0, u16::MAX);
        assert!((min_pushback - 0.0).abs() < 0.001, "Expected 0.0 (no impact), got {min_pushback}");
    }

    #[test]
    fn test_pushback_on_2s_recovery() {
        // Simulate pushback on 2s recovery with equal stats (100, 100) = complete negation
        let pushback_pct = calculate_recovery_pushback(100, 100);
        let extension = 2.0 * pushback_pct;  // 2s × 0.0 = 0s (negated)
        assert_eq!(extension, 0.0, "Expected 0.0s extension (stats cancel), got {extension}");
    }

}


