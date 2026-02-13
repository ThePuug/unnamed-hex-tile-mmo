use bevy::prelude::*;
use crate::common::{
    components::{heal::PendingHeal, resources::Health, ActorAttributes, Loc},
    message::{Do, Event as GameEvent},
};

/// Apply healing events to targets (SOW-021 Phase 3)
/// Handles Dominance-based healing reduction aura
pub fn apply_healing_system(
    mut commands: Commands,
    mut heal_query: Query<(Entity, &PendingHeal)>,
    mut health_query: Query<&mut Health>,
    attrs_query: Query<&ActorAttributes>,
    loc_query: Query<&Loc>,
    mut writer: MessageWriter<Do>,
) {
    for (heal_entity, pending_heal) in heal_query.iter() {
        // Get target's health
        let Ok(mut health) = health_query.get_mut(pending_heal.target) else {
            // Target doesn't have health, remove heal event
            commands.entity(heal_entity).despawn();
            continue;
        };

        // Calculate Dominance reduction (SOW-021 Phase 3)
        let reduction = calculate_dominance_reduction(
            pending_heal.target,
            &loc_query,
            &attrs_query,
        );

        // Apply reduction to heal amount
        let effective_heal = pending_heal.amount * (1.0 - reduction);

        // Apply healing (cap at max health)
        let old_health = health.state;
        health.state = (health.state + effective_heal).min(health.max);
        let actual_heal = health.state - old_health;

        // Broadcast healing event to clients
        if actual_heal > 0.0 {
            writer.write(Do {
                event: GameEvent::Heal {
                    target: pending_heal.target,
                    amount: actual_heal,
                },
            });

            // Update incremental health state
            writer.write(Do {
                event: GameEvent::Incremental {
                    ent: pending_heal.target,
                    component: crate::common::message::Component::Health(*health),
                },
            });
        }

        // Remove the pending heal
        commands.entity(heal_entity).despawn();
    }
}

/// Calculate Dominance-based healing reduction (SOW-021 Phase 3)
/// Scan 5-hex radius for enemies with Dominance
/// Dominance vs Toughness contest determines reduction strength
/// Worst-effect-wins (highest Dominance applies)
fn calculate_dominance_reduction(
    target: Entity,
    loc_query: &Query<&Loc>,
    attrs_query: &Query<&ActorAttributes>,
) -> f32 {
    use crate::common::systems::combat::damage::contest_modifier;

    const RADIUS: i32 = 5;
    const BASE_REDUCTION: f32 = 0.25; // 25% base reduction
    const MAX_REDUCTION: f32 = 0.50; // Cap at 50%

    // Get target's location and toughness
    let Ok(target_loc) = loc_query.get(target) else {
        return 0.0;
    };
    let Ok(target_attrs) = attrs_query.get(target) else {
        return 0.0;
    };
    let target_toughness = target_attrs.toughness();

    // Find highest Dominance within range
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

    if max_dominance == 0 {
        return 0.0; // No Dominance in range
    }

    // Calculate reduction using Dominance vs Toughness contest
    // contest_modifier returns [0.0, 1.0] multiplier (0.0 = equal/disadvantage, 1.0 = max advantage)
    let contest_mod = contest_modifier(max_dominance, target_toughness);
    let reduction = BASE_REDUCTION * contest_mod;
    reduction.min(MAX_REDUCTION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{
        components::ActorAttributes,
        systems::combat::damage::contest_modifier,
    };

    /// Calculate expected reduction for Dominance vs Toughness contest
    fn expected_reduction_contest(dominance: u16, toughness: u16) -> f32 {
        const BASE_REDUCTION: f32 = 0.25;
        const MAX_REDUCTION: f32 = 0.50;

        if dominance == 0 {
            return 0.0;
        }

        let contest_mod = contest_modifier(dominance, toughness);
        (BASE_REDUCTION * contest_mod).min(MAX_REDUCTION)
    }

    #[test]
    fn test_reduction_zero_dominance() {
        // Zero Dominance should give zero reduction regardless of Toughness
        assert_eq!(expected_reduction_contest(0, 0), 0.0);
        assert_eq!(expected_reduction_contest(0, 100), 0.0);
        assert_eq!(expected_reduction_contest(0, 500), 0.0);
    }

    #[test]
    fn test_reduction_neutral_contest() {
        // Equal Dominance/Toughness → contest_modifier = 0.0 → no reduction (negated)
        assert_eq!(expected_reduction_contest(100, 100), 0.0);
        assert_eq!(expected_reduction_contest(200, 200), 0.0);
    }

    #[test]
    fn test_reduction_dominance_advantage() {
        // High Dominance vs low Toughness → stronger reduction
        // Dominance 200 vs Toughness 100: contest_mod > 0.0 (advantage)
        let reduction = expected_reduction_contest(200, 100);
        assert!(reduction > 0.0, "High Dominance should provide reduction");
    }

    #[test]
    fn test_reduction_toughness_resistance() {
        // Low Dominance vs high Toughness → no reduction (defender advantage)
        // Dominance 100 vs Toughness 200: contest_mod = 0.0 (negated)
        let reduction = expected_reduction_contest(100, 200);
        assert_eq!(reduction, 0.0, "High Toughness should completely negate healing reduction");
    }

    #[test]
    fn test_reduction_capped_at_50_percent() {
        // Maximum Dominance advantage should cap at 50% reduction
        // contest_modifier caps at 1.0, so 0.25 * 1.0 = 0.25, already below 0.50 cap
        // The cap is for future-proofing if BASE_REDUCTION changes
        let reduction = expected_reduction_contest(1000, 0);
        assert!(reduction <= 0.50, "Reduction should be capped at 50%, got {}", reduction);
        assert!((reduction - 0.25).abs() < 0.01, "Max contest (1.0) × base (0.25) = 0.25");
    }

    #[test]
    fn test_reduction_minimum_floor() {
        // Minimum reduction (high Toughness, low Dominance)
        // contest_modifier floors at 0.0 (defender advantage), so reduction = 0.0
        let reduction = expected_reduction_contest(100, 500);
        assert_eq!(reduction, 0.0, "Defender advantage should give 0.0 reduction");
    }

    #[test]
    fn test_healing_reduction_application() {
        // Test that healing is correctly reduced based on contest
        let base_heal = 100.0;

        // Neutral contest (100 vs 100): 0% reduction (equal stats cancel)
        let reduction = expected_reduction_contest(100, 100);
        let effective = base_heal * (1.0 - reduction);
        assert!((effective - 100.0).abs() < 1.0, "Neutral contest should give full 100hp healed");

        // High Dominance (200 vs 100): >0% reduction
        let reduction_high = expected_reduction_contest(200, 100);
        let effective_high = base_heal * (1.0 - reduction_high);
        assert!(effective_high < 100.0, "High Dominance should heal less than neutral");

        // High Toughness (100 vs 200): 0% reduction (defender advantage)
        let reduction_low = expected_reduction_contest(100, 200);
        let effective_low = base_heal * (1.0 - reduction_low);
        assert!((effective_low - 100.0).abs() < 1.0, "High Toughness should give full healing");
    }

    // Integration tests for calculate_dominance_reduction would require full ECS setup
    // with entities, components, and queries. Those are better suited for end-to-end tests
    // or integration test suites. For now, we verify the contest formula logic above.
}
