use bevy::prelude::*;
use crate::common::components::{Actor, resources::Health};
use crate::client::components::DeathMarker;

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

/// Apply death pose to newly dead entities and despawn after 3 seconds
pub fn cleanup_dead_entities(
    mut commands: Commands,
    mut query: Query<(Entity, &DeathMarker, &mut Transform)>,
    time: Res<Time>,
) {
    const DEATH_LINGER_SECS: f32 = 3.0;

    for (entity, marker, mut transform) in &mut query {
        let elapsed = (time.elapsed() - marker.death_time).as_secs_f32();

        if elapsed <= 0.01 {
            // First frame: tip over 90 degrees to lay on side
            transform.rotation *= Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        }

        if elapsed >= DEATH_LINGER_SECS {
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
            Health { max: 100.0, state: 0.0, step: 0.0 },
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
            Health { max: 100.0, state: 50.0, step: 50.0 },
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
            Health { max: 100.0, state: 100.0, step: 100.0 },
            Visibility::Hidden,
        )).id();

        world.run_system_once(update_dead_visibility).unwrap();

        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }
}
