//! Derived stat calculations using the three-pole scaling system
//!
//! **Phase 3 Examples** - Demonstrates how to use the scaling system for derived stats
//! These are reference implementations showing the pattern. Integration is optional.
//!
//! See ADR-016 Phase 3 for architectural guidance on when to use these vs simple formulas.

use super::{calculate_magnitude_value, calculate_ratio_effectiveness, ComponentScaling, MagnitudeScalars, PHYSICAL_MITIGATION};
use crate::common::components::ActorAttributes;

/// Calculate physical damage mitigation percentage using the scaling system
///
/// Uses the PHYSICAL_MITIGATION definition which includes:
/// - Base mitigation (magnitude: 0.5% per Vitality)
/// - Contested bonus (ratio: your Vitality vs attacker's Might)
///
/// # Arguments
/// * `defender_attrs` - Defender's attributes
/// * `attacker_attrs` - Attacker's attributes (for contested calculation)
///
/// # Returns
/// Mitigation percentage (0.0 to 0.75, representing 0% to 75%)
///
/// # Example
/// ```
/// use unnamed_hex_tile_mmo::common::{
///     components::ActorAttributes,
///     systems::combat::scaling::derived::calculate_physical_mitigation_scaled,
/// };
///
/// let defender = ActorAttributes::new(0, 0, 0, -50, 0, 0, 0, 0, 0, 10); // 50 Vitality
/// let attacker = ActorAttributes::new(-30, 0, 0, 0, 0, 0, 0, 0, 0, 10); // 30 Might
///
/// let mitigation = calculate_physical_mitigation_scaled(&defender, &attacker);
/// // Base: 50 * 0.005 = 0.25 (25%)
/// // Contested: depends on Vitality vs Might ratio
/// // Total: ~30-40% depending on the matchup
/// ```
pub fn calculate_physical_mitigation_scaled(
    defender_attrs: &ActorAttributes,
    attacker_attrs: &ActorAttributes,
) -> f32 {
    // Component 1: Base mitigation (magnitude)
    let base_mit_component = PHYSICAL_MITIGATION.components
        .iter()
        .find(|c| c.name == "base_mitigation")
        .expect("PHYSICAL_MITIGATION must have base_mitigation component");

    let ComponentScaling::Magnitude { base, scalars } = base_mit_component.scaling else {
        panic!("Base mitigation must use Magnitude scaling");
    };

    let base_mitigation = calculate_magnitude_value(
        base,
        defender_attrs.total_level,
        defender_attrs.vitality() as i8,
        0, // Mitigation doesn't use reach
        scalars,
    );

    // Component 2: Contested bonus (ratio)
    let contested_component = PHYSICAL_MITIGATION.components
        .iter()
        .find(|c| c.name == "contested_bonus")
        .expect("PHYSICAL_MITIGATION must have contested_bonus component");

    let ComponentScaling::MagnitudeAndRatio { ratio, .. } = contested_component.scaling else {
        panic!("Contested mitigation must use MagnitudeAndRatio scaling");
    };

    let contested_bonus = calculate_ratio_effectiveness(
        defender_attrs.vitality() as i8,
        defender_attrs.total_level,
        attacker_attrs.might() as i8,
        attacker_attrs.total_level,
        ratio,
    );

    // Combine: base + contested, capped at 75%
    let total_mitigation = (base_mitigation + contested_bonus).min(0.75);
    total_mitigation
}

/// Calculate maximum health using the scaling system
///
/// This is a simple magnitude-based calculation:
/// base + (vitality_reach * scalar)
///
/// # Example
/// ```
/// use unnamed_hex_tile_mmo::common::{
///     components::ActorAttributes,
///     systems::combat::scaling::derived::calculate_max_health_scaled,
/// };
///
/// let attrs = ActorAttributes::new(0, 0, 0, -50, 10, 0, 0, 0, 0, 10); // 50 Vitality, 10 spectrum
/// let max_hp = calculate_max_health_scaled(&attrs);
/// // base 100 + (vitality_reach * 19)
/// ```
pub fn calculate_max_health_scaled(attrs: &ActorAttributes) -> f32 {
    let scalars = MagnitudeScalars {
        level: 0.0,   // HP doesn't scale with level in current design
        stat: 0.0,    // HP uses reach, not stat
        reach: 19.0,  // +19 HP per reach point
    };

    calculate_magnitude_value(
        100.0, // Base HP
        attrs.total_level,
        0, // Unused (reach is used instead)
        attrs.vitality_reach() as u32,
        scalars,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_attrs(
        might_grace: i8,
        vitality_focus: i8,
        total_level: u32,
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_grace, 0, 0,
            vitality_focus, 0, 0,
            0, 0, 0,
            total_level,
        )
    }

    #[test]
    fn test_physical_mitigation_base() {
        let defender = test_attrs(0, -50, 10); // 50 Vitality
        let attacker = test_attrs(-50, 0, 10); // 50 Might

        let mitigation = calculate_physical_mitigation_scaled(&defender, &attacker);

        // Base mitigation: 50 * 0.005 = 0.25 (25%)
        // Contested depends on ratio
        assert!(mitigation >= 0.25, "Should have at least base mitigation");
        assert!(mitigation <= 0.75, "Should be capped at 75%");
    }

    #[test]
    fn test_physical_mitigation_high_vitality_vs_low_might() {
        let defender = test_attrs(0, -80, 10); // 80 Vitality
        let attacker = test_attrs(-20, 0, 10); // 20 Might

        let mitigation = calculate_physical_mitigation_scaled(&defender, &attacker);

        // High Vitality vs low Might should give significant mitigation
        assert!(mitigation > 0.4, "High Vitality should give >40% mitigation");
    }

    #[test]
    fn test_physical_mitigation_low_vitality_vs_high_might() {
        let defender = test_attrs(0, -20, 10); // 20 Vitality
        let attacker = test_attrs(-80, 0, 10); // 80 Might

        let mitigation = calculate_physical_mitigation_scaled(&defender, &attacker);

        // Low Vitality vs high Might should give less mitigation than high vitality
        // Base: 20 * 0.005 = 0.10, plus contested bonus
        assert!(mitigation < 0.45, "Low Vitality vs high Might should be less effective");
        assert!(mitigation > 0.05, "Should still have some base mitigation");
    }

    #[test]
    fn test_max_health_scaling() {
        let low_vitality = test_attrs(0, 0, 10); // 0 Vitality
        let high_vitality = test_attrs(0, -100, 10); // 100 Vitality

        let hp_low = calculate_max_health_scaled(&low_vitality);
        let hp_high = calculate_max_health_scaled(&high_vitality);

        // 0 vitality_reach = 100 HP
        assert_eq!(hp_low, 100.0);

        // 100 vitality_reach = 100 + (100 * 19) = 2000 HP
        assert_eq!(hp_high, 2000.0);
    }

    #[test]
    fn test_mitigation_capped_at_75_percent() {
        // Even with extreme stats, mitigation should cap at 75%
        let defender = test_attrs(0, -100, 50); // Max Vitality, high level
        let attacker = test_attrs(0, 0, 1); // No Might, low level

        let mitigation = calculate_physical_mitigation_scaled(&defender, &attacker);

        assert!(mitigation <= 0.75, "Mitigation must cap at 75%");
    }
}
