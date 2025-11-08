//! Three-Pole Attribute Scaling System
//!
//! Unified scaling system for combat abilities and derived stats.
//!
//! ## Three Fundamental Poles
//!
//! **1. MAGNITUDE (Absolute Power)**
//! - Raw capability that scales with absolute stat values
//! - "More is better" regardless of context
//! - Used for: Damage output, HP pools, resource capacity
//!
//! **2. COMMITMENT (Specialization Efficiency)**
//! - Effectiveness based on percentage of total investment
//! - Rewards specialization equally at all levels
//! - Used for: Cooldown reduction, attack speed, movement speed
//!
//! **3. RATIO (Contested Matchup)**
//! - Effectiveness determined by comparison between actors
//! - Your investment vs. their investment/level
//! - Used for: Counter reflection, mitigation penetration, CC duration
//!
//! ## Design Philosophy
//!
//! Each ability *component* (not ability as a whole) uses 1-2 poles.
//! For example:
//! - **Lunge** has:
//!   - Damage component (MAGNITUDE pole)
//!   - Recovery component (COMMITMENT pole)
//! - **Counter** has:
//!   - Reflection % component (COMMITMENT + RATIO poles)
//!
//! This provides:
//! - **Clarity**: Each component's scaling intent is explicit
//! - **Consistency**: All abilities follow the same framework
//! - **Balance**: Easy to compare and tune across abilities
//! - **Testability**: Pure functions, easy to unit test
//!
//! ## Usage
//!
//! ```
//! use unnamed_hex_tile_mmo::common::systems::combat::scaling::{
//!     calculate_magnitude_value, MagnitudeScalars, AUTO_ATTACK,
//! };
//!
//! // Get damage component from ability definition
//! let damage_component = AUTO_ATTACK.components
//!     .iter()
//!     .find(|c| c.name == "damage")
//!     .unwrap();
//!
//! // Calculate damage using magnitude scaling
//! // (This example shows the pattern - in practice, use match on ComponentScaling)
//! ```
//!
//! See ADR-016 for complete architectural details and migration plan.

// Module organization
pub mod commitment;
pub mod definitions;
pub mod magnitude;
pub mod ratio;
pub mod types;

// Re-export commonly used items for convenience
pub use commitment::calculate_commitment_modifier;
pub use definitions::{
    AbilityComponent, AbilityDefinition, AttributeStat, ComponentScaling,
    AUTO_ATTACK, COUNTER, LUNGE, OVERPOWER, PHYSICAL_MITIGATION,
};
pub use magnitude::calculate_magnitude_value;
pub use ratio::calculate_ratio_effectiveness;
pub use types::{
    CommitmentCurve, CurveFunction, CurveMode, MagnitudeScalars, RatioConfig,
};

#[cfg(test)]
mod tests {
    use super::*;

    // ===== INTEGRATION TESTS =====
    // Test complete ability scaling calculations

    #[test]
    fn test_auto_attack_complete_scaling() {
        let damage_component = AUTO_ATTACK.components
            .iter()
            .find(|c| c.name == "damage")
            .unwrap();

        let ComponentScaling::Magnitude { base, scalars } = damage_component.scaling else {
            panic!("Auto-attack damage must use magnitude scaling");
        };

        // Level 10, 20 Might
        let damage = calculate_magnitude_value(base, 10, 20, 0, scalars);
        // 20 (base) + 5 (level * 0.5) + 20 (might * 1.0) = 45
        assert_eq!(damage, 45.0);

        // Level 50, 100 Might
        let damage_high = calculate_magnitude_value(base, 50, 100, 0, scalars);
        // 20 + 25 + 100 = 145
        assert_eq!(damage_high, 145.0);
    }

    #[test]
    fn test_lunge_complete_scaling() {
        // Find both components
        let damage_component = LUNGE.components
            .iter()
            .find(|c| c.name == "damage")
            .unwrap();

        let recovery_component = LUNGE.components
            .iter()
            .find(|c| c.name == "recovery")
            .unwrap();

        // Calculate damage (magnitude)
        let ComponentScaling::Magnitude { base: damage_base, scalars: damage_scalars } = damage_component.scaling else {
            panic!("Lunge damage must use magnitude scaling");
        };

        let damage = calculate_magnitude_value(damage_base, 10, 30, 0, damage_scalars);
        // 40 (base) + 10 (level * 1.0) + 60 (might * 2.0) = 110
        assert_eq!(damage, 110.0);

        // Calculate recovery (commitment)
        let ComponentScaling::Commitment { curve } = recovery_component.scaling else {
            panic!("Lunge recovery must use commitment scaling");
        };

        // 50% Presence commitment (10 points = 20 stat at level 10)
        let recovery = calculate_commitment_modifier(10, 10, curve);
        // base=1.0, ratio=0.5, sqrt=0.707, reduction = 1.0 * (1 - 0.707 * 0.33) ≈ 0.766
        assert!((recovery - 0.766).abs() < 0.01);
    }

