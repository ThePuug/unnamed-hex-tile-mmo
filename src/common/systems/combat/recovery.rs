use bevy::prelude::*;

use crate::common::components::recovery::GlobalRecovery;

/// System to tick down the global recovery timer
/// Runs every frame and decrements the remaining lockout time
/// Removes GlobalRecovery component when lockout expires
pub fn global_recovery_system(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery) in query.iter_mut() {
        if recovery.is_active() {
            recovery.tick(delta);

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
        let mut recovery = GlobalRecovery::new(0.5, AbilityType::Knockback);

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
}
