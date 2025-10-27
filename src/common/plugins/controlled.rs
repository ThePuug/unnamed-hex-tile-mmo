use bevy::prelude::*;

/// Plugin that manages controlled entity systems
///
/// This plugin provides:
/// - Physics application to controlled entities
/// - Input time accumulation (tick)
/// - Remote player interpolation toward authoritative state
///
/// Used by both client and server for entities with Behaviour::Controlled.
pub struct ControlledPlugin;

impl Plugin for ControlledPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                crate::common::systems::behaviour::controlled::apply,
                crate::common::systems::behaviour::controlled::interpolate_remote,
                crate::common::systems::behaviour::controlled::tick,
            ),
        );
    }
}
