use bevy::prelude::*;
use common_bevy::components::{Actor, reaction_queue::ReactionQueue, resources::{CombatState, Health}};
use crate::components::{DeathMarker, Viewed};

/// Restore visibility for actors that were hidden (e.g. after respawn)
/// Dead actors now get a death pose via DeathMarker instead of being hidden
pub fn update_dead_visibility(
    mut query: Query<(&Health, &mut Visibility), With<Actor>>,
) {
    for (health, mut visibility) in &mut query {
        if health.state > 0.0 && *visibility == Visibility::Hidden {
            *visibility = Visibility::Visible;
        }
    }
}

/// Apply death pose to newly dead entities and despawn after 3 seconds, but
/// the one the client sees as: its body stays until the view ends. As it
/// falls it leaves the fight: the threats it held go and it is out of
/// combat, the dead taking no more part.
pub fn cleanup_dead_entities(
    mut commands: Commands,
    mut query: Query<(Entity, &DeathMarker, &mut Transform, Option<&mut ReactionQueue>, Option<&mut CombatState>, Has<Viewed>)>,
    time: Res<Time>,
) {
    const DEATH_LINGER_SECS: f32 = 3.0;

    for (entity, marker, mut transform, queue, combat, viewed) in &mut query {
        let elapsed = (time.elapsed() - marker.death_time).as_secs_f32();

        if elapsed <= 0.01 {
            // First frame: tip over 90 degrees to lay on side
            transform.rotation *= Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
            if let Some(mut queue) = queue {
                queue.threats.clear();
            }
            if let Some(mut combat) = combat {
                combat.in_combat = false;
            }
        }

        if elapsed >= DEATH_LINGER_SECS && !viewed {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn test_dead_actor_stays_visible() {
        let mut world = World::new();

        // Dead actors stay visible (death pose handled by DeathMarker/cleanup_dead_entities)
        let entity = world.spawn((
            Actor,
            Health { max: 100.0, state: 0.0 },
            Visibility::Visible,
        )).id();

        world.run_system_once(update_dead_visibility).unwrap();

        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }

    #[test]
    fn test_alive_actor_stays_visible() {
        let mut world = World::new();

        let entity = world.spawn((
            Actor,
            Health { max: 100.0, state: 50.0 },
            Visibility::Visible,
        )).id();

        world.run_system_once(update_dead_visibility).unwrap();

        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }

    #[test]
    fn test_respawned_actor_becomes_visible() {
        let mut world = World::new();

        // Actor that was hidden for some reason gets restored when health > 0
        let entity = world.spawn((
            Actor,
            Health { max: 100.0, state: 100.0 },
            Visibility::Hidden,
        )).id();

        world.run_system_once(update_dead_visibility).unwrap();

        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }

    #[test]
    fn a_viewed_body_stays_until_the_view_ends() {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs(10));
        world.insert_resource(time);
        let fell = || DeathMarker { death_time: std::time::Duration::from_secs(1) };
        let viewed = world.spawn((fell(), Transform::default(), Viewed)).id();
        let other = world.spawn((fell(), Transform::default())).id();

        world.run_system_once(cleanup_dead_entities).unwrap();

        assert!(world.get_entity(viewed).is_ok(), "the viewed body is kept");
        assert!(world.get_entity(other).is_err(), "the rest linger and go");
    }

    #[test]
    fn a_body_leaves_the_fight_as_it_falls() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mut queue = ReactionQueue::default();
        queue.threats.push_back(common_bevy::systems::combat::queue::create_threat(
            Entity::PLACEHOLDER, &Default::default(), &Default::default(), 10.0,
            None, std::time::Duration::ZERO, 0.0, 0.0,
        ));
        let body = world.spawn((
            DeathMarker { death_time: std::time::Duration::ZERO },
            Transform::default(),
            queue,
            CombatState { in_combat: true, last_action: std::time::Duration::ZERO },
            Viewed,
        )).id();

        world.run_system_once(cleanup_dead_entities).unwrap();

        assert!(!world.get::<CombatState>(body).unwrap().in_combat, "out of combat");
        assert!(world.get::<ReactionQueue>(body).unwrap().threats.is_empty(), "holding no threats");
    }
}
