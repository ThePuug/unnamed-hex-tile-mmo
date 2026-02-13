use bevy::prelude::*;

use crate::common::components::recovery::GlobalRecovery;
use crate::common::components::ActorAttributes;

/// Calculate Composure-based recovery reduction factor (SOW-021 Phase 2)
///
/// Formula: base_reduction = composure × 0.5%
///          contested_reduction = base_reduction × contest_modifier(composure, target_impact)
///          final_reduction = contested_reduction.min(150%)
/// Returns a factor that multiplies effective delta time (higher = faster recovery)
///
/// Examples (uncontested):
/// - 0 composure → 1.0 (no bonus)
/// - 100 composure → 1.645x (64.5% faster)
/// - 200 composure → 2.408x (140.8% faster)
/// - 300+ composure → 2.5x (150% faster, capped)
pub fn calculate_composure_reduction(composure: u16, target_impact: u16) -> f32 {
    use crate::common::systems::combat::damage::contest_modifier;

    const K: f32 = 0.005; // 0.5% per point
    const MAX_REDUCTION: f32 = 1.50; // Cap at 150% faster recovery

    // Calculate base reduction from raw stat
    let base_reduction = (composure as f32) * K;

    // Apply contest modifier directly to benefit
    let contest_mod = contest_modifier(composure, target_impact);
    let contested_reduction = base_reduction * contest_mod;

    let reduction = contested_reduction.min(MAX_REDUCTION);
    1.0 + reduction
}

