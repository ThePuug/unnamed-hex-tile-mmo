use bevy::prelude::*;

/// Plugin that manages server-only behaviour systems

/// This plugin provides:
/// - Chase: Unified hostile pursuit and engagement behavior

/// Only used by the server.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::systems::combat::dice::Dice>();
        app.register_required_components::<crate::systems::behaviour::chase::Chase, crate::systems::combat::dice::Rolls>();
        app.add_systems(
            FixedUpdate,
            (
                common_bevy::components::status::tick_status,
                crate::systems::behaviour::hex_assignment::assign_hexes,
                crate::systems::behaviour::chase::chase,
            )
        );
    }
}
