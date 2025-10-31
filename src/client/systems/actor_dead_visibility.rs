use bevy::prelude::*;
use crate::common::components::{behaviour::Behaviour, resources::Health};

/// Hide dead players (those with health <= 0)
/// Makes the player model invisible while waiting for respawn
pub fn update_dead_visibility(
    mut query: Query<(&Health, &mut Visibility), With<Behaviour>>,
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
