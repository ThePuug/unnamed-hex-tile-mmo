use bevy::prelude::*;
use rand::seq::IteratorRandom;
use qrz::{Convert, Qrz};

use common::{
    components::{
        Loc, heading::Heading, position::Position, resources::Health,
        behaviour::PlayerControlled, AirTime, ActorAttributes, target::Target,
        returning::Returning,
        engagement::EngagementMember,
    },
    message::{Event, Do, Component as MessageComponent},
    plugins::nntree::*,
    resources::map::Map,
    systems::physics,
};
use crate::components::{
    target_lock::TargetLock,
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
    intent_state_opt: Option<&mut common::components::movement_intent_state::MovementIntentState>,
) {
    // Get or initialize MovementIntentState
    let intent_state = if let Some(state) = intent_state_opt {
        state
    } else {
        // First time - add component and skip (will process next frame)
        commands.entity(npc_entity).insert(common::components::movement_intent_state::MovementIntentState::default());
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
        use common::components::heading::HERE;
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

/// Score a neighbor tile for kite movement.
/// Higher score = more preferred destination.
/// Balances staying at optimal distance from player with staying near spawn.
fn score_neighbor(
    neighbor: &Qrz,
    player: &Qrz,
    spawn: &Qrz,
    optimal_mid: i16,
    leash_distance: i16,
) -> i32 {
    let dist_to_player = neighbor.flat_distance(player) as i32;
    let dist_to_spawn = neighbor.flat_distance(spawn) as i32;
    let leash = leash_distance as i32;

    // Prefer being at optimal distance from player (weight: 3)
    let range_score = -(dist_to_player - optimal_mid as i32).abs() * 3;

    // Prefer being closer to spawn, with increasing urgency near leash boundary
    // Weight ramps from 1 (at spawn) to 3 (at leash distance)
    let leash_weight = 1 + dist_to_spawn * 2 / leash;
    let leash_score = -dist_to_spawn * leash_weight;

    range_score + leash_score
}

/// Kite behavior - ranged hostile that maintains optimal distance (ADR-010 Phase 4)
///
/// Implements distance-based state machine for ranged kiting enemies:
/// - Acquires hostile targets within aggro range
/// - Maintains sticky targeting via TargetLock
/// - **Flees** when target closes within disengage_distance (< 3 hexes)
/// - **Repositions** when target is 3-5 hexes away (moves to optimal zone 6-7 hexes)
/// - **Attacks** when target is in optimal_distance range (5-8 hexes) - instant hit every attack_interval seconds
/// - **Advances** when target is beyond optimal range (> 8 hexes) - moves closer
/// - **Leashes** when too far from spawner (returns to spawn)
///
/// # Design Pattern
/// Inverse pathfinding: Kiter moves AWAY from player to maintain distance.
/// Attack timer independent of movement: Continues firing while repositioning.
#[derive(Clone, Component, Copy, Debug)]
pub struct Kite {
    pub acquisition_range: u32,      // How far to search for targets (e.g., 15 hexes)
    pub leash_distance: i16,         // Max chase distance from spawn (e.g., 30 hexes)
    pub optimal_distance_min: i16,   // Min optimal attack range (e.g., 5 hexes)
    pub optimal_distance_max: i16,   // Max optimal attack range (e.g., 8 hexes)
    pub disengage_distance: i16,     // Flee threshold when target too close (e.g., 3 hexes)
}

impl Kite {
    /// Create a new Kite behavior with Forest Sprite stats (ADR-010 Phase 4)
    pub fn forest_sprite() -> Self {
        Self {
            acquisition_range: 15,        // 15 hexes aggro range
            leash_distance: 30,           // 30 hexes leash
            optimal_distance_min: 5,      // 5-8 hex optimal zone
            optimal_distance_max: 8,
            disengage_distance: 3,        // Flee if < 3 hexes
        }
    }

    /// Determine what action the kiter should take based on distance to target
    pub fn determine_action(&self, distance_to_target: i16) -> KiteAction {
        if distance_to_target < self.disengage_distance {
            KiteAction::Flee
        } else if distance_to_target < self.optimal_distance_min {
            KiteAction::Reposition
        } else if distance_to_target <= self.optimal_distance_max {
            KiteAction::Attack
        } else {
            KiteAction::Advance
        }
    }
}

/// State machine for kiting behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KiteAction {
    Flee,        // Move away from target (distance < 3 hexes)
    Reposition,  // Move to optimal zone (distance 3-5 hexes, move to 6-7 hexes)
    Attack,      // Fire projectile (distance 5-8 hexes)
    Advance,     // Move closer to target (distance > 8 hexes)
}

// Kite system implementation
pub fn kite(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut Kite,
        &Loc,
        &mut Heading,
        &mut Position,
        &mut AirTime,
        Option<&ActorAttributes>,
        Option<&TargetLock>,
        Option<&Returning>,
        Option<&mut common::components::movement_intent_state::MovementIntentState>,  // ADR-011
        &EngagementMember,
    )>,
    q_target: Query<(&Loc, &Health), With<PlayerControlled>>,
    q_spawner: Query<&Loc, Without<Kite>>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
    mut writer: MessageWriter<common::message::Do>,
) {
    for (npc_entity, kite_config, npc_loc, mut npc_heading, mut npc_position, mut npc_airtime, attrs, lock_opt, returning_opt, mut intent_state_opt, engagement_member) in &mut query {

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
                if distance_from_spawn > kite_config.leash_distance {
                    // Too far from spawn - return to spawn
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
                    kite_config.acquisition_range as i32 * kite_config.acquisition_range as i32,
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
                        kite_config.leash_distance,
                        spawner_loc,
                    ));
                    commands.entity(npc_entity).insert(Target { entity: Some(new_target), last_target: Some(new_target) });
                    new_target
                } else {
                    // No targets found - stop kiting
                    continue;
                }
            }
        };

        // 2. GET TARGET LOCATION
        let Ok((target_loc, _)) = q_target.get(target_entity) else {
            continue;
        };

        // 3. CHECK DISTANCE AND DETERMINE ACTION
        let distance = npc_loc.flat_distance(target_loc);
        let action = kite_config.determine_action(distance);

        // Always face target (kiter maintains facing while moving)
        let direction = (**target_loc - **npc_loc).normalize();
        let desired_heading = Heading::new(direction);
        *npc_heading = desired_heading;

        // Fetch spawn location for score-based neighbor selection
        let Ok(&spawner_loc) = q_spawner.get(engagement_member.0) else {
            continue;
        };
        let spawn_qrz = *spawner_loc;
        let optimal_mid = (kite_config.optimal_distance_min + kite_config.optimal_distance_max) / 2;

        // 4. EXECUTE ACTION
        match action {
            KiteAction::Flee | KiteAction::Reposition => {
                // Score-based neighbor selection: balances optimal range + leash safety
                let target_qrz = **target_loc;
                let Some((start, _)) = map.find(**npc_loc, -60) else {
                    continue;
                };

                let neighbors = map.neighbors(start);
                let best_neighbor = neighbors
                    .iter()
                    .filter(|(neighbor, _)| {
                        nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
                    })
                    .max_by_key(|(neighbor, _)| score_neighbor(neighbor, &target_qrz, &spawn_qrz, optimal_mid, kite_config.leash_distance));

                if let Some((next_tile, _)) = best_neighbor {
                    // Move away from target
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

                commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            }
            KiteAction::Attack => {
                // Stay in place — process_passive_auto_attack handles damage via AttackRange(6)
                commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            }
            KiteAction::Advance => {
                // Score-based: when too far, score naturally prefers moving toward optimal range
                let target_qrz = **target_loc;
                let Some((start, _)) = map.find(**npc_loc, -60) else {
                    continue;
                };

                let neighbors = map.neighbors(start);
                let best_neighbor = neighbors
                    .iter()
                    .filter(|(neighbor, _)| {
                        nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
                    })
                    .max_by_key(|(neighbor, _)| score_neighbor(neighbor, &target_qrz, &spawn_qrz, optimal_mid, kite_config.leash_distance));

                if let Some((next_tile, _)) = best_neighbor {
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

                commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kite_forest_sprite_stats() {
        let kite = Kite::forest_sprite();
        assert_eq!(kite.acquisition_range, 15);
        assert_eq!(kite.leash_distance, 30);
        assert_eq!(kite.optimal_distance_min, 5);
        assert_eq!(kite.optimal_distance_max, 8);
        assert_eq!(kite.disengage_distance, 3);
    }

    #[test]
    fn test_determine_action_flee_when_too_close() {
        let kite = Kite::forest_sprite();

        // Distance 0 hexes - FLEE
        assert_eq!(kite.determine_action(0), KiteAction::Flee);

        // Distance 2 hexes - FLEE
        assert_eq!(kite.determine_action(2), KiteAction::Flee);

        // Distance exactly at disengage threshold (3) - NOT flee (>= 3 is safe)
        // NOTE: ADR-010 says "< 3 hexes" so 3 is safe
        assert_ne!(kite.determine_action(3), KiteAction::Flee);
    }

    #[test]
    fn test_determine_action_reposition_when_suboptimal() {
        let kite = Kite::forest_sprite();

        // Distance 3 hexes - REPOSITION (too close, but not flee range)
        assert_eq!(kite.determine_action(3), KiteAction::Reposition);

        // Distance 4 hexes - REPOSITION
        assert_eq!(kite.determine_action(4), KiteAction::Reposition);

        // Distance 5 hexes - NOT reposition (in optimal range now)
        assert_ne!(kite.determine_action(5), KiteAction::Reposition);
    }

    #[test]
    fn test_determine_action_attack_in_optimal_range() {
        let kite = Kite::forest_sprite();

        // Distance 5 hexes - ATTACK (min optimal)
        assert_eq!(kite.determine_action(5), KiteAction::Attack);

        // Distance 6 hexes - ATTACK (mid optimal)
        assert_eq!(kite.determine_action(6), KiteAction::Attack);

        // Distance 8 hexes - ATTACK (max optimal)
        assert_eq!(kite.determine_action(8), KiteAction::Attack);

        // Distance 9 hexes - NOT attack (too far)
        assert_ne!(kite.determine_action(9), KiteAction::Attack);
    }

    #[test]
    fn test_determine_action_advance_when_too_far() {
        let kite = Kite::forest_sprite();

        // Distance 9 hexes - ADVANCE (beyond optimal)
        assert_eq!(kite.determine_action(9), KiteAction::Advance);

        // Distance 15 hexes - ADVANCE (well beyond optimal)
        assert_eq!(kite.determine_action(15), KiteAction::Advance);

        // Distance 8 hexes - NOT advance (in optimal range)
        assert_ne!(kite.determine_action(8), KiteAction::Advance);
    }

    #[test]
    fn test_optimal_range_boundaries() {
        let kite = Kite::forest_sprite();

        // Test boundary conditions for optimal range (5-8 hexes)
        // Just below optimal (4 hexes) - REPOSITION
        assert_eq!(kite.determine_action(4), KiteAction::Reposition);

        // Min optimal (5 hexes) - ATTACK
        assert_eq!(kite.determine_action(5), KiteAction::Attack);

        // Max optimal (8 hexes) - ATTACK
        assert_eq!(kite.determine_action(8), KiteAction::Attack);

        // Just above optimal (9 hexes) - ADVANCE
        assert_eq!(kite.determine_action(9), KiteAction::Advance);
    }

    /// Test that Kite behavior handles distance transitions correctly (ADR-010 Phase 5)
    ///
    /// Validates that the state machine transitions appropriately as distance changes
    #[test]
    fn test_forest_sprite_distance_based_states() {
        let kite = Kite::forest_sprite();

        // Player too close (2 hexes) - FLEE
        assert_eq!(kite.determine_action(2), KiteAction::Flee, "Should flee when player at 2 hexes");

        // Player at 4 hexes - REPOSITION (moving to optimal 5-8 range)
        assert_eq!(kite.determine_action(4), KiteAction::Reposition, "Should reposition at 4 hexes");

        // Player at 6 hexes - ATTACK (optimal range)
        assert_eq!(kite.determine_action(6), KiteAction::Attack, "Should attack at 6 hexes");

        // Player too far (10 hexes) - ADVANCE
        assert_eq!(kite.determine_action(10), KiteAction::Advance, "Should advance at 10 hexes");
    }

    #[test]
    fn test_score_prefers_optimal_distance() {
        let player = Qrz { q: 0, r: 0, z: 0 };
        let spawn = Qrz { q: 0, r: 0, z: 0 };
        let optimal_mid = 6;
        let leash_distance = 30;

        let at_optimal = Qrz { q: 6, r: 0, z: 0 };
        let too_close = Qrz { q: 2, r: 0, z: 0 };
        let too_far = Qrz { q: 12, r: 0, z: 0 };

        let score_optimal = score_neighbor(&at_optimal, &player, &spawn, optimal_mid, leash_distance);
        let score_close = score_neighbor(&too_close, &player, &spawn, optimal_mid, leash_distance);
        let score_far = score_neighbor(&too_far, &player, &spawn, optimal_mid, leash_distance);

        assert!(score_optimal > score_close, "Optimal distance ({}) should score higher than too close ({})", score_optimal, score_close);
        assert!(score_optimal > score_far, "Optimal distance ({}) should score higher than too far ({})", score_optimal, score_far);
    }

    #[test]
    fn test_score_prefers_closer_to_spawn() {
        // Player and spawn in different locations
        let player = Qrz { q: 0, r: 0, z: 0 };
        let spawn = Qrz { q: 10, r: -10, z: 0 };
        let optimal_mid = 6;
        let leash_distance = 30;

        // Both at distance 5 from player, but different distances from spawn
        // (5, -5): dist_to_player = 5, dist_to_spawn = 5
        // (-5, 5): dist_to_player = 5, dist_to_spawn = 15
        let closer_to_spawn = Qrz { q: 5, r: -5, z: 0 };
        let farther_from_spawn = Qrz { q: -5, r: 5, z: 0 };

        assert_eq!(
            closer_to_spawn.flat_distance(&player),
            farther_from_spawn.flat_distance(&player),
            "Both should be equidistant from player"
        );

        let score_closer = score_neighbor(&closer_to_spawn, &player, &spawn, optimal_mid, leash_distance);
        let score_farther = score_neighbor(&farther_from_spawn, &player, &spawn, optimal_mid, leash_distance);

        assert!(score_closer > score_farther,
            "Closer to spawn ({}) should score higher than farther ({})", score_closer, score_farther);
    }

    #[test]
    fn test_score_leash_ramp() {
        // Verify that moving 1 hex farther from spawn is penalized more
        // when already far from spawn (leash weight ramps up).
        let player = Qrz { q: -20, r: 0, z: 0 };
        let spawn = Qrz { q: 0, r: 0, z: 0 };
        let optimal_mid = 6;
        let leash_distance = 20;

        // Near-spawn pair: 5→6 hexes from spawn
        let near_a = Qrz { q: 5, r: 0, z: 0 };
        let near_b = Qrz { q: 6, r: 0, z: 0 };

        // Far-from-spawn pair: 9→10 hexes from spawn (crosses weight threshold)
        let far_a = Qrz { q: 9, r: 0, z: 0 };
        let far_b = Qrz { q: 10, r: 0, z: 0 };

        let cost_near = score_neighbor(&near_a, &player, &spawn, optimal_mid, leash_distance)
            - score_neighbor(&near_b, &player, &spawn, optimal_mid, leash_distance);
        let cost_far = score_neighbor(&far_a, &player, &spawn, optimal_mid, leash_distance)
            - score_neighbor(&far_b, &player, &spawn, optimal_mid, leash_distance);

        assert!(cost_far > cost_near,
            "Marginal cost of 1 hex from spawn should be higher when far (far: {}, near: {})",
            cost_far, cost_near);
    }

}
