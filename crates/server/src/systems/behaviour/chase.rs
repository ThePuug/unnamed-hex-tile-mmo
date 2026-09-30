use bevy::prelude::*;
use rand::seq::IteratorRandom;

use common_bevy::{
    components::{
        Loc, resources::Health,
        behaviour::Side, status::Status, ActorAttributes, target::Target,
        returning::Returning,
        hex_assignment::AssignedHex,
        engagement::EngagementMember,
    },
    message::{Event, Do, Component as MessageComponent},
    plugins::nntree::*,
    resources::map::Map,
};
use crate::components::{
    target_lock::TargetLock,
};
use super::Body;

/// Chase behavior - unified hostile pursuit and engagement

/// Handles the complete chase loop in a single behavior:
/// - Acquires hostile targets within range
/// - Maintains sticky targeting via TargetLock
/// - Continuously paths toward target with greedy movement
/// - Faces and attacks when in range
/// - All without behavior tree composition overhead
///
/// Every NPC chases, melee or ranged: one that fights from range closes
/// only until its target is within its `attack_range`, and stands and
/// shoots from there.
#[derive(Clone, Component, Copy, Debug)]
pub struct Chase {
    pub acquisition_range: u32,  // How far to search for targets
    pub leash_distance: i32,     // Max chase distance (0 = infinite)
    pub attack_range: i32,       // Distance to engage from: the ring its assigned hex is on, or with no hex where it stops closing
}

pub fn chase(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut query: Query<(
        Entity,
        &Chase,
        &Loc,
        Body,
        Option<&ActorAttributes>,
        Option<&TargetLock>,
        Option<&Returning>,
        &EngagementMember,
        Option<&AssignedHex>,  // Path to assigned hex
        &Side,
        Option<&Status>,
    )>,
    q_target: Query<(&Loc, &Health, &Side)>,
    q_spawner: Query<&Loc, Without<Chase>>,  // Query spawner locations
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
) {
    for (npc_entity, &chase_config, npc_loc, mut body, attrs, lock_opt, returning_opt, engagement_member, assigned_hex_opt, own_side, status) in &mut query {

        // Held: it neither walks nor turns
        if Status::holds(status) {
            continue;
        }
        let dt_ms = dt.delta().as_millis() as i16;
        let movement_speed = common_bevy::systems::movement::speed(attrs.map_or(0.005, |a| a.movement_speed()), status);

        // Check if NPC is already in returning state
        if returning_opt.is_some() {
            // Get spawner location to return to
            let Ok(&spawner_loc) = q_spawner.get(engagement_member.0) else {
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
            let Some((start, _)) = map.get_by_qr(npc_loc.q, npc_loc.r) else {
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
                body.step_toward(npc_loc, start, *next_tile, movement_speed, dt_ms, &map, &nntree);
            }

            // Clear target while returning
            commands.entity(npc_entity).insert(Target::default());
            continue;
        }

        // 1. TARGETING: Find or keep target
        let target_entity = if let Some(lock) = lock_opt {
            // Validate existing lock
            if let Ok((target_loc, target_health, _)) = q_target.get(lock.locked_target) {
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
                let Ok(&spawner_loc) = q_spawner.get(engagement_member.0) else {
                    continue;
                };

                let distance_from_spawn = npc_loc.flat_distance(&spawner_loc);
                if distance_from_spawn > chase_config.leash_distance {
                    // Too far from spawn - return to spawn instead of acquiring new target
                    let spawn_qrz = *spawner_loc;
                    let Some((start, _)) = map.get_by_qr(npc_loc.q, npc_loc.r) else {
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
                        body.step_toward(npc_loc, start, *next_tile, movement_speed, dt_ms, &map, &nntree);
                    }

                    commands.entity(npc_entity).insert(Target::default());
                    continue;
                }

                // Close enough to spawn - search for new target
                let valid_targets: Vec<Entity> = super::spotted(&nntree, *npc_loc, chase_config.acquisition_range)
                    .filter_map(|ent| {
                        q_target.get(ent).ok().and_then(|(_, health, side)| {
                            if health.current() > 0.0 && side.is_hostile_to(*own_side) {
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
        let Ok((target_loc, _, _)) = q_target.get(target_entity) else {
            continue;
        };

        // Determine movement destination — assigned hex if available, otherwise player tile
        let move_target = assigned_hex_opt.map(|ah| ah.0).unwrap_or(**target_loc);

        // 3. CHECK RANGE — NPC must be on assigned hex AND within attack range
        // to attack, measured as its swing measures it
        let distance_to_player = npc_loc.distance(target_loc);
        let on_assigned_hex = assigned_hex_opt
            .map(|ah| npc_loc.flat_distance(&Loc::new(ah.0)) == 0)
            .unwrap_or(true); // No assignment = no hex constraint

        if distance_to_player <= chase_config.attack_range && on_assigned_hex {
            // In attack range AND on assigned hex — face target (auto-attack handles damage)
            body.face(npc_loc, **target_loc, dt_ms, &map, &nntree);
            commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            continue;
        }

        // On assigned hex but not in attack range — hold position, face target.
        // Prevents oscillation when assigned hex is farther than attack range.
        if on_assigned_hex && assigned_hex_opt.is_some() {
            body.face(npc_loc, **target_loc, dt_ms, &map, &nntree);
            commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            continue;
        }

        // 4. MOVEMENT: Greedy chase toward assigned hex (or player if no assignment)
        let target_qrz = move_target;

        // Find terrain under current location and target
        let Some((start, _)) = map.get_by_qr(npc_loc.q, npc_loc.r) else {
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
            // A step that gives ground it takes backing away, facing its
            // target, so stepping out to its place never turns its back or
            // costs it a swing
            if next_tile.flat_distance(target_loc) > start.flat_distance(target_loc) {
                body.back_toward(npc_loc, start, *next_tile, **target_loc, movement_speed, dt_ms, &map, &nntree);
            } else {
                body.step_toward(npc_loc, start, *next_tile, movement_speed, dt_ms, &map, &nntree);
            }

            // Update Target component for reactive systems
            commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
        }

        // Behavior never "completes" during chase - always running
    }
}
