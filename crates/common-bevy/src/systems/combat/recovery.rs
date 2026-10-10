use bevy::prelude::*;

use crate::components::recovery::GlobalRecovery;
use crate::components::ActorAttributes;
use crate::tuning::Tuning;

/// Calculate Fitness-based recovery time reduction percentage.

/// `Tuning::fitness_share` × contest_factor(Fitness, target's Focus),
/// with no ceiling.

/// Returns the reduction, 0 up to `Tuning::fitness_share`, never reaching it.
/// Caller converts to speed multiplier: 1.0 / (1.0 - reduction).
pub fn calculate_fitness_reduction(
    tuning: &Tuning,
    fitness: u16,
    target_focus: u16,
    edge: f32,
) -> f32 {
    use crate::systems::combat::damage::contest_factor;

    tuning.fitness_share * contest_factor(tuning, fitness, target_focus, edge)
}

/// Counts every recovery down and ends it when it runs out, its combo and
/// its burst with it: faster by its actor's Fitness, contested by the
/// opponent's Focus with the level gap weighing in.
pub fn global_recovery_system(
    tuning: Res<Tuning>,
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery, &ActorAttributes)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery, attrs) in query.iter_mut() {
        if recovery.is_active() {
            let reduction_pct = calculate_fitness_reduction(
                &tuning,
                attrs.fitness(),
                recovery.target_focus,
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
