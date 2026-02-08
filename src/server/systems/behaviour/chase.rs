use bevy::{prelude::*, ecs::hierarchy::ChildOf};
use rand::seq::IteratorRandom;
use qrz::{Convert, Qrz};

use crate::{
    common::{
        components::{
            Loc, heading::Heading, position::Position, resources::Health,
            behaviour::PlayerControlled, AirTime, ActorAttributes, target::Target,
            returning::Returning,
        },
        message::{Event, Do, Component as MessageComponent},
        plugins::nntree::*,
        resources::map::Map,
        systems::physics,
    },
    server::components::{
        target_lock::TargetLock,
    },
};

/// Helper: Broadcast movement intent when NPC decides to move (ADR-011)
fn broadcast_intent(
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
    map: &Map,
    npc_entity: Entity,
    npc_loc: &Loc,
    next_tile: Qrz,
    heading: &Heading,
    movement_speed: f32,
    intent_state_opt: Option<&mut crate::common::components::movement_intent_state::MovementIntentState>,
) {
    // Get or initialize MovementIntentState
    let mut intent_state = if let Some(state) = intent_state_opt {
        state
    } else {
        // First time - add component and skip (will process next frame)
        commands.entity(npc_entity).insert(crate::common::components::movement_intent_state::MovementIntentState::default());
        return;
    };

    // Skip if already broadcast for this destination and heading
    if next_tile == intent_state.last_broadcast_dest && *heading == intent_state.last_broadcast_heading {
        return;
    }

    // Calculate distance and duration (to heading-adjusted destination position)
    let current_world = map.convert(**npc_loc) + Vec3::ZERO; // At tile center when deciding
    let dest_tile_center = map.convert(next_tile);

    // Calculate heading-adjusted offset at destination (movement direction)
    let movement_direction = next_tile - **npc_loc;
    let dest_offset = if movement_direction != qrz::Qrz::default() {
        use crate::common::components::heading::HERE;
        let heading_neighbor = map.convert(next_tile + movement_direction);
        let direction = heading_neighbor - dest_tile_center;
        (direction * HERE).xz()
    } else {
        Vec2::ZERO
    };
    let dest_world = dest_tile_center + Vec3::new(dest_offset.x, 0.0, dest_offset.y);

    let distance = (dest_world - current_world).length();
    let duration_ms = (distance / movement_speed) as u16;

    // Update state and broadcast
    intent_state.last_broadcast_dest = next_tile;
    intent_state.last_broadcast_heading = *heading;

    writer.write(Do {
        event: Event::MovementIntent {
            ent: npc_entity,
            destination: next_tile + qrz::Qrz::Z, // Entity stands ON terrain (one tile above)
            duration_ms,
        }
    });
}

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
    mut writer: MessageWriter<Do>,  // ADR-011: Broadcast movement intents
    mut query: Query<(
        Entity,
        &Chase,
        &Loc,
        &mut Heading,
        &mut Position,
        &mut AirTime,
        Option<&ActorAttributes>,
        Option<&TargetLock>,
        Option<&Returning>,
        Option<&mut crate::common::components::movement_intent_state::MovementIntentState>,  // ADR-011
        &ChildOf,
    )>,
    q_target: Query<(&Loc, &Health), With<PlayerControlled>>,
    q_spawner: Query<&Loc, Without<Chase>>,  // Query spawner locations
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
) {
    for (npc_entity, &chase_config, npc_loc, mut npc_heading, mut npc_position, mut npc_airtime, attrs, lock_opt, returning_opt, mut intent_state_opt, child_of) in &mut query {

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
                commands.entity(npc_entity).insert(Target::default());
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

                // ADR-011: Broadcast intent BEFORE physics computes movement
                broadcast_intent(&mut commands, &mut writer, &map, npc_entity, npc_loc, *next_tile, &npc_heading, movement_speed, intent_state_opt.as_deref_mut());

                let (offset, airtime) = physics::apply(
                    Loc::new(*next_tile),
                    dt_ms,
                    *npc_loc,
                    npc_position.offset,
                    npc_airtime.state,
                    movement_speed,
                    *npc_heading,
                    &map,
                    &nntree,
                );

                npc_position.offset = offset;
                npc_airtime.state = airtime;
            }

            // Clear target while returning
            commands.entity(npc_entity).insert(Target::default());
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
                        // Broadcast Returning to clients for leash health regen prediction
                        writer.write(Do {
                            event: Event::Incremental {
                                ent: npc_entity,
                                component: MessageComponent::Returning(Returning),
                            },
                        });
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

                        // ADR-011: Broadcast intent BEFORE physics computes movement
                        broadcast_intent(&mut commands, &mut writer, &map, npc_entity, npc_loc, *next_tile, &npc_heading, movement_speed, intent_state_opt.as_deref_mut());

                        let (offset, airtime) = physics::apply(
                            Loc::new(*next_tile),
                            dt_ms,
                            *npc_loc,
                            npc_position.offset,
                            npc_airtime.state,
                            movement_speed,
                            *npc_heading,
                            &map,
                            &nntree,
                        );

                        npc_position.offset = offset;
                        npc_airtime.state = airtime;
                    }

                    commands.entity(npc_entity).insert(Target::default());
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
                    commands.entity(npc_entity).insert(Target { entity: Some(new_target), last_target: Some(new_target) });
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
            commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
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

            // ADR-011: Broadcast intent BEFORE physics computes movement
            broadcast_intent(&mut commands, &mut writer, &map, npc_entity, npc_loc, *next_tile, &npc_heading, movement_speed, intent_state_opt.as_deref_mut());

            let (offset, airtime) = physics::apply(
                Loc::new(*next_tile),
                dt_ms,
                *npc_loc,
                npc_position.offset,
                npc_airtime.state,
                movement_speed,
                *npc_heading,
                &map,
                &nntree,
            );

            npc_position.offset = offset;
            npc_airtime.state = airtime;

            // Update Target component for reactive systems
            commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
        }

        // Behavior never "completes" during chase - always running
    }
}
