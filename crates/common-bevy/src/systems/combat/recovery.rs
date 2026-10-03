use bevy::prelude::*;

use crate::components::recovery::GlobalRecovery;
use crate::components::ActorAttributes;
use crate::tuning::Tuning;

/// Calculate Composure-based recovery time reduction percentage.

/// Pattern 1 (Nullifying): 33% × contest_factor(Composure, target's Impact),
/// with no ceiling.

/// Returns the reduction, 0 up to `Tuning::composure_share`, never reaching it.
/// Caller converts to speed multiplier: 1.0 / (1.0 - reduction).
pub fn calculate_composure_reduction(
    tuning: &Tuning,
    composure: u16,
    target_impact: u16,
    edge: f32,
) -> f32 {
    use crate::systems::combat::damage::contest_factor;

    tuning.composure_share * contest_factor(tuning, composure, target_impact, edge)
}

/// Counts every recovery down and ends it when it runs out, its combo and
/// its burst with it: faster by its actor's Composure, contested by the
/// opponent's Impact with the level gap weighing in. A burst's window counts down in plain seconds beside it.
pub fn global_recovery_system(
    tuning: Res<Tuning>,
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery, &ActorAttributes)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery, attrs) in query.iter_mut() {
        if recovery.is_active() {
            let composure = attrs.composure();
            let reduction_pct = calculate_composure_reduction(
                &tuning,
                composure,
                recovery.target_impact,
                recovery.target_level.map_or(0.0, |target| crate::systems::combat::damage::level_edge(&tuning, attrs.total_level(), target)),
            );

            let speed_multiplier = 1.0 / (1.0 - reduction_pct);

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

    #[test]
    fn test_composure_reduction_zero() {
        let tuning = Tuning::DEFAULT;
        let reduction = calculate_composure_reduction(&tuning, 0, 0, 0.0);
        assert!((reduction - 0.0).abs() < 0.001, "0 composure → 0% reduction, got {reduction}");
    }

    #[test]
    fn test_composure_reduction_nullifies_at_equal() {
        let tuning = Tuning::DEFAULT;
        // Equal level, equal stats: contest = 0 → nullified
        let reduction = calculate_composure_reduction(&tuning, 100, 100, 0.0);
        assert!((reduction - 0.0).abs() < 0.001, "Equal stats → 0% reduction, got {reduction}");
    }
}
