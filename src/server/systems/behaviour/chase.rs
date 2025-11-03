use bevy::{prelude::*, ecs::hierarchy::ChildOf};
use rand::seq::IteratorRandom;

use crate::{
    common::{
        components::{
            Loc, heading::Heading, offset::Offset, resources::Health,
            behaviour::PlayerControlled, AirTime, ActorAttributes, target::Target,
        },
        plugins::nntree::*,
        resources::map::Map,
        systems::physics,
    },
    server::components::{
        returning::Returning,
        target_lock::TargetLock,
    },
};

/// Chase behavior - unified hostile pursuit and engagement
///
/// Handles the complete chase loop in a single behavior:
/// - Acquires hostile targets within range
/// - Maintains sticky targeting via TargetLock
/// - Continuously paths toward target with greedy movement
/// - Faces and attacks when in range
/// - All without behavior tree composition overhead
#[derive(Clone, Component, Copy, Debug)]
pub struct Chase {
    pub acquisition_range: u32,  // How far to search for targets
    pub leash_distance: i16,     // Max chase distance (0 = infinite)
    pub attack_range: i16,       // Distance to engage (typically 1 for melee)
}

pub fn chase(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &Chase,
        &Loc,
        &mut Heading,
        &mut Offset,
        &mut AirTime,
        Option<&ActorAttributes>,
        Option<&TargetLock>,
        Option<&Returning>,
        &ChildOf,
    )>,
    q_target: Query<(&Loc, &Health), With<PlayerControlled>>,
    q_spawner: Query<&Loc, Without<Chase>>,  // Query spawner locations
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
) {
    for (npc_entity, &chase_config, npc_loc, mut npc_heading, mut npc_offset, mut npc_airtime, attrs, lock_opt, returning_opt, child_of) in &mut query {

        // Check if NPC is already in returning state
        if returning_opt.is_some() {
            // Get spawner location to return to
            let Ok(&spawner_loc) = q_spawner.get(child_of.parent()) else {
                continue;
            };

            // Check if we're back at spawn
            let distance_to_spawn = npc_loc.flat_distance(&spawner_loc);
            if distance_to_spawn <= 2 {
                // Close enough to spawn - clear returning state, lock, and target
                commands.entity(npc_entity).remove::<Returning>();
                commands.entity(npc_entity).remove::<TargetLock>();
                commands.entity(npc_entity).insert(Target(None));
                continue;
            }

            // Path back to spawn using greedy movement
            let spawn_qrz = *spawner_loc;
            let Some((start, _)) = map.find(**npc_loc, -60) else {
                continue;
            };

            let neighbors = map.neighbors(start);
            let best_neighbor = neighbors
                .iter()
                .filter(|(neighbor, _)| {
                    nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
                })
                .min_by_key(|(neighbor, _)| neighbor.distance(&spawn_qrz));

            if let Some((next_tile, _)) = best_neighbor {
                let direction = (*next_tile - start).normalize();
                let desired_heading = Heading::new(direction);
                *npc_heading = desired_heading;

                if npc_loc.z <= next_tile.z && npc_airtime.state.is_none() {
                    npc_airtime.state = Some(125);
                }

                let dt_ms = dt.delta().as_millis() as i16;
                let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);

                let (offset, airtime) = physics::apply(
                    Loc::new(*next_tile),
                    dt_ms,
                    *npc_loc,
                    npc_offset.state,
                    npc_airtime.state,
                    movement_speed,
                    *npc_heading,
                    &map,
                    &nntree,
                );

                npc_offset.state = offset;
                npc_airtime.state = airtime;
            }

            // Clear target while returning
            commands.entity(npc_entity).insert(Target(None));
            continue;
        }

        // 1. TARGETING: Find or keep target
        let target_entity = if let Some(lock) = lock_opt {
            // Validate existing lock
            if let Ok((target_loc, target_health)) = q_target.get(lock.locked_target) {
                if target_health.current() > 0.0 {
                    if lock.is_target_valid(Some(target_loc), npc_loc) {
                        // Keep existing target
                        Some(lock.locked_target)
                    } else {
                        // Leash broken - NPC went too far from origin
                        // Add Returning component to initiate return to spawn
                        // Keep TargetLock to prevent re-acquisition during return
                        commands.entity(npc_entity).insert(Returning);
                        None
                    }
                } else {
                    // Target died - remove lock and search
                    commands.entity(npc_entity).remove::<TargetLock>();
                    None
                }
            } else {
                // Target despawned - remove lock and search
                commands.entity(npc_entity).remove::<TargetLock>();
                None
            }
        } else {
            None
        };

        let target_entity = match target_entity {
            Some(ent) => ent,
            None => {
                // Check if we're too far from spawner to acquire new targets
                let Ok(&spawner_loc) = q_spawner.get(child_of.parent()) else {
                    continue;
                };

                let distance_from_spawn = npc_loc.flat_distance(&spawner_loc);
                if distance_from_spawn > chase_config.leash_distance {
                    // Too far from spawn - return to spawn instead of acquiring new target
                    let spawn_qrz = *spawner_loc;
                    let Some((start, _)) = map.find(**npc_loc, -60) else {
                        continue;
                    };

                    let neighbors = map.neighbors(start);
                    let best_neighbor = neighbors
                        .iter()
                        .filter(|(neighbor, _)| {
                            nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
                        })
                        .min_by_key(|(neighbor, _)| neighbor.distance(&spawn_qrz));

                    if let Some((next_tile, _)) = best_neighbor {
                        let direction = (*next_tile - start).normalize();
                        let desired_heading = Heading::new(direction);
                        *npc_heading = desired_heading;

                        if npc_loc.z <= next_tile.z && npc_airtime.state.is_none() {
                            npc_airtime.state = Some(125);
                        }

                        let dt_ms = dt.delta().as_millis() as i16;
                        let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);

                        let (offset, airtime) = physics::apply(
                            Loc::new(*next_tile),
                            dt_ms,
                            *npc_loc,
                            npc_offset.state,
                            npc_airtime.state,
                            movement_speed,
                            *npc_heading,
                            &map,
                            &nntree,
                        );

                        npc_offset.state = offset;
                        npc_airtime.state = airtime;
                    }

                    commands.entity(npc_entity).insert(Target(None));
                    continue;
                }

                // Close enough to spawn - search for new target
                let nearby = nntree.locate_within_distance(
                    *npc_loc,
                    chase_config.acquisition_range as i32 * chase_config.acquisition_range as i32,
                );

                let valid_targets: Vec<Entity> = nearby
                    .filter_map(|result| {
                        let ent = result.ent;
                        q_target.get(ent).ok().and_then(|(_, health)| {
                            if health.current() > 0.0 && ent != npc_entity {
                                Some(ent)
                            } else {
                                None
                            }
                        })
                    })
                    .collect();

                if let Some(&new_target) = valid_targets.iter().choose(&mut rand::rng()) {
                    // Lock new target - use spawner location as leash origin
                    commands.entity(npc_entity).insert(TargetLock::new(
                        new_target,
                        chase_config.leash_distance,
                        spawner_loc,  // Spawner location is the leash anchor point
                    ));
                    commands.entity(npc_entity).insert(Target(Some(new_target)));
                    new_target
                } else {
                    // No targets found - stop chasing
                    continue;
                }
            }
        };

        // 2. GET TARGET LOCATION
        let Ok((target_loc, _)) = q_target.get(target_entity) else {
            continue;
        };

        // 3. CHECK RANGE
        let distance = npc_loc.flat_distance(target_loc);

        if distance <= chase_config.attack_range {
            // In attack range - face target (auto-attack handles damage)
            let direction = (**target_loc - **npc_loc).normalize();
            let desired_heading = Heading::new(direction);
            *npc_heading = desired_heading;
            commands.entity(npc_entity).insert(Target(Some(target_entity)));
            continue;
        }

        // 4. MOVEMENT: Greedy chase toward target
        let target_qrz = **target_loc;

        // Find terrain under current location and target
        let Some((start, _)) = map.find(**npc_loc, -60) else {
            continue;
        };

        // Greedy: pick neighbor closest to target
        let neighbors = map.neighbors(start);
        let best_neighbor = neighbors
            .iter()
            .filter(|(neighbor, _)| {
                nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
            })
            .min_by_key(|(neighbor, _)| neighbor.distance(&target_qrz));

        if let Some((next_tile, _)) = best_neighbor {
            // Move toward target
            let direction = (*next_tile - start).normalize();
            let desired_heading = Heading::new(direction);

            if *npc_heading != desired_heading {
                *npc_heading = desired_heading;
            }

            // Trigger jump if moving upward
            if npc_loc.z <= next_tile.z && npc_airtime.state.is_none() {
                npc_airtime.state = Some(125);
            }

            // Apply physics
            let dt_ms = dt.delta().as_millis() as i16;
            let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);

            let (offset, airtime) = physics::apply(
                Loc::new(*next_tile),
                dt_ms,
                *npc_loc,
                npc_offset.state,
                npc_airtime.state,
                movement_speed,
                *npc_heading,
                &map,
                &nntree,
            );

            npc_offset.state = offset;
            npc_airtime.state = airtime;

            // Update Target component for reactive systems
            commands.entity(npc_entity).insert(Target(Some(target_entity)));
        }

        // Behavior never "completes" during chase - always running
    }
}
