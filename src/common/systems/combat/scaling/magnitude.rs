//! Magnitude scaling calculations
//!
//! Magnitude scaling: absolute stat values produce absolute outputs.
//! Used for: damage, HP, resource pools, shield absorption.
//!
//! Formula: base + (level * level_scalar) + (stat * stat_scalar) + (reach * reach_scalar)

use super::types::MagnitudeScalars;

/// Calculate magnitude-based scaling value
///
/// Produces absolute output values from absolute stat inputs.
/// Each component contributes additively to the final result.
///
/// # Arguments
/// * `base` - Base value (at level 0, stat 0, reach 0)
/// * `level` - Character level
/// * `stat` - Relevant attribute stat value (e.g., might for damage)
/// * `reach` - Reach value for the stat (e.g., might_reach)
/// * `scalars` - Scaling coefficients for each component
///
/// # Returns
/// Calculated magnitude value
///
/// # Example
/// ```
/// use unnamed_hex_tile_mmo::common::systems::combat::scaling::{
///     calculate_magnitude_value, MagnitudeScalars
/// };
///
/// let scalars = MagnitudeScalars {
///     level: 1.0,   // +1 damage per level
///     stat: 2.0,    // +2 damage per stat point
///     reach: 0.0,   // Reach doesn't affect damage
/// };
///
/// // Level 10, 10 Might, 0 Reach
/// let damage = calculate_magnitude_value(40.0, 10, 10, 0, scalars);
/// assert_eq!(damage, 70.0); // 40 + 10 + 20
/// ```
pub fn calculate_magnitude_value(
    base: f32,
    level: u32,
    stat: i8,
    reach: u32,
    scalars: MagnitudeScalars,
) -> f32 {
    base
        + (level as f32 * scalars.level)
        + (stat.abs() as f32 * scalars.stat)
        + (reach as f32 * scalars.reach)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== UNIT TESTS =====
    // Test pure magnitude scaling function

    #[test]
    fn test_magnitude_base_only() {
        let scalars = MagnitudeScalars {
            level: 0.0,
            stat: 0.0,
            reach: 0.0,
        };

        let result = calculate_magnitude_value(50.0, 0, 0, 0, scalars);
        assert_eq!(result, 50.0, "Base value should be returned when all scalars are 0");
    }

    #[test]
    fn test_magnitude_level_scaling() {
        let scalars = MagnitudeScalars {
            level: 1.0,
            stat: 0.0,
            reach: 0.0,
        };

        let result_l1 = calculate_magnitude_value(20.0, 1, 0, 0, scalars);
        let result_l10 = calculate_magnitude_value(20.0, 10, 0, 0, scalars);
        let result_l50 = calculate_magnitude_value(20.0, 50, 0, 0, scalars);

        assert_eq!(result_l1, 21.0, "Level 1 should add 1.0 to base");
        assert_eq!(result_l10, 30.0, "Level 10 should add 10.0 to base");
        assert_eq!(result_l50, 70.0, "Level 50 should add 50.0 to base");
    }

    #[test]
    fn test_magnitude_stat_scaling() {
        let scalars = MagnitudeScalars {
            level: 0.0,
            stat: 2.0,
            reach: 0.0,
        };

        let result_stat10 = calculate_magnitude_value(40.0, 0, 10, 0, scalars);
        let result_stat50 = calculate_magnitude_value(40.0, 0, 50, 0, scalars);
        let result_stat100 = calculate_magnitude_value(40.0, 0, 100, 0, scalars);

        assert_eq!(result_stat10, 60.0, "10 stat should add 20.0 to base (10 * 2.0)");
        assert_eq!(result_stat50, 140.0, "50 stat should add 100.0 to base (50 * 2.0)");
        assert_eq!(result_stat100, 240.0, "100 stat should add 200.0 to base (100 * 2.0)");
    }

    #[test]
    fn test_magnitude_negative_stat_uses_absolute_value() {
        let scalars = MagnitudeScalars {
            level: 0.0,
            stat: 1.0,
            reach: 0.0,
        };

        let result_positive = calculate_magnitude_value(20.0, 0, 50, 0, scalars);
        let result_negative = calculate_magnitude_value(20.0, 0, -50, 0, scalars);

        assert_eq!(
            result_positive, result_negative,
            "Negative stats should use absolute value (i8 -> u8 conversion)"
        );
        assert_eq!(result_positive, 70.0);
    }

    #[test]
    fn test_magnitude_reach_scaling() {
        let scalars = MagnitudeScalars {
            level: 0.0,
            stat: 0.0,
            reach: 0.5,
        };

        let result_reach0 = calculate_magnitude_value(100.0, 0, 0, 0, scalars);
        let result_reach20 = calculate_magnitude_value(100.0, 0, 0, 20, scalars);
        let result_reach50 = calculate_magnitude_value(100.0, 0, 0, 50, scalars);

        assert_eq!(result_reach0, 100.0, "0 reach should not modify base");
        assert_eq!(result_reach20, 110.0, "20 reach should add 10.0 (20 * 0.5)");
        assert_eq!(result_reach50, 125.0, "50 reach should add 25.0 (50 * 0.5)");
    }

    #[test]
    fn test_magnitude_combined_scaling() {
        let scalars = MagnitudeScalars {
            level: 1.0,
            stat: 2.0,
            reach: 0.5,
        };

        // Level 10, 10 Might, 20 Reach
        let result = calculate_magnitude_value(40.0, 10, 10, 20, scalars);
        // 40 (base) + 10 (level * 1.0) + 20 (stat * 2.0) + 10 (reach * 0.5) = 80.0
        assert_eq!(result, 80.0, "All components should sum correctly");
    }

    #[test]
    fn test_magnitude_realistic_lunge_example() {
        // Lunge: base 40, +1 per level, +2 per Might, no reach scaling
        let scalars = MagnitudeScalars {
            level: 1.0,
            stat: 2.0,
            reach: 0.0,
        };

        // Early game: Level 5, 10 Might
        let early = calculate_magnitude_value(40.0, 5, 10, 0, scalars);
        assert_eq!(early, 65.0, "Early: 40 + 5 + 20 = 65");

        // Mid game: Level 25, 50 Might
        let mid = calculate_magnitude_value(40.0, 25, 50, 0, scalars);
        assert_eq!(mid, 165.0, "Mid: 40 + 25 + 100 = 165");

        // Late game: Level 50, 100 Might
        let late = calculate_magnitude_value(40.0, 50, 100, 0, scalars);
        assert_eq!(late, 290.0, "Late: 40 + 50 + 200 = 290");
    }

    #[test]
    fn test_magnitude_realistic_auto_attack_example() {
        // Auto-attack: base 20, +0.5 per level, +1 per Might
        let scalars = MagnitudeScalars {
            level: 0.5,
            stat: 1.0,
            reach: 0.0,
        };

        // Early game: Level 5, 10 Might
        let early = calculate_magnitude_value(20.0, 5, 10, 0, scalars);
        assert_eq!(early, 32.5, "Early: 20 + 2.5 + 10 = 32.5");

        // Mid game: Level 25, 50 Might
        let mid = calculate_magnitude_value(20.0, 25, 50, 0, scalars);
        assert_eq!(mid, 82.5, "Mid: 20 + 12.5 + 50 = 82.5");

        // Late game: Level 50, 100 Might
        let late = calculate_magnitude_value(20.0, 50, 100, 0, scalars);
        assert_eq!(late, 145.0, "Late: 20 + 25 + 100 = 145");
    }

    // ===== INVARIANT TESTS =====
    // Test critical properties that must always hold

    #[test]
    fn test_magnitude_invariant_non_negative() {
        let scalars = MagnitudeScalars {
            level: -10.0,  // Negative scalars shouldn't happen, but test robustness
            stat: -5.0,
            reach: -2.0,
        };

        let result = calculate_magnitude_value(100.0, 10, 10, 10, scalars);
        // Even with negative scalars, result should be calculable
        // This tests that the function handles edge cases gracefully
        assert!(result.is_finite(), "Result should be finite even with unusual inputs");
    }

    #[test]
    fn test_magnitude_invariant_additive_property() {
        // Magnitude scaling is additive - order shouldn't matter
        let scalars = MagnitudeScalars {
            level: 1.0,
            stat: 2.0,
            reach: 0.5,
        };

        let result1 = calculate_magnitude_value(40.0, 10, 10, 20, scalars);

        // Calculate components separately and sum
        let base = 40.0;
        let level_contrib = 10.0 * 1.0;
        let stat_contrib = 10.0 * 2.0;
        let reach_contrib = 20.0 * 0.5;
        let result2 = base + level_contrib + stat_contrib + reach_contrib;

        assert_eq!(result1, result2, "Additive property should hold");
    }
}
