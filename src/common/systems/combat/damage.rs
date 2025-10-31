//! Damage calculation functions for combat system
//!
//! This module provides the core damage calculation formulas for the combat system,
//! implementing a two-phase damage calculation model:
//!
//! **Phase 1 (at attack time):** Calculate outgoing damage with attacker's attributes
//! **Phase 2 (at resolution time):** Apply passive modifiers with defender's attributes
//!
//! See ADR-005 for architectural details.

use crate::common::components::reaction_queue::DamageType;
use crate::common::components::ActorAttributes;
use rand::Rng;

/// Calculate outgoing damage based on attacker's attributes
///
/// Formula (from combat-system.md spec):
/// - Physical: damage = base * (1 + might/100)
/// - Magic: damage = base * (1 + focus/100)
///
/// # Arguments
/// * `base_damage` - Base damage from ability
/// * `attrs` - Attacker's attributes
/// * `damage_type` - Physical or Magic
///
/// # Returns
/// Scaled damage before mitigation
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let damage = calculate_outgoing_damage(20.0, &attrs, DamageType::Physical);
/// ```
pub fn calculate_outgoing_damage(
    base_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
) -> f32 {
    let scaling_attribute = match damage_type {
        DamageType::Physical => attrs.might() as f32,
        DamageType::Magic => attrs.focus() as f32,
    };

    base_damage * (1.0 + scaling_attribute / 100.0)
}

/// Roll for critical hit and calculate multiplier
///
/// Formula (from combat-system.md spec):
/// - Crit chance: base (5%) + (instinct / 200)
///   - At instinct=0: 5% crit chance
///   - At instinct=100: 55% crit chance
/// - Crit multiplier: 1.5 + (instinct / 200)
///   - At instinct=0: 1.5x damage
///   - At instinct=100: 2.0x damage
///
/// # Arguments
/// * `attrs` - Attacker's attributes
///
/// # Returns
/// Tuple of (was_crit: bool, multiplier: f32)
/// - If crit: (true, 1.5-2.0)
/// - If not crit: (false, 1.0)
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let (was_crit, multiplier) = roll_critical(&attrs);
/// ```
pub fn roll_critical(attrs: &ActorAttributes) -> (bool, f32) {
    let instinct = attrs.instinct() as f32;
    let base_crit_chance = 0.05; // 5%
    let crit_chance = base_crit_chance + (instinct / 200.0);

    let mut rng = rand::rng();
    let was_crit = rng.random::<f32>() < crit_chance;

    let crit_multiplier = if was_crit {
        1.5 + (instinct / 200.0)
    } else {
        1.0
    };

    (was_crit, crit_multiplier)
}

