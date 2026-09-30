use bevy::prelude::*;

/// Plugin that manages server-only behaviour systems

/// This plugin provides:
/// - Chase: Unified hostile pursuit and engagement behavior

/// Only used by the server.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                crate::systems::stagger::tick_stagger,
                common_bevy::components::stunned::tick_stunned,
                common_bevy::components::status::tick_status,
                crate::systems::behaviour::hex_assignment::assign_hexes,
                crate::systems::behaviour::chase::chase,
                crate::systems::stagger::enforce_stagger
                    .after(crate::systems::behaviour::chase::chase),
            )
        );
    }
}
