use bevy::prelude::*;

use crate::common::components::recovery::GlobalRecovery;
use crate::common::components::ActorAttributes;

/// Calculate Composure-based recovery time reduction percentage.
///
/// Pattern 1 (Nullifying): base × gap × contest_factor
/// Base: 33%, Cap: 33%
///
/// Returns reduction percentage (0.0 to 0.33).
/// Caller converts to speed multiplier: 1.0 / (1.0 - reduction)
pub fn calculate_composure_reduction(
    composure: u16,
    target_impact: u16,
    defender_level: u32,
    attacker_level: u32,
) -> f32 {
    use crate::common::systems::combat::damage::{gap_factor, contest_factor};

    const BASE_REDUCTION: f32 = 0.33;
    const MAX_REDUCTION: f32 = 0.33;

    let base = BASE_REDUCTION;
    let gap = gap_factor(defender_level, attacker_level);
    let contest = contest_factor(composure, target_impact);

    (base * gap * contest).min(MAX_REDUCTION)
}

/// System to tick down the global recovery timer.
/// Applies Composure-based time reduction with gap and contest modifiers.
pub fn global_recovery_system(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery, &ActorAttributes)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery, attrs) in query.iter_mut() {
        if recovery.is_active() {
            let composure = attrs.composure();
            let reduction_pct = calculate_composure_reduction(
                composure,
                recovery.target_impact,
                attrs.total_level(),
                recovery.target_level,
            );

            // Convert reduction percentage to speed multiplier
            // 33% reduction → 1.0 / 0.67 = 1.49× speed
            let speed_multiplier = if reduction_pct >= 0.999 {
                100.0 // Cap to prevent division by zero
            } else {
                1.0 / (1.0 - reduction_pct)
            };

            let effective_delta = delta * speed_multiplier;

            recovery.tick(effective_delta);

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

    #[test]
    fn test_system_logic_ticks_down_recovery() {
        let mut recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);
        recovery.tick(0.3);
        assert!((recovery.remaining - 0.7).abs() < 0.001);
        assert!(recovery.is_active());
    }

    #[test]
    fn test_system_logic_marks_inactive_when_expired() {
        let mut recovery = GlobalRecovery::new(0.5, AbilityType::Lunge);
        recovery.tick(0.6);
        assert_eq!(recovery.remaining, 0.0);
        assert!(!recovery.is_active());
    }

    #[test]
    fn test_composure_reduction_zero() {
        let reduction = calculate_composure_reduction(0, 0, 10, 10);
        assert!((reduction - 0.0).abs() < 0.001, "0 composure → 0% reduction, got {reduction}");
    }

    #[test]
    fn test_composure_reduction_nullifies_at_equal() {
        // Equal level, equal stats: contest = 0 → nullified
        let reduction = calculate_composure_reduction(100, 100, 10, 10);
        assert!((reduction - 0.0).abs() < 0.001, "Equal stats → 0% reduction, got {reduction}");
    }
}
