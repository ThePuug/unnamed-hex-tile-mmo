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
/// - Physical: damage = base * (1 + might/33)
/// - Magic: damage = base * (1 + focus/33)
///
/// Scaling: 20 might = 1.6x, 50 might = 2.5x, 100 might = 4.0x
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

    // Linear scaling from attributes, then super-linear level multiplier (ADR-020)
    let linear = base_damage * (1.0 + scaling_attribute / 165.0);
    linear * attrs.damage_level_multiplier()
}

/// Roll for critical hit and calculate multiplier
///
/// Formula (scaled for u16 values):
/// - Crit chance: base (5%) + (instinct / 1000)
///   - At instinct=0: 5% crit chance
///   - At instinct=500 (level 50): 55% crit chance
/// - Crit multiplier: 1.5 + (instinct / 1000)
///   - At instinct=0: 1.5x damage
///   - At instinct=500 (level 50): 2.0x damage
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
    let crit_chance = base_crit_chance + (instinct / 1000.0);

    let mut rng = rand::rng();
    let was_crit = rng.random::<f32>() < crit_chance;

    let crit_multiplier = if was_crit {
        1.5 + (instinct / 1000.0)
    } else {
        1.0
    };

    (was_crit, crit_multiplier)
}

/// Apply passive defensive modifiers to damage
///
/// Formula (scaled for u16 values):
/// - Physical: mitigation = vitality / 330 (capped at 75%)
/// - Magic: mitigation = focus / 330 (capped at 75%)
/// - Final damage = outgoing * (1 - mitigation)
///
/// Scaling: vitality=100 → 30% mitigation, vitality=250 → 75% (cap)
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
            (vitality / 330.0).min(0.75) // Cap at 75% reduction
        }
        DamageType::Magic => {
            let focus = attrs.focus() as f32;
            (focus / 330.0).min(0.75) // Cap at 75% reduction
        }
    };

    let final_damage = outgoing_damage * (1.0 - mitigation);
    final_damage.max(0.0) // Clamp to 0 (no healing from negative damage)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test attributes with simple values
    // Axes: might/grace (negative/positive), vitality/focus (negative/positive)
    fn test_attrs_simple(
        might_grace_axis: i8,     // Negative for might, positive for grace
        vitality_focus_axis: i8,  // Negative for vitality, positive for focus
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_grace_axis, 0, 0,      // might_grace: axis, spectrum, shift
            vitality_focus_axis, 0, 0,   // vitality_focus: axis, spectrum, shift
            0, 0, 0,                      // instinct_presence: axis, spectrum, shift
        )
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants (ADR-015)
    // Formula values may change during balancing, but the two-phase timing must remain.

    /// INV-005: Outgoing Damage Uses Attacker Attributes at Attack Time
    /// Phase 1 of damage calculation MUST use attacker's attributes to calculate outgoing damage.
    /// This ensures attacker's power at attack time determines base damage.
    #[test]
    fn test_outgoing_damage_uses_attacker_attributes_at_attack_time() {
        let attacker_attrs = test_attrs_simple(
            -50,  // might=50 (might_grace_axis=-50)
            0,    // vitality_focus_axis=0
        );
        let base_damage = 20.0;

        let outgoing = calculate_outgoing_damage(base_damage, &attacker_attrs, DamageType::Physical);

        // Formula: base × (1 + might/33) = 20 × (1 + 50/33) ≈ 50.3
        let expected = base_damage * (1.0 + 50.0 / 33.0);
        assert!(
            (outgoing - expected).abs() < 0.1,
            "Outgoing damage incorrect: expected ~{}, got {}",
            expected, outgoing
        );
    }

    /// INV-005: Passive Modifiers Use Defender Attributes at Resolution Time
    /// Phase 2 of damage calculation MUST use defender's attributes for mitigation.
    /// This ensures defender's mitigation at resolution time determines final damage.
    #[test]
    fn test_passive_modifiers_use_defender_attributes_at_resolution_time() {
        let defender_attrs = test_attrs_simple(
            0,    // might_grace_axis=0
            -66,  // vitality=66 (vitality_focus_axis=-66)
        );
        let outgoing_damage = 50.0;

        let final_damage = apply_passive_modifiers(outgoing_damage, &defender_attrs, DamageType::Physical);

        // Formula: mitigation = (vitality/66) = 1.0, capped at 0.75
        // final = 50 × (1 - 0.75) = 12.5
        assert!(
            (final_damage - 12.5).abs() < 0.1,
            "Final damage incorrect: expected 12.5, got {}",
            final_damage
        );
    }

    /// INV-005: Two-Phase Timing Handles Attribute Changes Mid-Queue
    /// Critical invariant: Attacker attributes frozen at attack time,
    /// defender attributes used at resolution time.
    /// This ensures fairness when attributes change (buffs/debuffs) between attack and resolution.
    #[test]
    fn test_two_phase_timing_handles_attribute_changes_mid_queue() {
        // Scenario: Attacker has 50 might at attack time,
        // defender gains 66 vitality buff before resolution

        let attacker_attrs_at_attack = test_attrs_simple(
            -50,  // might=50 (might_grace_axis=-50)
            0,
        );
        let defender_attrs_at_resolution = test_attrs_simple(
            0,
            -66,  // vitality=66 (vitality_focus_axis=-66)
        );

        // Phase 1: Attack time (attacker's stats frozen)
        let outgoing = calculate_outgoing_damage(20.0, &attacker_attrs_at_attack, DamageType::Physical);

        // (Time passes, defender gains buff)

        // Phase 2: Resolution time (defender's new stats apply)
        let final_damage = apply_passive_modifiers(outgoing, &defender_attrs_at_resolution, DamageType::Physical);

        // Verify phase 1 used attacker's attack-time attributes
        let expected_outgoing = 20.0 * (1.0 + 50.0 / 33.0);
        assert!((outgoing - expected_outgoing).abs() < 0.1);

        // Verify phase 2 used defender's resolution-time attributes
        let expected_final = expected_outgoing * (1.0 - 0.75); // 75% cap
        assert!((final_damage - expected_final).abs() < 0.1);
    }

    /// INV-005: Physical Damage Scales with Might
    /// Verify outgoing damage calculation for physical damage type.
    #[test]
    fn test_physical_damage_scales_with_might() {
        let low_might = test_attrs_simple(0, 0);
        let high_might = test_attrs_simple(-100, 0);  // might=100 (might_grace_axis=-100)

        let base = 20.0;
        let damage_low = calculate_outgoing_damage(base, &low_might, DamageType::Physical);
        let damage_high = calculate_outgoing_damage(base, &high_might, DamageType::Physical);

        // High might should deal more damage
        assert!(damage_high > damage_low, "High might should deal more damage");

        // Verify formula: base × (1 + might/33)
        let expected_low = base * (1.0 + 0.0 / 33.0); // = 20
        let expected_high = base * (1.0 + 100.0 / 33.0); // ≈ 80.6
        assert!((damage_low - expected_low).abs() < 0.1);
        assert!((damage_high - expected_high).abs() < 0.1);
    }

    /// INV-005: Magic Damage Scales with Focus
    /// Verify outgoing damage calculation for magic damage type.
    #[test]
    fn test_magic_damage_scales_with_focus() {
        let low_focus = test_attrs_simple(0, 0);
        let high_focus = test_attrs_simple(0, 100);  // focus=100 (vitality_focus_axis=100)

        let base = 20.0;
        let damage_low = calculate_outgoing_damage(base, &low_focus, DamageType::Magic);
        let damage_high = calculate_outgoing_damage(base, &high_focus, DamageType::Magic);

        // High focus should deal more damage
        assert!(damage_high > damage_low, "High focus should deal more damage");

        // Verify formula: base × (1 + focus/33)
        let expected_low = base * (1.0 + 0.0 / 33.0); // = 20
        let expected_high = base * (1.0 + 100.0 / 33.0); // ≈ 80.6
        assert!((damage_low - expected_low).abs() < 0.1);
        assert!((damage_high - expected_high).abs() < 0.1);
    }

    /// INV-005: Physical Mitigation Scales with Vitality
    /// Verify passive modifiers use vitality for physical damage mitigation.
    #[test]
    fn test_physical_mitigation_scales_with_vitality() {
        let low_vitality = test_attrs_simple(0, 0);
        let high_vitality = test_attrs_simple(0, -33); // vitality=33 (vitality_focus_axis=-33)

        let outgoing = 100.0;
        let final_low = apply_passive_modifiers(outgoing, &low_vitality, DamageType::Physical);
        let final_high = apply_passive_modifiers(outgoing, &high_vitality, DamageType::Physical);

        // High vitality should take less damage
        assert!(final_high < final_low, "High vitality should mitigate more damage");

        // Verify formula: outgoing × (1 - vitality/66)
        let expected_low = outgoing * (1.0 - 0.0 / 66.0); // = 100
        let expected_high = outgoing * (1.0 - 33.0 / 66.0); // = 50
        assert!((final_low - expected_low).abs() < 0.1);
        assert!((final_high - expected_high).abs() < 0.1);
    }

    /// INV-005: Magic Mitigation Scales with Focus
    /// Verify passive modifiers use focus for magic damage mitigation.
    #[test]
    fn test_magic_mitigation_scales_with_focus() {
        let low_focus = test_attrs_simple(0, 0);
        let high_focus = test_attrs_simple(0, 33); // focus=33 (vitality_focus_axis=33)

        let outgoing = 100.0;
        let final_low = apply_passive_modifiers(outgoing, &low_focus, DamageType::Magic);
        let final_high = apply_passive_modifiers(outgoing, &high_focus, DamageType::Magic);

        // High focus should take less damage
        assert!(final_high < final_low, "High focus should mitigate more magic damage");

        // Verify formula: outgoing × (1 - focus/66)
        let expected_low = outgoing * (1.0 - 0.0 / 66.0); // = 100
        let expected_high = outgoing * (1.0 - 33.0 / 66.0); // = 50
        assert!((final_low - expected_low).abs() < 0.1);
        assert!((final_high - expected_high).abs() < 0.1);
    }
}
