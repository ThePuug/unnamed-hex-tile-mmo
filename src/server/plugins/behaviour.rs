use bevy::prelude::*;

/// Plugin that manages server-only behaviour systems
///
/// This plugin provides:
/// - Chase: Unified hostile pursuit and engagement behavior
/// - Kite: Flee behavior when player gets too close
///
/// Only used by the server.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                crate::server::systems::behaviour::hex_assignment::assign_hexes,
                crate::server::systems::behaviour::chase::chase,
                crate::server::systems::behaviour::kite::kite,
            )
        );
    }
}
