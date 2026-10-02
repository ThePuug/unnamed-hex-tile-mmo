use bevy::prelude::*;
use common_bevy::components::{Actor, reaction_queue::ReactionQueue, resources::{CombatState, Health}};
use crate::components::{DeathMarker, Viewed};
use common_bevy::tuning::Tuning;

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
/// combat, the dead taking no more part. The player's own character, which
/// the server respawns, stands again once its health returns.
pub fn cleanup_dead_entities(
    mut commands: Commands,
    mut query: Query<(Entity, &DeathMarker, &mut Transform, Option<&mut ReactionQueue>, Option<&mut CombatState>, Has<Viewed>, Option<&Health>)>,
    time: Res<Time>,
) {
    const DEATH_LINGER_SECS: f32 = 3.0;

    for (entity, marker, mut transform, queue, combat, viewed, health) in &mut query {
        let elapsed = (time.elapsed() - marker.death_time).as_secs_f32();
        // Respawned, which the server does at full health: the marker goes
        // and the actor's pose follows its heading again
        if elapsed > 0.01 && health.is_some_and(|health| health.max > 0.0 && health.state >= health.max) {
            commands.entity(entity).remove::<DeathMarker>();
            continue;
        }

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
    fn the_player_stands_again_once_respawned_whole() {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs(10));
        world.insert_resource(time);
        let fell = || DeathMarker { death_time: std::time::Duration::from_secs(9) };
        let lying = world.spawn((fell(), Transform::default(), Viewed, Health { max: 100.0, state: 40.0 })).id();
        let risen = world.spawn((fell(), Transform::default(), Viewed, Health { max: 100.0, state: 100.0 })).id();

        world.run_system_once(cleanup_dead_entities).unwrap();

        assert!(world.get::<DeathMarker>(lying).is_some(), "a health not yet whole leaves it down");
        assert!(world.get::<DeathMarker>(risen).is_none(), "respawned whole, it stands");
    }

    #[test]
    fn a_body_leaves_the_fight_as_it_falls() {
        let tuning = Tuning::DEFAULT;
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mut queue = ReactionQueue::default();
        queue.threats.push_back(common_bevy::systems::combat::queue::create_threat(
            &tuning,
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
