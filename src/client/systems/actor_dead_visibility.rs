use bevy::prelude::*;
use crate::common::components::{Actor, resources::Health};

/// Hide dead actors (those with health <= 0)
/// Makes the actor model invisible while waiting for respawn
/// Works for both players (PvP) and NPCs
pub fn update_dead_visibility(
    mut query: Query<(&Health, &mut Visibility), With<Actor>>,
) {
    for (health, mut visibility) in &mut query {
        if health.state <= 0.0 {
            *visibility = Visibility::Hidden;
        } else if *visibility == Visibility::Hidden {
            // Restore visibility when health > 0 (respawned)
            *visibility = Visibility::Visible;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn test_dead_actor_becomes_hidden() {
        let mut world = World::new();

        // Create dead actor
        let entity = world.spawn((
            Actor,
            Health {
                max: 100.0,
                state: 0.0,
                step: 0.0,
            },
            Visibility::Visible,
        )).id();

        // Run system
        world.run_system_once(update_dead_visibility).unwrap();

        // Verify visibility is hidden
        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Hidden);
    }

    #[test]
    fn test_alive_actor_stays_visible() {
        let mut world = World::new();

        // Create alive actor
        let entity = world.spawn((
            Actor,
            Health {
                max: 100.0,
                state: 50.0,
                step: 50.0,
            },
            Visibility::Visible,
        )).id();

        // Run system
        world.run_system_once(update_dead_visibility).unwrap();

        // Verify visibility is still visible
        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }

    #[test]
    fn test_respawned_actor_becomes_visible() {
        let mut world = World::new();

        // Create actor that was hidden (previously dead)
        let entity = world.spawn((
            Actor,
            Health {
                max: 100.0,
                state: 100.0, // Respawned with full health
                step: 100.0,
            },
            Visibility::Hidden, // Was hidden while dead
        )).id();

        // Run system
        world.run_system_once(update_dead_visibility).unwrap();

        // Verify visibility is restored
        let visibility = world.get::<Visibility>(entity).unwrap();
        assert_eq!(*visibility, Visibility::Visible);
    }

    #[test]
    fn test_works_for_players_and_npcs() {
        let mut world = World::new();

        // Create dead player (no Behaviour component)
        let player = world.spawn((
            Actor,
            Health {
                max: 100.0,
                state: 0.0,
                step: 0.0,
            },
            Visibility::Visible,
        )).id();

        // Create dead NPC (has Behaviour component)
        let npc = world.spawn((
            Actor,
            crate::common::components::behaviour::Behaviour::default(),
            Health {
                max: 100.0,
                state: 0.0,
                step: 0.0,
            },
            Visibility::Visible,
        )).id();

        // Run system
        world.run_system_once(update_dead_visibility).unwrap();

        // Verify both are hidden
        let player_vis = world.get::<Visibility>(player).unwrap();
        let npc_vis = world.get::<Visibility>(npc).unwrap();
        assert_eq!(*player_vis, Visibility::Hidden, "Dead player should be hidden");
        assert_eq!(*npc_vis, Visibility::Hidden, "Dead NPC should be hidden");
    }
}
