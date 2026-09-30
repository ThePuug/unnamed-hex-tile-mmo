use bevy::prelude::*;

use crate::components::recovery::{Combo, GlobalRecovery};
use crate::components::ActorAttributes;

/// Calculate Composure-based recovery time reduction percentage.

/// Pattern 1 (Nullifying): 33% × contest_factor(Composure, target's Impact),
/// with no ceiling.

/// Returns reduction percentage, 0 and up.
/// Caller converts to speed multiplier: 1.0 / (1.0 - reduction), a lockout
/// all but gone at 100%.
pub fn calculate_composure_reduction(
    composure: u16,
    target_impact: u16,
    edge: f32,
) -> f32 {
    use crate::systems::combat::damage::contest_factor;

    crate::tuning::tuning().composure_share * contest_factor(composure, target_impact, edge)
}

/// Counts every lockout down and ends it when it runs out, its offer and
/// its combo with it: faster by its actor's Composure, contested by the
/// opponent's Impact with the level gap weighing in, and slower by a daze's
/// pace. A combo's window counts down in plain seconds beside it.
pub fn global_recovery_system(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery, &ActorAttributes, Option<&crate::components::status::Status>)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery, attrs, status) in query.iter_mut() {
        if recovery.is_active() {
            let composure = attrs.composure();
            let reduction_pct = calculate_composure_reduction(
                composure,
                recovery.target_impact,
                recovery.target_level.map_or(0.0, |target| crate::systems::combat::damage::level_edge(attrs.total_level(), target)),
            );

            // Convert reduction percentage to speed multiplier
            // 33% reduction → 1.0 / 0.67 = 1.49× speed
            let speed_multiplier = if reduction_pct >= 0.999 {
                100.0 // Cap to prevent division by zero
            } else {
                1.0 / (1.0 - reduction_pct)
            };

            let effective_delta = delta * speed_multiplier * status.map_or(1.0, crate::components::status::Status::daze_pace);

            recovery.tick(effective_delta);
            recovery.combo = recovery.combo
                .map(|combo| Combo { window: combo.window - delta, ..combo })
                .filter(|combo| combo.window > 0.0);

            if !recovery.is_active() {
                commands.entity(entity).remove::<GlobalRecovery>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composure_reduction_zero() {
        let reduction = calculate_composure_reduction(0, 0, 0.0);
        assert!((reduction - 0.0).abs() < 0.001, "0 composure → 0% reduction, got {reduction}");
    }

    #[test]
    fn test_composure_reduction_nullifies_at_equal() {
        // Equal level, equal stats: contest = 0 → nullified
        let reduction = calculate_composure_reduction(100, 100, 0.0);
        assert!((reduction - 0.0).abs() < 0.001, "Equal stats → 0% reduction, got {reduction}");
    }
}
