use bevy::prelude::*;
use common_bevy::components::{
    Actor, Loc, position::{Position, VisualPosition}, reaction_queue::ReactionQueue,
    resources::{CombatState, Health}, status::Status,
};
use common_bevy::message::{Do, Event};
use crate::components::{DeathMarker, Viewed};

/// Shows an actor hidden by its respawn ([`respawn`]) once it is alive
pub fn update_dead_visibility(
    mut query: Query<(&Health, &mut Visibility), (With<Actor>, Without<DeathMarker>)>,
) {
    for (health, mut visibility) in &mut query {
        if health.state > 0.0 && *visibility == Visibility::Hidden {
            *visibility = Visibility::Visible;
        }
    }
}

/// Stands a body the client kept, the local player's, again where the
/// server respawned it: hidden, it moves there and stands, its status
/// gone with its death, and [`update_dead_visibility`] shows it once its
/// health says it is alive. It never stands where it fell.
pub fn respawn(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut bodies: Query<&mut Transform, With<DeathMarker>>,
    map: Res<common_bevy::resources::map::Map>,
    origin: Res<crate::resources::RenderOrigin>,
) {
    for message in reader.read() {
        let Do { event: Event::Respawn { ent, qrz } } = message else { continue };
        let Ok(mut transform) = bodies.get_mut(*ent) else { continue };
        let at = origin.render_tile(&map, *qrz);
        transform.translation = at;
        transform.rotation = Quat::IDENTITY;
        commands.entity(*ent)
            .remove::<(DeathMarker, Status)>()
            .insert((Visibility::Hidden, Loc::new(*qrz), Position::at_tile(*qrz), VisualPosition::at(at)));
    }
}

/// Apply death pose to newly dead entities and despawn after 3 seconds, but
/// the one the client sees as: its body stays until the view ends. As it
/// falls it leaves the fight: the threats it held go and it is out of
/// combat, the dead taking no more part. The player's own character lies
/// until the server respawns it ([`respawn`]).
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
    use common_bevy::tuning::Tuning;

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
    fn the_player_stands_again_only_where_it_respawns_and_shows_once_alive() {
        use qrz::Qrz;
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.insert_resource(crate::resources::world_map());
        app.init_resource::<crate::resources::RenderOrigin>();
        app.add_systems(Update, (cleanup_dead_entities, respawn, update_dead_visibility));
        let fell = DeathMarker { death_time: std::time::Duration::ZERO };
        let player = app.world_mut().spawn((
            Actor, fell, Transform::default(), Visibility::Visible, Viewed,
            Health { max: 100.0, state: 100.0 }, Status::default(), Loc::new(Qrz { q: 9, r: 9, z: 0 }),
        )).id();
        app.update();
        assert!(app.world().get::<DeathMarker>(player).is_some(), "whole health alone leaves it lying where it fell");

        let spawn_point = Qrz { q: 0, r: 0, z: 0 };
        app.world_mut().get_mut::<Health>(player).unwrap().state = 0.0;
        app.world_mut().write_message(Do { event: Event::Respawn { ent: player, qrz: spawn_point } });
        app.update();
        assert!(app.world().get::<DeathMarker>(player).is_none(), "respawned, it stands");
        assert_eq!(**app.world().get::<Loc>(player).unwrap(), spawn_point, "at the spawn point");
        assert!(app.world().get::<Status>(player).is_none(), "its status gone with its death");
        assert_eq!(*app.world().get::<Visibility>(player).unwrap(), Visibility::Hidden, "unseen until it is alive");

        app.world_mut().get_mut::<Health>(player).unwrap().state = 100.0;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(player).unwrap(), Visibility::Visible);
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