    #[test]
    fn test_counter_complete_scaling() {
        let reflection_component = COUNTER.components
            .iter()
            .find(|c| c.name == "reflection")
            .unwrap();

        let ComponentScaling::CommitmentAndRatio { commitment, ratio } = reflection_component.scaling else {
            panic!("Counter reflection must use commitment+ratio scaling");
        };

        // Your Grace: 50% commitment (10 points = 20 stat at level 10)
        let skill_modifier = calculate_commitment_modifier(10, 10, commitment);

        // Opponent: Same level, same Might investment
        let ratio_modifier = calculate_ratio_effectiveness(10, 10, 10, 10, ratio);

        // Combined reflection %
        let reflection_percent = (skill_modifier * ratio_modifier).clamp(0.25, 0.90);

        // Both should be > 0, producing meaningful reflection
        assert!(skill_modifier > 0.5);
        assert!(ratio_modifier > 0.5);
        assert!(reflection_percent > 0.5 && reflection_percent < 0.90);
    }

    #[test]
    fn test_physical_mitigation_complete_scaling() {
        let base_mit = PHYSICAL_MITIGATION.components
            .iter()
            .find(|c| c.name == "base_mitigation")
            .unwrap();

        let contested = PHYSICAL_MITIGATION.components
            .iter()
            .find(|c| c.name == "contested_bonus")
            .unwrap();

        // Base mitigation (magnitude)
        let ComponentScaling::Magnitude { base, scalars } = base_mit.scaling else {
            panic!("Base mitigation must use magnitude scaling");
        };

        let base_mitigation = calculate_magnitude_value(base, 0, 50, 0, scalars);
        // 0 + 0 + (50 * 0.005) = 0.25 (25% mitigation)
        assert_eq!(base_mitigation, 0.25);

        // Contested bonus (ratio)
        let ComponentScaling::MagnitudeAndRatio { ratio, .. } = contested.scaling else {
            panic!("Contested mitigation must use magnitude+ratio scaling");
        };

        // Your Vitality vs attacker's Might (equal matchup)
        let contested_bonus = calculate_ratio_effectiveness(25, 10, 25, 10, ratio);

        // Should produce bonus mitigation
        assert!(contested_bonus > 0.15);
    }

    // ===== ARCHITECTURAL INVARIANT TESTS =====

    #[test]
    fn test_all_abilities_have_primary_stat() {
        let definitions = [&AUTO_ATTACK, &LUNGE, &OVERPOWER, &COUNTER];

        for def in definitions {
            for component in def.components {
                // Every component must specify which stat scales it
                match component.primary_stat {
                    AttributeStat::Might
                    | AttributeStat::Grace
                    | AttributeStat::Vitality
                    | AttributeStat::Focus
                    | AttributeStat::Instinct
                    | AttributeStat::Presence => {},
                }
            }
        }
    }

    #[test]
    fn test_contested_components_have_secondary_stat() {
        // Counter uses contested scaling - must have secondary stat
        let reflection = &COUNTER.components[0];
        match reflection.scaling {
            ComponentScaling::CommitmentAndRatio { .. }
            | ComponentScaling::MagnitudeAndRatio { .. } => {
                assert!(
                    reflection.secondary_stat.is_some(),
                    "Contested scaling requires secondary stat (opponent's stat)"
                );
            }
            _ => panic!("Counter should use contested scaling"),
        }
    }

    #[test]
    fn test_magnitude_components_dont_need_secondary_stat() {
        // Auto-attack uses pure magnitude - no secondary stat needed
        let damage = &AUTO_ATTACK.components[0];
        match damage.scaling {
            ComponentScaling::Magnitude { .. } => {
                assert!(
                    damage.secondary_stat.is_none(),
                    "Pure magnitude scaling shouldn't have secondary stat"
                );
            }
            _ => panic!("Auto-attack should use pure magnitude scaling"),
        }
    }

    #[test]
    fn test_scaling_calculations_are_deterministic() {
        // Same inputs should always produce same outputs
        let scalars = MagnitudeScalars {
            level: 1.0,
            stat: 2.0,
            reach: 0.0,
        };

        let result1 = calculate_magnitude_value(40.0, 10, 20, 0, scalars);
        let result2 = calculate_magnitude_value(40.0, 10, 20, 0, scalars);

        assert_eq!(result1, result2, "Scaling calculations must be deterministic");
    }
}
