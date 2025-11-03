use bevy::prelude::*;

/// Plugin that manages server-only behaviour systems
///
/// This plugin provides:
/// - Chase: Unified hostile pursuit and engagement behavior
///
/// Only used by the server.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            crate::server::systems::behaviour::chase::chase,
        );
    }
}
