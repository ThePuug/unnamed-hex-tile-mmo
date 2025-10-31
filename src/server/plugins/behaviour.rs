use bevy::prelude::*;

/// Plugin that manages server-only behaviour systems
///
/// This plugin provides:
/// - AI behaviour: find_something_interesting_within, find_or_keep_target (ADR-006)
/// - Random positioning: nearby
/// - Pathfinding: pathto tick and apply
/// - Combat: attack_target, face_target (ADR-006), use_ability_if_adjacent (ADR-006)
///
/// Only used by the server.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                // Legacy nodes (still used by Rabbit)
                crate::server::systems::behaviour::find_something_interesting_within,
                crate::server::systems::behaviour::attack_target,

                // ADR-006 nodes for Wild Dog combat
                crate::server::systems::behaviour::find_target::find_or_keep_target,
                crate::server::systems::behaviour::face_target::face_target,
                crate::server::systems::behaviour::use_ability::use_ability_if_adjacent,

                // Shared nodes
                crate::server::systems::behaviour::nearby,
                crate::server::systems::behaviour::pathto::tick,
                crate::server::systems::behaviour::pathto::apply,
            ),
        );
    }
}
