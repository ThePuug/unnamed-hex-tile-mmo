use bevy::prelude::*;
use crate::common::{
    components::{heal::PendingHeal, resources::Health, ActorAttributes, Loc},
    message::{Do, Event as GameEvent},
};

/// Apply healing events to targets
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
        let Ok(mut health) = health_query.get_mut(pending_heal.target) else {
            commands.entity(heal_entity).despawn();
            continue;
        };

        let reduction = calculate_dominance_reduction(
            pending_heal.target,
            &loc_query,
            &attrs_query,
        );

        let effective_heal = pending_heal.amount * (1.0 - reduction);

        let old_health = health.state;
        health.state = (health.state + effective_heal).min(health.max);
        let actual_heal = health.state - old_health;

        if actual_heal > 0.0 {
            writer.write(Do {
                event: GameEvent::Heal {
                    target: pending_heal.target,
                    amount: actual_heal,
                },
            });

            writer.write(Do {
                event: GameEvent::Incremental {
                    ent: pending_heal.target,
                    component: crate::common::message::Component::Health(*health),
                },
            });
        }

        commands.entity(heal_entity).despawn();
    }
}

/// Calculate Dominance-based healing reduction.
///
/// Pattern 1 (Nullifying): base × gap × contest_factor
/// Base: 25%, Cap: 50%
fn calculate_dominance_reduction(
    target: Entity,
    loc_query: &Query<&Loc>,
    attrs_query: &Query<&ActorAttributes>,
) -> f32 {
    use crate::common::systems::combat::damage::{gap_factor, contest_factor};

    const RADIUS: i32 = 5;
    const BASE_REDUCTION: f32 = 0.25;
    const MAX_REDUCTION: f32 = 0.50;

    let Ok(target_loc) = loc_query.get(target) else {
        return 0.0;
    };
    let Ok(target_attrs) = attrs_query.get(target) else {
        return 0.0;
    };
    let target_toughness = target_attrs.toughness();
    let target_level = target_attrs.total_level();

    // Find highest Dominance within range
    let mut max_dominance = 0u16;
    let mut dominant_level = 0u32;

    for (loc, attrs) in loc_query.iter().zip(attrs_query.iter()) {
        let distance = target_loc.flat_distance(loc) as i32;
        if distance <= RADIUS {
            let dominance = attrs.dominance();
            if dominance > max_dominance {
                max_dominance = dominance;
                dominant_level = attrs.total_level();
            }
        }
    }

    if max_dominance == 0 {
        return 0.0;
    }

    let gap = gap_factor(dominant_level, target_level);
    let contest = contest_factor(max_dominance, target_toughness);
    (BASE_REDUCTION * gap * contest).min(MAX_REDUCTION)
}
