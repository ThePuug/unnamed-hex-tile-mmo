use bevy::prelude::*;
use rand::seq::IteratorRandom;
use qrz::Qrz;

use common_bevy::{
    components::{
        Loc, resources::Health,
        behaviour::Side, status::Status, ActorAttributes, target::Target,
        returning::Returning, stagger::Stagger,
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

/// Score a neighbor tile for kite movement.
/// Higher score = more preferred destination.
/// Balances staying at optimal distance from player with staying near spawn.
fn score_neighbor(
    neighbor: &Qrz,
    player: &Qrz,
    spawn: &Qrz,
    aim: i32,
    leash_distance: i32,
) -> i32 {
    let dist_to_player = neighbor.flat_distance(player);
    let dist_to_spawn = neighbor.flat_distance(spawn);
    let leash = leash_distance;

    // Prefer being at the aimed distance from player (weight: 3)
    let range_score = -(dist_to_player - aim).abs() * 3;

    // Prefer being closer to spawn, with increasing urgency near leash boundary
    // Weight ramps from 1 (at spawn) to 3 (at leash distance)
    let leash_weight = 1 + dist_to_spawn * 2 / leash;
    let leash_score = -dist_to_spawn * leash_weight;

    range_score + leash_score
}

/// How far a Kiter reaches, in tiles: its auto-attack, its Volley, and
/// the far edge of its band.
pub const KITER_REACH: i32 = 20;

/// Kite behavior - ranged hostile that maintains optimal distance

/// Implements distance-based state machine for ranged kiting enemies:
/// - Acquires hostile targets within aggro range
/// - Maintains sticky targeting via TargetLock
/// - **Holds** within its band, where its auto-attack reaches, turning to
///   face the target: the only distance it opens is its Volley's leap, a
///   mechanic of the skill that lands with the Volley's slow
/// - **Advances** when target is beyond its band (> 6 hexes) - moves closer
/// - **Leashes** when too far from spawner (returns to spawn)

/// # Design Pattern
/// Inverse pathfinding: Kiter moves AWAY from player to maintain distance.
/// Attack timer independent of movement: Continues firing while advancing.
#[derive(Clone, Component, Copy, Debug)]
pub struct Kite {
    pub acquisition_range: u32,      // How far to search for targets
    pub leash_distance: i32,         // Max chase distance from spawn
    pub optimal_distance_max: i32,   // The far edge of its band: no further than its AttackRange
}

impl Kite {
    /// Create a new Kite behavior with Forest Sprite stats
    pub fn forest_sprite() -> Self {
        Self {
            acquisition_range: super::ACQUISITION_RANGE,
            leash_distance: super::LEASH_DISTANCE,
            optimal_distance_max: KITER_REACH,
        }
    }

    /// Determine what action the kiter should take based on distance to
    /// target: within its band it stands and shoots, since walking away a
    /// target as fast as it closes on it anyway, and past it it closes in.
    pub fn determine_action(&self, distance_to_target: i32) -> KiteAction {
        if distance_to_target > self.optimal_distance_max {
            KiteAction::Advance
        } else {
            KiteAction::Attack
        }
    }
}

/// State machine for kiting behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KiteAction {
    Attack,      // Hold and let the auto-attack fire
    Advance,     // Move closer to target (distance > 6 hexes)
}

// Kite system implementation
pub fn kite(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut Kite,
        &Loc,
        Body,
        Option<&ActorAttributes>,
        Option<&TargetLock>,
        Option<&Returning>,
        &EngagementMember,
        Option<&Stagger>,
        &Side,
        Option<&Status>,
    )>,
    q_target: Query<(&Loc, &Health, &Side)>,
    q_spawner: Query<&Loc, Without<Kite>>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    dt: Res<Time>,
    mut writer: MessageWriter<common_bevy::message::Do>,
) {
    for (npc_entity, kite_config, npc_loc, mut body, attrs, lock_opt, returning_opt, engagement_member, stagger_opt, own_side, status) in &mut query {

        // Staggered — skip all movement and intent broadcasting
        if stagger_opt.is_some() {
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
                let nearby = nntree.locate_within_distance(
                    *npc_loc,
                    kite_config.acquisition_range as i64 * kite_config.acquisition_range as i64,
                );

                let valid_targets: Vec<Entity> = nearby
                    .filter_map(|result| {
                        let ent = result.ent;
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
        let Ok((target_loc, _, _)) = q_target.get(target_entity) else {
            continue;
        };

        // 3. CHECK DISTANCE AND DETERMINE ACTION
        let distance = npc_loc.flat_distance(target_loc);
        let action = kite_config.determine_action(distance);

        // Fetch spawn location for score-based neighbor selection
        let Ok(&spawner_loc) = q_spawner.get(engagement_member.0) else {
            continue;
        };
        let spawn_qrz = *spawner_loc;
        // Where it makes for: the far edge of its band, as far as its shots reach
        let aim = kite_config.optimal_distance_max;

        // 4. EXECUTE ACTION
        match action {
            KiteAction::Attack => {
                // Stay in place, turning to face the target — process_passive_auto_attack handles damage via AttackRange(6)
                body.face(npc_loc, **target_loc, dt_ms, &map, &nntree);
                commands.entity(npc_entity).insert(Target { entity: Some(target_entity), last_target: Some(target_entity) });
            }
            KiteAction::Advance => {
                // Score-based: when too far, score naturally prefers moving toward optimal range
                let target_qrz = **target_loc;
                let Some((start, _)) = map.get_by_qr(npc_loc.q, npc_loc.r) else {
                    continue;
                };

                let neighbors = map.neighbors(start);
                let best_neighbor = neighbors
                    .iter()
                    .filter(|(neighbor, _)| {
                        nntree.locate_all_at_point(&Loc::new(*neighbor + qrz::Qrz::Z)).count() < 7
                    })
                    .max_by_key(|(neighbor, _)| score_neighbor(neighbor, &target_qrz, &spawn_qrz, aim, kite_config.leash_distance));

                if let Some((next_tile, _)) = best_neighbor {
                    body.step_toward(npc_loc, start, *next_tile, movement_speed, dt_ms, &map, &nntree);
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
        assert!(kite.acquisition_range as i32 > kite.optimal_distance_max, "it picks targets beyond its reach");
        assert_eq!(kite.optimal_distance_max, KITER_REACH);
    }

    #[test]
    fn it_stands_within_its_band_and_advances_past_it() {
        let kite = Kite::forest_sprite();
        for distance in 0..=kite.optimal_distance_max {
            assert_eq!(kite.determine_action(distance), KiteAction::Attack, "at {distance}");
        }
        assert_eq!(kite.determine_action(kite.optimal_distance_max + 1), KiteAction::Advance);
        assert_eq!(kite.determine_action(kite.optimal_distance_max * 2), KiteAction::Advance);
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
