use bevy::prelude::*;
use bevy_behave::prelude::*;

use crate::common::components::{Loc, heading::Heading};
use crate::server::systems::behaviour::Target;

/// FaceTarget behavior node - updates NPC heading to face their Target entity
///
/// From ADR-006:
/// - Runs TWICE in Wild Dog sequence (before and after pathfinding)
/// - Before: Sets initial heading toward target
/// - After: Corrects heading after pathfinding may have changed it
/// - Required for directional targeting (60° facing cone from ADR-004)
#[derive(Clone, Component, Copy, Default)]
pub struct FaceTarget;

pub fn face_target(
    mut commands: Commands,
    mut query: Query<(&FaceTarget, &BehaveCtx)>,
    mut q_npc: Query<(&Loc, Option<&Target>, &mut Heading)>,
    q_target_loc: Query<&Loc>,
) {
    for (_, &ctx) in &mut query {
        let npc_entity = ctx.target_entity();

        // Get NPC's location, Target, and current heading
        let Ok((npc_loc, target_opt, mut npc_heading)) = q_npc.get_mut(npc_entity) else {
            commands.trigger(ctx.failure());
            continue;
        };

        // Check if NPC has a Target
        let Some(target) = target_opt else {
            // No Target set, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Get Target's location
        let Ok(target_loc) = q_target_loc.get(**target) else {
            // Target entity missing Loc, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Calculate direction vector from NPC to Target
        let direction_qrz = **target_loc - **npc_loc;

        // Convert direction to Heading (wraps the direction vector)
        let new_heading = Heading::new(direction_qrz);

        // Update NPC's heading
        *npc_heading = new_heading;

        // Success (heading updated)
        commands.trigger(ctx.success());
    }
}
