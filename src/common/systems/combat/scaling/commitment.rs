//! Commitment scaling calculations
//!
//! Commitment scaling: effectiveness based on percentage of total investment.
//! Rewards specialization equally at all levels.
//! Used for: cooldown reduction, attack speed, movement speed.
//!
//! Formula: investment_ratio = (stat / 2) / total_level
//! (1 point = 2 axis movement, so stat/2 = points spent)

use super::types::{CommitmentCurve, CurveFunction, CurveMode};

/// Calculate commitment-based modifier
///
/// Produces efficiency modifiers based on investment ratio.
/// A 50% commitment at level 10 produces the same result as 50% at level 50.
///
/// # Arguments
/// * `stat` - Attribute stat value (negative values return base)
/// * `total_level` - Total character level (sum of all level-ups)
/// * `curve` - Configuration for curve shape and application
///
/// # Returns
/// Modified value based on investment ratio
///
/// # Example
/// ```
/// use unnamed_hex_tile_mmo::common::systems::combat::scaling::{
///     calculate_commitment_modifier, CommitmentCurve, CurveFunction, CurveMode
/// };
///
/// // 33% cooldown reduction at max (sqrt diminishing returns)
/// let curve = CommitmentCurve {
///     base: 1.0,
///     scale: 1.0 / 3.0,
///     max_ratio: 2.0,
///     function: CurveFunction::Sqrt,
///     mode: CurveMode::Reduction,
/// };
///
/// // 50% commitment (10 points at level 10)
/// let recovery = calculate_commitment_modifier(10, 10, curve);
/// // 1.0 * (1 - sqrt(0.5) * 0.33) ≈ 0.76 (24% faster recovery)
/// ```
pub fn calculate_commitment_modifier(
    stat: i8,
    total_level: u32,
    curve: CommitmentCurve,
) -> f32 {
    // Negative stats return base value (no investment)
    if stat <= 0 {
        return curve.base;
    }

    // Investment ratio: (points spent) / (points available)
    // 1 point = 2 axis movement, so stat/2 = points spent
    let investment_ratio = ((stat as f32 / 2.0) / total_level as f32)
        .min(curve.max_ratio);

    // Apply curve function (linear, sqrt, square)
    let modified_ratio = match curve.function {
        CurveFunction::Linear => investment_ratio,
        CurveFunction::Sqrt => investment_ratio.sqrt(),
        CurveFunction::Square => investment_ratio.powi(2),
    };

    // Apply to base value based on mode
    match curve.mode {
        CurveMode::Additive => curve.base + (modified_ratio * curve.scale),
        CurveMode::Multiplicative => curve.base * (1.0 + modified_ratio * curve.scale),
        CurveMode::Reduction => curve.base * (1.0 - modified_ratio * curve.scale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== UNIT TESTS =====

    #[test]
    fn test_commitment_zero_stat_returns_base() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 0.5,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Additive,
        };

        let result = calculate_commitment_modifier(0, 10, curve);
        assert_eq!(result, 1.0, "Zero stat should return base value");
    }

    #[test]
    fn test_commitment_negative_stat_returns_base() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 0.5,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Additive,
        };

        let result = calculate_commitment_modifier(-50, 10, curve);
        assert_eq!(result, 1.0, "Negative stat should return base value");
    }

    #[test]
    fn test_commitment_linear_additive() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Additive,
        };

        // 50% commitment (10 stat points at level 10)
        let result = calculate_commitment_modifier(10, 10, curve);
        // investment_ratio = (10/2) / 10 = 0.5
        // linear: 0.5
        // additive: 1.0 + (0.5 * 1.0) = 1.5
        assert_eq!(result, 1.5);
    }

    #[test]
    fn test_commitment_linear_multiplicative() {
        let curve = CommitmentCurve {
            base: 100.0,
            scale: 0.5,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Multiplicative,
        };

        // 50% commitment
        let result = calculate_commitment_modifier(10, 10, curve);
        // investment_ratio = 0.5
        // linear: 0.5
        // multiplicative: 100.0 * (1 + 0.5 * 0.5) = 100.0 * 1.25 = 125.0
        assert_eq!(result, 125.0);
    }

    #[test]
    fn test_commitment_linear_reduction() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 0.5,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Reduction,
        };

        // 50% commitment
        let result = calculate_commitment_modifier(10, 10, curve);
        // investment_ratio = 0.5
        // linear: 0.5
        // reduction: 1.0 * (1 - 0.5 * 0.5) = 1.0 * 0.75 = 0.75
        assert_eq!(result, 0.75);
    }

    #[test]
    fn test_commitment_sqrt_diminishing_returns() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0,
            max_ratio: 2.0,
            function: CurveFunction::Sqrt,
            mode: CurveMode::Additive,
        };

        // 25% commitment
        let result_25 = calculate_commitment_modifier(5, 10, curve);
        // investment_ratio = (5/2) / 10 = 0.25
        // sqrt: 0.5
        // additive: 1.0 + 0.5 = 1.5

        // 100% commitment
        let result_100 = calculate_commitment_modifier(20, 10, curve);
        // investment_ratio = (20/2) / 10 = 1.0
        // sqrt: 1.0
        // additive: 1.0 + 1.0 = 2.0

        assert!((result_25 - 1.5).abs() < 0.001);
        assert!((result_100 - 2.0).abs() < 0.001);

        // Verify diminishing returns: 4x investment doesn't give 4x benefit
        let benefit_25 = result_25 - 1.0; // 0.5
        let benefit_100 = result_100 - 1.0; // 1.0
        assert!(benefit_100 / benefit_25 < 4.0, "Sqrt should show diminishing returns");
    }

    #[test]
    fn test_commitment_square_accelerating_returns() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0,
            max_ratio: 2.0,
            function: CurveFunction::Square,
            mode: CurveMode::Additive,
        };

        // 50% commitment
        let result_50 = calculate_commitment_modifier(10, 10, curve);
        // investment_ratio = 0.5
        // square: 0.25
        // additive: 1.0 + 0.25 = 1.25

        // 100% commitment
        let result_100 = calculate_commitment_modifier(20, 10, curve);
        // investment_ratio = 1.0
        // square: 1.0
        // additive: 1.0 + 1.0 = 2.0

        assert_eq!(result_50, 1.25);
        assert_eq!(result_100, 2.0);

        // Verify accelerating returns
        let benefit_50 = result_50 - 1.0; // 0.25
        let benefit_100 = result_100 - 1.0; // 1.0
        assert!(benefit_100 / benefit_50 > 2.0, "Square should show accelerating returns");
    }

    // ===== INVARIANT TESTS =====

    #[test]
    fn test_commitment_invariant_same_ratio_same_level() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0 / 3.0,
            max_ratio: 2.0,
            function: CurveFunction::Sqrt,
            mode: CurveMode::Reduction,
        };

        // 50% commitment at level 10 (5 points = 10 stat)
        let result_l10 = calculate_commitment_modifier(10, 10, curve);

        // 50% commitment at level 50 (25 points = 50 stat)
        let result_l50 = calculate_commitment_modifier(50, 50, curve);

        assert!(
            (result_l10 - result_l50).abs() < 0.001,
            "Same commitment % should give same result regardless of level: {} vs {}",
            result_l10, result_l50
        );
    }

    #[test]
    fn test_commitment_invariant_cap_at_max_ratio() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0,
            max_ratio: 1.0, // Cap at 100% investment
            function: CurveFunction::Linear,
            mode: CurveMode::Additive,
        };

        // 100% commitment (capped)
        let result_100 = calculate_commitment_modifier(20, 10, curve);

        // 200% commitment (should be capped at max_ratio)
        let result_200 = calculate_commitment_modifier(40, 10, curve);

        assert_eq!(
            result_100, result_200,
            "Investment should cap at max_ratio"
        );
    }

    #[test]
    fn test_commitment_realistic_recovery_reduction() {
        // Lunge recovery: base 1.0s, up to 33% reduction with Presence
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0 / 3.0,
            max_ratio: 2.0,
            function: CurveFunction::Sqrt,
            mode: CurveMode::Reduction,
        };

        // No Presence
        let recovery_0 = calculate_commitment_modifier(0, 10, curve);
        assert_eq!(recovery_0, 1.0, "No investment = base recovery");

        // 25% commitment (2.5 points = 5 Presence at level 10)
        let recovery_25 = calculate_commitment_modifier(5, 10, curve);
        // ratio = 0.25, sqrt = 0.5, reduction = 1.0 * (1 - 0.5 * 0.333...) ≈ 0.8333
        assert!((recovery_25 - 0.8333).abs() < 0.001);

        // 100% commitment (10 points = 20 Presence at level 10)
        let recovery_100 = calculate_commitment_modifier(20, 10, curve);
        // ratio = 1.0, sqrt = 1.0, reduction = 1.0 * (1 - 1.0 * 0.33) ≈ 0.67
        assert!((recovery_100 - 0.67).abs() < 0.01);

        // Full investment is faster than no investment
        assert!(recovery_100 < recovery_0);
    }

    #[test]
    fn test_commitment_realistic_attack_speed() {
        // Attack speed: base 1.0 (100%), up to +50% with Instinct
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 0.5,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Multiplicative,
        };

        // No Instinct
        let speed_0 = calculate_commitment_modifier(0, 10, curve);
        assert_eq!(speed_0, 1.0);

        // 50% commitment
        let speed_50 = calculate_commitment_modifier(10, 10, curve);
        // ratio = 0.5, linear = 0.5, mult = 1.0 * (1 + 0.5 * 0.5) = 1.25
        assert_eq!(speed_50, 1.25);

        // 100% commitment
        let speed_100 = calculate_commitment_modifier(20, 10, curve);
        // ratio = 1.0, linear = 1.0, mult = 1.0 * (1 + 1.0 * 0.5) = 1.5
        assert_eq!(speed_100, 1.5);
    }

    #[test]
    fn test_commitment_zero_level_edge_case() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 0.5,
            max_ratio: 2.0,
            function: CurveFunction::Linear,
            mode: CurveMode::Additive,
        };

        // Level 0 would cause division by zero - should be handled gracefully
        // In practice, level should always be >= 1, but test robustness
        let result = calculate_commitment_modifier(10, 0, curve);

        // When total_level is 0, investment_ratio becomes inf, capped at max_ratio
        // This tests that the min(max_ratio) cap works correctly
        assert!(result.is_finite(), "Should handle level 0 gracefully");
    }
}
