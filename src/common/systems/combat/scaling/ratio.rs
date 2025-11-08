//! Ratio scaling calculations
//!
//! Ratio scaling: effectiveness determined by comparison between actors.
//! Your investment vs. their investment/level.
//! Used for: counter reflection, mitigation penetration, CC duration.
//!
//! Formula:
//! - your_power = sqrt((your_stat / 2) / your_level)
//! - their_power = sqrt((their_stat / 2) / their_level) * level_factor
//! - ratio = your_power / their_resistance
//! - effectiveness = base * (base_multiplier + ratio * ratio_scale)

use super::types::RatioConfig;

/// Calculate ratio-based effectiveness for contested interactions
///
/// Determines effectiveness by comparing your investment to opponent's.
/// Both specialization and level difference matter.
///
/// # Arguments
/// * `your_stat` - Your relevant attribute stat
/// * `your_level` - Your character level
/// * `their_stat` - Opponent's relevant attribute stat
/// * `their_level` - Opponent's character level
/// * `config` - Configuration for ratio calculation
///
/// # Returns
/// Effectiveness value (e.g., reflection %, mitigation %)
///
/// # Example
/// ```
/// use unnamed_hex_tile_mmo::common::systems::combat::scaling::{
///     calculate_ratio_effectiveness, RatioConfig
/// };
///
/// let config = RatioConfig {
///     base: 0.5,              // 50% base reflection
///     base_multiplier: 1.0,
///     ratio_scale: 0.5,
///     max_ratio: 2.0,
///     min_resistance: 0.5,
///     level_matters: true,    // Higher level enemies harder to counter
/// };
///
/// // Your Grace vs their Might (same level, equal investment)
/// let reflection = calculate_ratio_effectiveness(20, 10, 20, 10, config);
/// // Equal matchup = base effectiveness
/// ```
pub fn calculate_ratio_effectiveness(
    your_stat: i8,
    your_level: u32,
    their_stat: i8,
    their_level: u32,
    config: RatioConfig,
) -> f32 {
    // Your skill/power
    let your_investment = ((your_stat.abs() as f32 / 2.0) / your_level as f32)
        .min(config.max_ratio);
    let your_power = your_investment.sqrt(); // Diminishing returns

    // Their resistance/defense
    let their_investment = ((their_stat.abs() as f32 / 2.0) / their_level as f32)
        .min(config.max_ratio);
    let their_power = their_investment.sqrt();

    // Level factor (experience advantage)
    let level_factor = if config.level_matters {
        (their_level as f32 / your_level as f32).sqrt()
    } else {
        1.0
    };

    let their_resistance = (their_power * level_factor).max(config.min_resistance);

    // Ratio determines effectiveness
    let ratio = your_power / their_resistance;

    // Apply to base value
    config.base * (config.base_multiplier + ratio * config.ratio_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== UNIT TESTS =====

    #[test]
    fn test_ratio_equal_matchup_no_level_factor() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: false,
        };

        // Equal stats, equal level
        let result = calculate_ratio_effectiveness(20, 10, 20, 10, config);

        // Both have same investment ratio and power
        // ratio = 1.0 (equal matchup)
        // effectiveness = 0.5 * (1.0 + 1.0 * 0.5) = 0.75
        assert!((result - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_ratio_your_advantage() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: false,
        };

        // You have 2x their investment
        let result = calculate_ratio_effectiveness(40, 10, 20, 10, config);

        // your_investment = (40/2)/10 = 2.0, power = sqrt(2.0) ≈ 1.414
        // their_investment = (20/2)/10 = 1.0, power = sqrt(1.0) = 1.0
        // ratio = 1.414 / 1.0 = 1.414
        // effectiveness = 0.5 * (1.0 + 1.414 * 0.5) ≈ 0.85
        assert!((result - 0.8535).abs() < 0.01);
    }

    #[test]
    fn test_ratio_their_advantage() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: false,
        };

        // They have 2x your investment
        let result = calculate_ratio_effectiveness(20, 10, 40, 10, config);

        // your_investment = 1.0, power = 1.0
        // their_investment = 2.0, power = sqrt(2.0) ≈ 1.414
        // ratio = 1.0 / 1.414 ≈ 0.707
        // effectiveness = 0.5 * (1.0 + 0.707 * 0.5) ≈ 0.677
        assert!((result - 0.677).abs() < 0.01);
    }

    #[test]
    fn test_ratio_level_matters_higher_level_opponent() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: true,
        };

        // Equal investment %, but opponent is higher level
        let result = calculate_ratio_effectiveness(10, 10, 50, 50, config);

        // Both have 50% investment, same power before level factor
        // level_factor = sqrt(50/10) = sqrt(5) ≈ 2.236
        // their_resistance is multiplied by level_factor
        // ratio < 1.0 (your disadvantage)
        // effectiveness < 0.75 (equal matchup baseline)

        // Equal investment, equal level for comparison
        let result_equal_level = calculate_ratio_effectiveness(10, 10, 10, 10, config);

        assert!(result < result_equal_level, "Higher level opponent should reduce effectiveness");
    }

    #[test]
    fn test_ratio_level_matters_lower_level_opponent() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: true,
        };

        // Equal investment %, but you are higher level
        let result = calculate_ratio_effectiveness(50, 50, 10, 10, config);

        // level_factor = sqrt(10/50) = sqrt(0.2) ≈ 0.447
        // their_resistance is reduced by level_factor
        // ratio > 1.0 (your advantage)
        // effectiveness > 0.75

        let result_equal_level = calculate_ratio_effectiveness(10, 10, 10, 10, config);

        assert!(result > result_equal_level, "Lower level opponent should increase effectiveness");
    }

    #[test]
    fn test_ratio_min_resistance_prevents_division_by_zero() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.5,
            level_matters: false,
        };

        // Opponent has 0 investment (should use min_resistance)
        let result = calculate_ratio_effectiveness(20, 10, 0, 10, config);

        // their_investment = 0, power = 0
        // their_resistance = max(0, 0.5) = 0.5 (min_resistance)
        // Calculation should succeed without division by zero
        assert!(result.is_finite(), "Should handle zero opponent stat gracefully");
        assert!(result > 0.0, "Should produce positive result with min_resistance");
    }

    #[test]
    fn test_ratio_negative_stats_use_absolute_value() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: false,
        };

        let result_positive = calculate_ratio_effectiveness(20, 10, 20, 10, config);
        let result_negative = calculate_ratio_effectiveness(-20, 10, -20, 10, config);

        assert_eq!(
            result_positive, result_negative,
            "Negative stats should use absolute value"
        );
    }

    #[test]
    fn test_ratio_max_ratio_caps_investment() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 1.0, // Cap at 100%
            min_resistance: 0.1,
            level_matters: false,
        };

        // 100% investment (capped)
        let result_100 = calculate_ratio_effectiveness(20, 10, 20, 10, config);

        // 200% investment (should be capped)
        let result_200 = calculate_ratio_effectiveness(40, 10, 40, 10, config);

        assert_eq!(
            result_100, result_200,
            "Investment should be capped at max_ratio"
        );
    }

    // ===== INVARIANT TESTS =====

    #[test]
    fn test_ratio_invariant_advantage_asymmetry() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 1.0,
            max_ratio: 2.0,
            min_resistance: 0.1,
            level_matters: false,
        };

        // Equal matchup (ratio = 1.0)
        let equal = calculate_ratio_effectiveness(20, 10, 20, 10, config);

        // A has 2x B's investment (ratio > 1.0)
        let a_vs_b = calculate_ratio_effectiveness(40, 10, 20, 10, config);

        // B has 1/2 A's investment (ratio < 1.0)
        let b_vs_a = calculate_ratio_effectiveness(20, 10, 40, 10, config);

        // Verify asymmetry: having advantage should matter more than equal matchup
        assert!(a_vs_b > equal, "Higher investment should increase effectiveness");
        assert!(b_vs_a < equal, "Lower investment should decrease effectiveness");

        // Due to sqrt diminishing returns, the asymmetry is expected
        // (having 2x investment doesn't give 2x as much benefit as having 0.5x investment hurts)
    }

    #[test]
    fn test_ratio_realistic_counter_reflection() {
        // Counter: base 50%, your Grace vs their Might
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.5,
            level_matters: true,
        };

        // Equal level, equal investment
        let equal = calculate_ratio_effectiveness(20, 10, 20, 10, config);
        // ratio = 1.0, reflection = 0.5 * (1.0 + 1.0 * 0.5) = 0.75
        assert!((equal - 0.75).abs() < 0.01);

        // You specialize in Grace (80% investment)
        let specialized = calculate_ratio_effectiveness(32, 10, 20, 10, config);
        // your_investment = (32/2)/10 = 1.6, power = sqrt(1.6) ≈ 1.265
        // their_investment = 1.0, power = 1.0
        // ratio = 1.265 / 1.0 = 1.265
        // reflection = 0.5 * (1.0 + 1.265 * 0.5) ≈ 0.816
        assert!(specialized > equal, "Specialization should increase reflection");
        assert!(specialized > 0.8, "High Grace should give >80% reflection");

        // Opponent is higher level (same % investment)
        let high_level_opponent = calculate_ratio_effectiveness(10, 10, 50, 50, config);
        // level_factor = sqrt(50/10) ≈ 2.236
        // their_resistance is multiplied by level_factor
        // ratio < 1.0, reflection < 0.75
        assert!(high_level_opponent < equal, "Higher level opponent harder to counter");
    }

    #[test]
    fn test_ratio_realistic_mitigation_penetration() {
        // Mitigation penetration: base 15%, your Might vs their Vitality
        let config = RatioConfig {
            base: 0.15,
            base_multiplier: 1.0,
            ratio_scale: 1.0,
            max_ratio: 2.0,
            min_resistance: 0.5,
            level_matters: true,
        };

        // Equal matchup
        let equal = calculate_ratio_effectiveness(20, 10, 20, 10, config);
        // ratio = 1.0, penetration = 0.15 * (1.0 + 1.0 * 1.0) = 0.30 (30%)
        assert!((equal - 0.30).abs() < 0.01);

        // High Might attacker vs low Vitality defender
        let penetration_advantage = calculate_ratio_effectiveness(40, 10, 10, 10, config);
        // Should have higher penetration
        assert!(penetration_advantage > equal);

        // Low Might attacker vs high Vitality defender
        let penetration_disadvantage = calculate_ratio_effectiveness(10, 10, 40, 10, config);
        // Should have lower penetration
        assert!(penetration_disadvantage < equal);
    }

    #[test]
    fn test_ratio_zero_investment_uses_min_resistance() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.5,
            level_matters: false,
        };

        // You have investment, opponent has none
        let result = calculate_ratio_effectiveness(20, 10, 0, 10, config);

        // your_investment = 1.0, power = 1.0
        // their_investment = 0, power = 0, resistance = min_resistance = 0.5
        // ratio = 1.0 / 0.5 = 2.0
        // effectiveness = 0.5 * (1.0 + 2.0 * 0.5) = 1.0
        assert!((result - 1.0).abs() < 0.01);
    }
}