/// Apply passive defensive modifiers to damage
///
/// Formula (from combat-system.md spec):
/// - Physical: mitigation = vitality / 200 (capped at 75%)
/// - Magic: mitigation = focus / 200 (capped at 75%)
/// - Final damage = outgoing * (1 - mitigation)
///
/// # Arguments
/// * `outgoing_damage` - Damage after attacker scaling
/// * `attrs` - Defender's attributes
/// * `damage_type` - Physical or Magic
///
/// # Returns
/// Final damage after mitigation (clamped to 0 minimum)
///
/// # Example
/// ```
/// let attrs = ActorAttributes::default();
/// let final_damage = apply_passive_modifiers(50.0, &attrs, DamageType::Physical);
/// ```
pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
) -> f32 {
    let mitigation = match damage_type {
        DamageType::Physical => {
            let vitality = attrs.vitality() as f32;
            (vitality / 200.0).min(0.75) // Cap at 75% reduction
        }
        DamageType::Magic => {
            let focus = attrs.focus() as f32;
            (focus / 200.0).min(0.75) // Cap at 75% reduction
        }
    };

    let final_damage = outgoing_damage * (1.0 - mitigation);
    final_damage.max(0.0) // Clamp to 0 (no healing from negative damage)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create attributes with specific values for testing
    fn create_test_attributes(
        might_grace_axis: i8,
        might_grace_spectrum: u8,
        vitality_focus_axis: i8,
        vitality_focus_spectrum: u8,
        instinct_presence_axis: i8,
        instinct_presence_spectrum: u8,
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_grace_axis,
            might_grace_spectrum,
            0, // might_grace_shift
            vitality_focus_axis,
            vitality_focus_spectrum,
            0, // vitality_focus_shift
            instinct_presence_axis,
            instinct_presence_spectrum,
            0, // instinct_presence_shift
        )
    }

    // ===== OUTGOING DAMAGE TESTS =====

    #[test]
    fn test_physical_damage_scaling_zero_might() {
        // At might=0: damage = base * (1 + 0/100) = base * 1.0
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);
        let damage = calculate_outgoing_damage(20.0, &attrs, DamageType::Physical);
        assert_eq!(damage, 20.0);
    }

    #[test]
    fn test_physical_damage_scaling_fifty_might() {
        // At might=50: damage = base * (1 + 50/100) = base * 1.5
        // To get might=50, need axis=-50 with spectrum=25 (gives might=75 at shift=0)
        // Actually, for might=50: axis=-50, spectrum=0 gives might=abs(-50) = 50
        let attrs = create_test_attributes(-50, 0, 0, 0, 0, 0);
        let damage = calculate_outgoing_damage(20.0, &attrs, DamageType::Physical);
        assert_eq!(damage, 30.0); // 20 * 1.5 = 30
    }

    #[test]
    fn test_physical_damage_scaling_hundred_might() {
        // At might=100: damage = base * (1 + 100/100) = base * 2.0
        let attrs = create_test_attributes(-100, 0, 0, 0, 0, 0);
        let damage = calculate_outgoing_damage(20.0, &attrs, DamageType::Physical);
        assert_eq!(damage, 40.0); // 20 * 2.0 = 40
    }

    #[test]
    fn test_magic_damage_scaling_zero_focus() {
        // At focus=0: damage = base * (1 + 0/100) = base * 1.0
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);
        let damage = calculate_outgoing_damage(15.0, &attrs, DamageType::Magic);
        assert_eq!(damage, 15.0);
    }

    #[test]
    fn test_magic_damage_scaling_fifty_focus() {
        // At focus=50: damage = base * (1 + 50/100) = base * 1.5
        let attrs = create_test_attributes(0, 0, 50, 0, 0, 0);
        let damage = calculate_outgoing_damage(15.0, &attrs, DamageType::Magic);
        assert_eq!(damage, 22.5); // 15 * 1.5 = 22.5
    }

    #[test]
    fn test_magic_damage_scaling_hundred_focus() {
        // At focus=100: damage = base * (1 + 100/100) = base * 2.0
        let attrs = create_test_attributes(0, 0, 100, 0, 0, 0);
        let damage = calculate_outgoing_damage(15.0, &attrs, DamageType::Magic);
        assert_eq!(damage, 30.0); // 15 * 2.0 = 30
    }

    // ===== CRITICAL HIT TESTS =====

    #[test]
    fn test_critical_roll_instinct_zero() {
        // At instinct=0: crit chance = 5% + (0/200) = 5%
        // We can't test randomness deterministically, but we can test the formula
        // by calling it many times and checking the distribution
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);

        let mut crit_count = 0;
        let mut total_multiplier = 0.0;
        let trials = 1000;

        for _ in 0..trials {
            let (was_crit, multiplier) = roll_critical(&attrs);
            if was_crit {
                crit_count += 1;
                total_multiplier += multiplier;
            }
        }

        let crit_rate = crit_count as f32 / trials as f32;

        // Allow 3% margin of error (5% ± 3% = 2-8%)
        assert!(crit_rate >= 0.02 && crit_rate <= 0.08,
            "Expected ~5% crit rate, got {:.1}%", crit_rate * 100.0);

        // Check average multiplier for crits (should be ~1.5)
        if crit_count > 0 {
            let avg_multiplier = total_multiplier / crit_count as f32;
            assert!((avg_multiplier - 1.5).abs() < 0.1,
                "Expected ~1.5x crit multiplier at instinct=0, got {:.2}x", avg_multiplier);
        }
    }

    #[test]
    fn test_critical_roll_instinct_hundred() {
        // At instinct=100: crit chance = 5% + (100/200) = 55%
        // Multiplier: 1.5 + (100/200) = 2.0x
        let attrs = create_test_attributes(0, 0, 0, 0, -100, 0);

        let mut crit_count = 0;
        let mut total_multiplier = 0.0;
        let trials = 1000;

        for _ in 0..trials {
            let (was_crit, multiplier) = roll_critical(&attrs);
            if was_crit {
                crit_count += 1;
                total_multiplier += multiplier;
            }
        }

        let crit_rate = crit_count as f32 / trials as f32;

        // Allow 5% margin of error (55% ± 5% = 50-60%)
        assert!(crit_rate >= 0.50 && crit_rate <= 0.60,
            "Expected ~55% crit rate at instinct=100, got {:.1}%", crit_rate * 100.0);

        // Check average multiplier for crits (should be ~2.0)
        if crit_count > 0 {
            let avg_multiplier = total_multiplier / crit_count as f32;
            assert!((avg_multiplier - 2.0).abs() < 0.1,
                "Expected ~2.0x crit multiplier at instinct=100, got {:.2}x", avg_multiplier);
        }
    }

    #[test]
    fn test_critical_non_crit_returns_one_multiplier() {
        // When not a crit, multiplier should always be 1.0
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);

        for _ in 0..100 {
            let (was_crit, multiplier) = roll_critical(&attrs);
            if !was_crit {
                assert_eq!(multiplier, 1.0, "Non-crit should have 1.0x multiplier");
            }
        }
    }

    // ===== PASSIVE MODIFIERS TESTS =====

    #[test]
    fn test_physical_mitigation_zero_vitality() {
        // At vitality=0: mitigation = 0/200 = 0%
        // Final damage = 50 * (1 - 0) = 50
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);
        let final_damage = apply_passive_modifiers(50.0, &attrs, DamageType::Physical);
        assert_eq!(final_damage, 50.0);
    }

    #[test]
    fn test_physical_mitigation_fifty_vitality() {
        // At vitality=50: mitigation = 50/200 = 25%
        // Final damage = 50 * (1 - 0.25) = 37.5
        let attrs = create_test_attributes(0, 0, -50, 0, 0, 0);
        let final_damage = apply_passive_modifiers(50.0, &attrs, DamageType::Physical);
        assert_eq!(final_damage, 37.5);
    }

    #[test]
    fn test_physical_mitigation_hundred_vitality() {
        // At vitality=100: mitigation = 100/200 = 50%
        // Final damage = 50 * (1 - 0.5) = 25
        let attrs = create_test_attributes(0, 0, -100, 0, 0, 0);
        let final_damage = apply_passive_modifiers(50.0, &attrs, DamageType::Physical);
        assert_eq!(final_damage, 25.0);
    }

    #[test]
    fn test_physical_mitigation_capped_at_75_percent() {
        // At vitality=150: mitigation = 150/200 = 75% (cap)
        // At vitality=200: mitigation = 200/200 = 100%, but capped at 75%
        // Final damage = 100 * (1 - 0.75) = 25
        let attrs = create_test_attributes(0, 0, -100, 50, 0, 0); // This gives very high vitality
        let final_damage = apply_passive_modifiers(100.0, &attrs, DamageType::Physical);
        // Should be at least 25 (75% mitigation cap)
        assert!(final_damage >= 25.0, "Mitigation should cap at 75%, got {}", final_damage);
    }

    #[test]
    fn test_magic_mitigation_zero_focus() {
        // At focus=0: mitigation = 0/200 = 0%
        // Final damage = 40 * (1 - 0) = 40
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);
        let final_damage = apply_passive_modifiers(40.0, &attrs, DamageType::Magic);
        assert_eq!(final_damage, 40.0);
    }

    #[test]
    fn test_magic_mitigation_fifty_focus() {
        // At focus=50: mitigation = 50/200 = 25%
        // Final damage = 40 * (1 - 0.25) = 30
        let attrs = create_test_attributes(0, 0, 50, 0, 0, 0);
        let final_damage = apply_passive_modifiers(40.0, &attrs, DamageType::Magic);
        assert_eq!(final_damage, 30.0);
    }

    #[test]
    fn test_magic_mitigation_hundred_focus() {
        // At focus=100: mitigation = 100/200 = 50%
        // Final damage = 40 * (1 - 0.5) = 20
        let attrs = create_test_attributes(0, 0, 100, 0, 0, 0);
        let final_damage = apply_passive_modifiers(40.0, &attrs, DamageType::Magic);
        assert_eq!(final_damage, 20.0);
    }

    // ===== EDGE CASE TESTS =====

    #[test]
    fn test_zero_base_damage() {
        let attrs = create_test_attributes(-50, 0, 0, 0, 0, 0);
        let damage = calculate_outgoing_damage(0.0, &attrs, DamageType::Physical);
        assert_eq!(damage, 0.0);
    }

    #[test]
    fn test_negative_damage_clamped_to_zero() {
        // This shouldn't happen in normal gameplay, but test defensive programming
        let attrs = create_test_attributes(0, 0, 0, 0, 0, 0);
        let final_damage = apply_passive_modifiers(-10.0, &attrs, DamageType::Physical);
        assert_eq!(final_damage, 0.0, "Negative damage should be clamped to 0");
    }

    #[test]
    fn test_full_pipeline_physical() {
        // Full damage calculation: attacker with might=50, defender with vitality=100
        // Base damage: 20
        // Outgoing: 20 * (1 + 50/100) = 30
        // Crit: assume no crit (multiplier = 1.0)
        // Mitigation: 30 * (1 - 100/200) = 30 * 0.5 = 15
        let attacker = create_test_attributes(-50, 0, 0, 0, 0, 0);
        let defender = create_test_attributes(0, 0, -100, 0, 0, 0);

        let outgoing = calculate_outgoing_damage(20.0, &attacker, DamageType::Physical);
        assert_eq!(outgoing, 30.0);

        let final_damage = apply_passive_modifiers(outgoing, &defender, DamageType::Physical);
        assert_eq!(final_damage, 15.0);
    }

    #[test]
    fn test_full_pipeline_magic() {
        // Full damage calculation: attacker with focus=100, defender with focus=50
        // Base damage: 15
        // Outgoing: 15 * (1 + 100/100) = 30
        // Mitigation: 30 * (1 - 50/200) = 30 * 0.75 = 22.5
        let attacker = create_test_attributes(0, 0, 100, 0, 0, 0);
        let defender = create_test_attributes(0, 0, 50, 0, 0, 0);

        let outgoing = calculate_outgoing_damage(15.0, &attacker, DamageType::Magic);
        assert_eq!(outgoing, 30.0);

        let final_damage = apply_passive_modifiers(outgoing, &defender, DamageType::Magic);
        assert_eq!(final_damage, 22.5);
    }
}