/// System to tick down the global recovery timer
/// Runs every frame and decrements the remaining lockout time
/// Applies Composure-based reduction (SOW-021 Phase 1)
/// Removes GlobalRecovery component when lockout expires
pub fn global_recovery_system(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery, &ActorAttributes)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery, attrs) in query.iter_mut() {
        if recovery.is_active() {
            // Apply Composure-based reduction contested by target's Impact
            let composure = attrs.composure();
            let reduction_factor = calculate_composure_reduction(composure, recovery.target_impact);
            let effective_delta = delta * reduction_factor;

            recovery.tick(effective_delta);

            // Lockout expired, remove component
            if !recovery.is_active() {
                commands.entity(entity).remove::<GlobalRecovery>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::message::AbilityType;

    // Note: Following DEVELOPER role guidance to write durable unit tests instead of brittle
    // integration tests. These tests verify the system logic by manually calling tick(),
    // avoiding coupling to ECS implementation details.

    #[test]
    fn test_system_logic_ticks_down_recovery() {
        // Test the core logic: ticking down recovery
        let mut recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);

        // Simulate frame with 0.3s delta
        recovery.tick(0.3);

        assert!(
            (recovery.remaining - 0.7).abs() < 0.001,
            "Expected 0.7s remaining, got {}",
            recovery.remaining
        );
        assert!(recovery.is_active());
    }

    #[test]
    fn test_system_logic_marks_inactive_when_expired() {
        // Test that recovery becomes inactive when expired
        let mut recovery = GlobalRecovery::new(0.5, AbilityType::Lunge);

        // Simulate frame with 0.6s delta (past expiry)
        recovery.tick(0.6);

        assert_eq!(recovery.remaining, 0.0);
        assert!(!recovery.is_active(), "Recovery should be inactive after expiry");
    }

    #[test]
    fn test_system_logic_clamps_to_zero() {
        // Test that recovery doesn't go negative
        let mut recovery = GlobalRecovery::new(0.1, AbilityType::Deflect);

        // Simulate large delta
        recovery.tick(5.0);

        assert_eq!(recovery.remaining, 0.0, "Recovery should clamp to 0, not go negative");
        assert!(!recovery.is_active());
    }

    #[test]
    fn test_system_logic_preserves_triggered_by() {
        // Test that triggered_by is preserved during ticking
        let mut recovery = GlobalRecovery::new(1.0, AbilityType::Overpower);

        recovery.tick(0.4);
        assert_eq!(
            recovery.triggered_by,
            AbilityType::Overpower,
            "triggered_by should be preserved during ticking"
        );

        recovery.tick(0.6);
        assert_eq!(
            recovery.triggered_by,
            AbilityType::Overpower,
            "triggered_by should be preserved even after expiring"
        );
    }

    #[test]
    fn test_system_logic_multiple_ticks() {
        // Test multiple tick calls accumulate correctly
        let mut recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);

        recovery.tick(0.5);
        assert!((recovery.remaining - 1.5).abs() < 0.001);
        assert!(recovery.is_active());

        recovery.tick(0.7);
        assert!((recovery.remaining - 0.8).abs() < 0.001);
        assert!(recovery.is_active());

        recovery.tick(1.0);
        assert_eq!(recovery.remaining, 0.0);
        assert!(!recovery.is_active());
    }

    // ===== COMPOSURE REDUCTION TESTS (SOW-021 Phase 1) =====

    #[test]
    fn test_composure_reduction_zero() {
        // 0 Composure → no reduction (1.0x multiplier)
        let factor = calculate_composure_reduction(0, 0);
        assert!((factor - 1.0).abs() < 0.001, "Expected 1.0, got {factor}");
    }

    #[test]
    fn test_composure_reduction_linear() {
        // Composure vs 0 Impact: contest = sqrt(delta/300)
        // 100 composure: contest = sqrt(100/300) = 0.577, benefit = 100 * 0.5% * 0.577 = 28.9%
        let factor_100 = calculate_composure_reduction(100, 0);
        assert!((factor_100 - 1.289).abs() < 0.01, "Expected ~1.289 (28.9% faster), got {factor_100}");

        // 200 composure: contest = sqrt(200/300) = 0.816, benefit = 200 * 0.5% * 0.816 = 81.6%
        let factor_200 = calculate_composure_reduction(200, 0);
        assert!((factor_200 - 1.816).abs() < 0.01, "Expected ~1.816 (81.6% faster), got {factor_200}");
    }

    #[test]
    fn test_composure_reduction_capped() {
        // 300 composure vs 0 Impact: contest = 1.0, benefit = 300 * 0.5% * 1.0 = 150% (cap)
        let factor_300 = calculate_composure_reduction(300, 0);
        assert!((factor_300 - 2.5).abs() < 0.001, "Expected 2.5 (150% cap), got {factor_300}");

        let factor_max = calculate_composure_reduction(u16::MAX, 0);
        assert!((factor_max - 2.5).abs() < 0.001, "Expected 2.5 (150% cap), got {factor_max}");
    }

    #[test]
    fn test_composure_reduces_recovery_time() {
        // Simulate recovery with 100 composure vs 0 Impact (~28.9% faster = 1.289x)
        let mut recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);
        let factor = calculate_composure_reduction(100, 0);

        // Tick with 0.5s delta, but effective delta is ~0.645s due to composure
        let effective_delta = 0.5 * factor; // 0.5 * 1.289 = 0.645
        recovery.tick(effective_delta);

        assert!((recovery.remaining - 0.355).abs() < 0.01, "Expected ~0.355s remaining, got {}", recovery.remaining);
    }

    #[test]
    fn test_composure_vs_pushback_integration() {
        // Test that Composure reduction can counteract Impact pushback
        let mut recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);

        // Apply 25% pushback (0.5s extension on 2.0s duration)
        recovery.apply_pushback(0.25);
        assert!((recovery.remaining - 2.5).abs() < 0.001, "Expected 2.5s after pushback");

        // Now tick with high composure (150% faster = 2.5x)
        let factor = calculate_composure_reduction(300, 0);
        let effective_delta = 1.0 * factor; // 1.0 * 2.5 = 2.5s
        recovery.tick(effective_delta);

        // Should be at 0.0s (2.5 - 2.5)
        assert!((recovery.remaining - 0.0).abs() < 0.001, "Expected 0.0s remaining, got {}", recovery.remaining);
    }

    #[test]
    fn test_composure_complete_recovery_faster() {
        // Verify that high composure allows faster recovery
        let mut no_composure = GlobalRecovery::new(1.0, AbilityType::Lunge);
        let mut high_composure = GlobalRecovery::new(1.0, AbilityType::Lunge);

        let composure_factor = calculate_composure_reduction(200, 0); // 1.816x (81.6% faster)

        // Both tick with 0.5s real delta
        no_composure.tick(0.5);
        high_composure.tick(0.5 * composure_factor); // 0.908s effective

        // High composure should have less remaining
        assert!((no_composure.remaining - 0.5).abs() < 0.001);
        assert!((high_composure.remaining - 0.092).abs() < 0.01);
        assert!(high_composure.remaining < no_composure.remaining, "High composure should recover faster");
    }
}
