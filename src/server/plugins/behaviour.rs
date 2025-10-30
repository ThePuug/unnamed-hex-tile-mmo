use bevy::prelude::*;

/// Plugin that manages server-only behaviour systems
///
/// This plugin provides:
/// - AI behaviour: find_something_interesting_within
/// - Random positioning: nearby
/// - Pathfinding: pathto tick and apply
/// - Combat: attack_target (directional targeting for NPCs)
///
/// Only used by the server.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                crate::server::systems::behaviour::find_something_interesting_within,
                crate::server::systems::behaviour::nearby,
                crate::server::systems::behaviour::pathto::tick,
                crate::server::systems::behaviour::pathto::apply,
                crate::server::systems::behaviour::attack_target,
            ),
        );
    }
}
