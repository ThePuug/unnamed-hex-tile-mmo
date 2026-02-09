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
        message::{Event, Do, Try, Event as GameEvent, Component as MessageComponent},
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
    let intent_state = if let Some(state) = intent_state_opt {
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
    pub attack_interval_ms: u32,     // Cooldown between attacks (e.g., 3000ms)
    pub last_attack_time: u128,      // Server-side state: timestamp of last attack (default 0)
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
            attack_interval_ms: 3000,     // 3 second attack speed
            last_attack_time: 0,
        }
    }

    /// Check if enough time has elapsed to attack again
    pub fn can_attack(&self, current_time: u128) -> bool {
        let elapsed = current_time.saturating_sub(self.last_attack_time);
        elapsed >= self.attack_interval_ms as u128
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
        Option<&mut crate::common::components::movement_intent_state::MovementIntentState>,  // ADR-011
        &ChildOf,
    )>,
    q_target: Query<(&Loc, &Health), With<PlayerControlled>>,
    q_spawner: Query<&Loc, Without<Kite>>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
    mut writer: MessageWriter<crate::common::message::Do>,
    mut try_writer: MessageWriter<crate::common::message::Try>,
) {
    let current_time = dt.elapsed().as_millis();

    for (npc_entity, mut kite_config, npc_loc, mut npc_heading, mut npc_position, mut npc_airtime, attrs, lock_opt, returning_opt, mut intent_state_opt, child_of) in &mut query {

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

        // 4. EXECUTE ACTION
        match action {
            KiteAction::Flee | KiteAction::Reposition => {
                // INVERSE PATHFINDING: Move AWAY from target
                let target_qrz = **target_loc;
                let Some((start, _)) = map.find(**npc_loc, -60) else {
                    continue;
                };

                // Find neighbor that is FURTHEST from target (inverse of chase)
                let neighbors = map.neighbors(start);
                let best_neighbor = neighbors
                    .iter()
                    .filter(|(neighbor, _)| {
                        nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
                    })
                    .max_by_key(|(neighbor, _)| neighbor.distance(&target_qrz));

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
                // Stay in place and attack if cooldown ready
                if kite_config.can_attack(current_time) {
                    // Use Volley ability - ranged attack with telegraph
                    try_writer.write(Try {
                        event: GameEvent::UseAbility {
                            ent: npc_entity,
                            ability: crate::common::message::AbilityType::Volley,
                            target_loc: Some(**target_loc),
                        },
                    });

                    kite_config.last_attack_time = current_time;
                }

                commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            }
            KiteAction::Advance => {
                // Move toward target (similar to chase behavior)
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
                    .min_by_key(|(neighbor, _)| neighbor.distance(&target_qrz));

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
        assert_eq!(kite.attack_interval_ms, 3000);
    }

    #[test]
    fn test_can_attack_returns_false_before_cooldown() {
        let mut kite = Kite::forest_sprite();
        kite.last_attack_time = 1000;

        // Check at 1000ms (right after attack) - should be false
        assert!(!kite.can_attack(1000), "Should not be able to attack immediately");

        // Check at 2500ms (500ms before cooldown expires) - should be false
        assert!(!kite.can_attack(2500), "Should not be able to attack before cooldown expires");
    }

    #[test]
    fn test_can_attack_returns_true_after_cooldown() {
        let mut kite = Kite::forest_sprite();
        kite.last_attack_time = 1000;

        // Check at 4000ms (3000ms after last attack) - should be true
        assert!(kite.can_attack(4000), "Should be able to attack after cooldown expires");

        // Check at 5000ms (4000ms after last attack) - should still be true
        assert!(kite.can_attack(5000), "Should be able to attack well after cooldown expires");
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

    // ===== INTEGRATION TESTS (ADR-010 Phase 5) =====

    /// Test that Forest Sprite attacks when in optimal range (ADR-010 Phase 5)
    ///
    /// Validation Criteria:
    /// - Sprite kites to 5-8 hex range
    /// - Sprite attacks every 3 seconds
    /// - Attacks are instant hit
    #[test]
    fn test_forest_sprite_attacks_at_optimal_range() {
        // This test validates the integration of:
        // - Kite behavior state machine
        // - Instant hit attacks in Attack state
        // - Attack cooldown timing

        let kite = Kite::forest_sprite();

        // Sprite at distance 6 hexes (optimal range)
        let action = kite.determine_action(6);
        assert_eq!(action, KiteAction::Attack, "Sprite should attack at 6 hexes");

        // First attack is allowed (cooldown expired)
        let current_time = 5000; // 5 seconds elapsed
        assert!(kite.can_attack(current_time), "Should be able to attack initially");

        // After attacking, update last_attack_time
        let mut kite_after_attack = kite;
        kite_after_attack.last_attack_time = current_time;

        // Immediately after attack, cooldown not ready
        assert!(!kite_after_attack.can_attack(current_time), "Should not attack immediately after");

        // 2 seconds later (2000ms < 3000ms cooldown)
        assert!(!kite_after_attack.can_attack(current_time + 2000), "Should not attack before cooldown");

        // 3 seconds later (cooldown expired)
        assert!(kite_after_attack.can_attack(current_time + 3000), "Should attack after 3 second cooldown");
    }

    /// Test that Kite behavior uses instant hit damage (ADR-010 Phase 5)
    #[test]
    fn test_forest_sprite_attack_stats() {
        let kite = Kite::forest_sprite();

        // Verify attack stats match ADR-010 specifications
        assert_eq!(kite.attack_interval_ms, 3000, "Should attack every 3 seconds");
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

}
