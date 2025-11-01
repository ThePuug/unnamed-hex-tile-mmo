use bevy::prelude::*;
use bevy_behave::prelude::*;
use rand::seq::IteratorRandom;

use crate::{
    common::{
        components::{Loc, resources::Health, behaviour::PlayerControlled},
        plugins::nntree::*,
    },
    server::{
        components::target_lock::TargetLock,
        systems::behaviour::Target,
    },
};

/// FindOrKeepTarget behavior node - implements sticky target acquisition
///
/// This is a CRITICAL component for MVP (ADR-006):
/// - Checks existing TargetLock first before searching
/// - Only finds new target if lock is invalid (dead, despawned, beyond leash)
/// - Creates TargetLock on new acquisition
/// - Prevents mid-chase target switching that breaks combat pressure
#[derive(Clone, Component, Copy, Debug)]
pub struct FindOrKeepTarget {
    pub dist: u32,              // Acquisition range
    pub leash_distance: i16,    // Max chase distance (0 = infinite)
}

pub fn find_or_keep_target(
    mut commands: Commands,
    nntree: Res<NNTree>,
    mut query: Query<(&FindOrKeepTarget, &BehaveCtx)>,
    q_npc: Query<(&Loc, Option<&TargetLock>)>,
    q_target: Query<(&Loc, &Health, Option<&PlayerControlled>)>,
) {
    for (&node, &ctx) in &mut query {
        let npc_entity = ctx.target_entity();

        // Get NPC's location and optional existing lock
        let Ok((npc_loc, lock_opt)) = q_npc.get(npc_entity) else {
            commands.trigger(ctx.failure());
            continue;
        };

        // 1. Check if we have a locked target that's still valid
        if let Some(lock) = lock_opt {
            // Try to get locked target's location, health, and PlayerControlled status
            if let Ok((target_loc, target_health, player_controlled)) = q_target.get(lock.locked_target) {
                // Validate: in chase range AND alive AND is a player (asymmetric targeting)
                if lock.is_target_valid(Some(target_loc), npc_loc)
                    && target_health.current() > 0.0
                    && player_controlled.is_some()  // NPCs only target players
                {
                    // Keep existing target (lock still valid)
                    commands.entity(npc_entity).insert(Target(lock.locked_target));
                    commands.trigger(ctx.success());
                    continue;
                }
            }

            // Target invalid: Remove lock and fall through to find new
            commands.entity(npc_entity).remove::<TargetLock>();
        }

        // 2. Find new target (no lock or lock was invalid)
        let nearby = nntree.locate_within_distance(*npc_loc, node.dist as i16 * node.dist as i16);

        // Filter to entities with Health > 0 AND PlayerControlled (asymmetric targeting)
        let valid_targets: Vec<Entity> = nearby
            .filter_map(|result| {
                let ent = result.ent;
                // Check if entity has Health, is alive, and is a player (asymmetric targeting)
                q_target.get(ent).ok().and_then(|(_, health, player_controlled)| {
                    if health.current() > 0.0
                        && ent != npc_entity  // Don't target self
                        && player_controlled.is_some()  // NPCs only target players
                    {
                        Some(ent)
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Pick a random target from valid targets
        if let Some(&new_target) = valid_targets.iter().choose(&mut rand::rng()) {
            // Lock to new target (persists until invalid)
            commands.entity(npc_entity).insert(TargetLock::new(
                new_target,
                node.leash_distance,
            ));
            commands.entity(npc_entity).insert(Target(new_target));
            commands.trigger(ctx.success());
        } else {
            // No valid targets: fail
            commands.trigger(ctx.failure());
        }
    }
}
