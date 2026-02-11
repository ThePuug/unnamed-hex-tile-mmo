//! # Hex Assignment System (SOW-018 Phase 2 & 3)
//!
//! Assigns unique approach hexes to melee NPCs in an engagement.
//! Recalculates on player tile change or NPC death.

use bevy::prelude::*;
use bevy::platform::collections::HashMap;
use qrz::Qrz;

use crate::{
    common::{
        components::{
            Loc,
            behaviour::PlayerControlled,
            engagement::{Engagement, EngagementMember},
            hex_assignment::{AssignedHex, HexAssignment},
            resources::Health,
            target::Target,
        },
        plugins::nntree::NNTree,
        resources::map::Map,
        spatial_difficulty::EnemyArchetype,
    },
    server::systems::behaviour::chase::Chase,
};

/// Positioning strategy determines hex preference ordering for each archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositioningStrategy {
    /// Maximize angular spread — Juggernauts surround from all sides
    Surround,
    /// Minimize angular spread — Berserkers cluster on one side
    Cluster,
    /// Hold at 2-3 hex range — Defenders don't compete for adjacent hexes
    Perimeter,
    /// Hold at 3-6 hex range — Kiters orbit at distance
    Orbital,
}

impl EnemyArchetype {
    /// Get the positioning strategy for this archetype.
    ///
    /// All melee archetypes (Chase behavior) use adjacent strategies.
    /// Perimeter/Orbital are reserved for future ranged archetypes.
    pub fn positioning_strategy(&self) -> PositioningStrategy {
        match self {
            EnemyArchetype::Berserker => PositioningStrategy::Cluster,
            EnemyArchetype::Juggernaut => PositioningStrategy::Surround,
            EnemyArchetype::Defender => PositioningStrategy::Surround,
            EnemyArchetype::Kiter => PositioningStrategy::Orbital,
        }
    }
}

/// Angular distance between two neighbor indices on a hex ring (0-5).
/// Wraps around: `min(|a - b|, 6 - |a - b|)`
pub fn angular_distance(a: usize, b: usize) -> usize {
    let diff = if a > b { a - b } else { b - a };
    diff.min(6 - diff)
}

/// Find the DIRECTIONS index for a neighbor hex relative to center.
/// Returns None if the hex is not an immediate neighbor.
fn direction_index(center: Qrz, neighbor: Qrz) -> Option<usize> {
    let delta = neighbor - center;
    // Zero out z for direction comparison
    let flat_delta = Qrz { q: delta.q, r: delta.r, z: 0 };
    qrz::DIRECTIONS.iter().position(|d| *d == flat_delta)
}

/// Assign hexes to NPCs based on engagement archetype strategy.
///
/// Returns a map of NPC entity → assigned hex.
///
/// Strategy priority (for mixed groups): Cluster → Surround → Perimeter/Orbital.
pub fn calculate_assignments(
    player_tile: Qrz,
    npcs: &[(Entity, PositioningStrategy)],
    available_adjacent: &[(Qrz, usize)], // (hex, direction_index)
    available_secondary: &[Qrz],         // hexes at distance 2 from player
) -> HashMap<Entity, Qrz> {
    let mut assignments = HashMap::default();
    let mut taken_adjacent: Vec<usize> = Vec::new(); // direction indices already assigned

    // Sort NPCs by strategy priority: Cluster first, then Surround, then Perimeter/Orbital
    let mut sorted_npcs: Vec<_> = npcs.to_vec();
    sorted_npcs.sort_by_key(|(_, strategy)| match strategy {
        PositioningStrategy::Cluster => 0,
        PositioningStrategy::Surround => 1,
        PositioningStrategy::Perimeter => 2,
        PositioningStrategy::Orbital => 3,
    });

    for (npc, strategy) in &sorted_npcs {
        match strategy {
            PositioningStrategy::Cluster => {
                if let Some(hex) = pick_cluster(available_adjacent, &taken_adjacent) {
                    if let Some(dir_idx) = available_adjacent.iter().find(|(h, _)| *h == hex).map(|(_, d)| *d) {
                        taken_adjacent.push(dir_idx);
                    }
                    assignments.insert(*npc, hex);
                } else if let Some(hex) = pick_secondary(player_tile, available_secondary, &assignments) {
                    assignments.insert(*npc, hex);
                }
            }
            PositioningStrategy::Surround => {
                if let Some(hex) = pick_surround(available_adjacent, &taken_adjacent) {
                    if let Some(dir_idx) = available_adjacent.iter().find(|(h, _)| *h == hex).map(|(_, d)| *d) {
                        taken_adjacent.push(dir_idx);
                    }
                    assignments.insert(*npc, hex);
                } else if let Some(hex) = pick_secondary(player_tile, available_secondary, &assignments) {
                    assignments.insert(*npc, hex);
                }
            }
            PositioningStrategy::Perimeter => {
                // Perimeter NPCs don't compete for adjacent hexes
                if let Some(hex) = pick_secondary(player_tile, available_secondary, &assignments) {
                    assignments.insert(*npc, hex);
                }
            }
            PositioningStrategy::Orbital => {
                // Orbital NPCs don't compete for adjacent hexes either
                if let Some(hex) = pick_secondary(player_tile, available_secondary, &assignments) {
                    assignments.insert(*npc, hex);
                }
            }
        }
    }

    assignments
}

/// Pick the best adjacent hex for Cluster strategy.
/// Minimize angular distance to already-taken faces (pack together).
fn pick_cluster(
    available: &[(Qrz, usize)],
    taken: &[usize],
) -> Option<Qrz> {
    let free: Vec<_> = available.iter()
        .filter(|(_, dir)| !taken.contains(dir))
        .collect();

    if free.is_empty() {
        return None;
    }

    if taken.is_empty() {
        // First NPC — pick any available hex (first one)
        return Some(free[0].0);
    }

    // Pick the free hex closest to any already-taken hex
    free.iter()
        .min_by_key(|(_, dir)| {
            taken.iter().map(|t| angular_distance(*dir, *t)).min().unwrap_or(6)
        })
        .map(|(hex, _)| *hex)
}

/// Pick the best adjacent hex for Surround strategy.
/// Maximize minimum angular distance from already-taken faces (spread out).
fn pick_surround(
    available: &[(Qrz, usize)],
    taken: &[usize],
) -> Option<Qrz> {
    let free: Vec<_> = available.iter()
        .filter(|(_, dir)| !taken.contains(dir))
        .collect();

    if free.is_empty() {
        return None;
    }

    if taken.is_empty() {
        return Some(free[0].0);
    }

    // Pick the free hex that maximizes the minimum angular distance to any taken hex
    free.iter()
        .max_by_key(|(_, dir)| {
            taken.iter().map(|t| angular_distance(*dir, *t)).min().unwrap_or(0)
        })
        .map(|(hex, _)| *hex)
}

/// Pick a secondary position (distance 2+ from player) for overflow NPCs.
fn pick_secondary(
    _player_tile: Qrz,
    available_secondary: &[Qrz],
    current_assignments: &HashMap<Entity, Qrz>,
) -> Option<Qrz> {
    let taken_hexes: Vec<Qrz> = current_assignments.values().copied().collect();

    available_secondary.iter()
        .find(|hex| !taken_hexes.contains(hex))
        .copied()
}

/// System: Assign hexes to melee NPCs in each engagement.
///
/// Runs in FixedUpdate. Triggers reassignment when:
/// - Player tile changes (detected via last_player_tile)
/// - NPC dies (freed hex)
/// - Engagement first acquires a target
pub fn assign_hexes(
    mut commands: Commands,
    mut engagement_query: Query<(&Engagement, &mut HexAssignment)>,
    npc_query: Query<(Entity, &Loc, Option<&Chase>, Option<&Target>), With<EngagementMember>>,
    player_query: Query<(Entity, &Loc), With<PlayerControlled>>,
    health_query: Query<&Health>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (engagement, mut hex_assign) in engagement_query.iter_mut() {
        // Find the target player for this engagement's NPCs
        let target_player = find_engagement_target(engagement, &npc_query);
        let Some(target_player) = target_player else {
            continue; // No NPC has a target yet
        };

        let Ok((_, player_loc)) = player_query.get(target_player) else {
            continue; // Target isn't a player or doesn't exist
        };

        let player_tile = **player_loc;

        // Check if reassignment is needed
        let needs_reassign = hex_assign.target_player != Some(target_player)
            || hex_assign.last_player_tile != Some(player_tile)
            || has_dead_npcs(engagement, &health_query);

        if !needs_reassign {
            continue;
        }

        // Update tracking state
        hex_assign.target_player = Some(target_player);
        hex_assign.last_player_tile = Some(player_tile);

        // Collect living melee NPCs with their strategies
        let alive_npcs: Vec<(Entity, PositioningStrategy)> = engagement.spawned_npcs.iter()
            .filter_map(|&npc_ent| {
                // Check NPC is alive
                let health = health_query.get(npc_ent).ok()?;
                if health.current() <= 0.0 { return None; }

                // Only melee NPCs (those with Chase) get hex assignments
                if npc_query.get(npc_ent).ok()?.2.is_some() {
                    Some((npc_ent, engagement.archetype.positioning_strategy()))
                } else {
                    None
                }
            })
            .collect();

        // Find available adjacent hexes (neighbors of player tile that exist in terrain)
        let terrain_tile = map.find(player_tile, -60);
        let Some((terrain_qrz, _)) = terrain_tile else { continue; };

        let neighbors = map.neighbors(terrain_qrz);
        let available_adjacent: Vec<(Qrz, usize)> = neighbors.iter()
            .filter_map(|(neighbor, _)| {
                let entity_tile = *neighbor + qrz::Qrz::Z;
                // Filter out hexes occupied by non-engagement entities (crowded)
                let occupant_count = nntree.locate_all_at_point(&Loc::new(entity_tile)).count();
                if occupant_count >= 7 { return None; }

                // Find direction index for strategy calculations
                let dir_idx = direction_index(terrain_qrz, *neighbor)?;
                Some((entity_tile, dir_idx))
            })
            .collect();

        // Find secondary positions (distance 2 from player)
        let available_secondary: Vec<Qrz> = neighbors.iter()
            .flat_map(|(neighbor, _)| {
                map.neighbors(*neighbor).into_iter()
                    .filter(|(n2, _)| {
                        let d = n2.flat_distance(&terrain_qrz);
                        d == 2 // Only hexes at distance 2 from player
                    })
                    .map(|(n2, _)| n2 + qrz::Qrz::Z)
            })
            .collect();

        // Calculate assignments
        let new_assignments = calculate_assignments(
            player_tile,
            &alive_npcs,
            &available_adjacent,
            &available_secondary,
        );

        // Apply assignments to NPC entities
        for (npc_ent, hex) in &new_assignments {
            commands.entity(*npc_ent).insert(AssignedHex(*hex));
        }

        // Clean up dead NPC assignments
        hex_assign.assignments.retain(|npc, _| {
            health_query.get(*npc).map(|h| h.current() > 0.0).unwrap_or(false)
        });

        // Store new assignments
        hex_assign.assignments = new_assignments;
    }
}

/// Find the player that this engagement's NPCs are targeting.
fn find_engagement_target(
    engagement: &Engagement,
    npc_query: &Query<(Entity, &Loc, Option<&Chase>, Option<&Target>), With<EngagementMember>>,
) -> Option<Entity> {
    for &npc_ent in &engagement.spawned_npcs {
        if let Ok((_, _, _, Some(target))) = npc_query.get(npc_ent) {
            if let Some(target_ent) = target.entity {
                return Some(target_ent);
            }
        }
    }
    None
}

/// Check if any NPC in the engagement has died (for reassignment trigger).
fn has_dead_npcs(engagement: &Engagement, health_query: &Query<&Health>) -> bool {
    engagement.spawned_npcs.iter().any(|&npc_ent| {
        health_query.get(npc_ent).map(|h| h.current() <= 0.0).unwrap_or(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angular_distance_same() {
        assert_eq!(angular_distance(0, 0), 0);
        assert_eq!(angular_distance(3, 3), 0);
    }

    #[test]
    fn angular_distance_adjacent() {
        assert_eq!(angular_distance(0, 1), 1);
        assert_eq!(angular_distance(5, 0), 1); // wraps
    }

    #[test]
    fn angular_distance_opposite() {
        assert_eq!(angular_distance(0, 3), 3);
        assert_eq!(angular_distance(1, 4), 3);
    }

    #[test]
    fn angular_distance_symmetric() {
        for a in 0..6 {
            for b in 0..6 {
                assert_eq!(angular_distance(a, b), angular_distance(b, a));
            }
        }
    }

    #[test]
    fn cluster_first_picks_any() {
        let available = vec![
            (Qrz { q: 1, r: 0, z: 1 }, 3), // east
            (Qrz { q: 0, r: 1, z: 1 }, 2), // south-east
        ];
        let taken = vec![];
        let hex = pick_cluster(&available, &taken);
        assert!(hex.is_some());
    }

    #[test]
    fn cluster_packs_adjacent() {
        let available = vec![
            (Qrz { q: 1, r: 0, z: 1 }, 3),    // east
            (Qrz { q: 1, r: -1, z: 1 }, 4),   // north-east
            (Qrz { q: -1, r: 0, z: 1 }, 0),   // west (opposite)
        ];
        let taken = vec![3]; // east is taken
        let hex = pick_cluster(&available, &taken);
        // Should pick north-east (dir 4, angular distance 1 from east)
        assert_eq!(hex, Some(Qrz { q: 1, r: -1, z: 1 }));
    }

    #[test]
    fn surround_spreads_out() {
        let available = vec![
            (Qrz { q: 1, r: 0, z: 1 }, 3),    // east
            (Qrz { q: 1, r: -1, z: 1 }, 4),   // north-east
            (Qrz { q: -1, r: 0, z: 1 }, 0),   // west (opposite)
        ];
        let taken = vec![3]; // east is taken
        let hex = pick_surround(&available, &taken);
        // Should pick west (dir 0, angular distance 3 — maximum spread from east)
        assert_eq!(hex, Some(Qrz { q: -1, r: 0, z: 1 }));
    }

    #[test]
    fn surround_2_juggernauts_opposite() {
        // All 6 adjacent hexes available — 2 Juggernauts should spread maximally (180°)
        let available: Vec<(Qrz, usize)> = (0..6).map(|i| {
            let hex = Qrz { q: 0, r: 0, z: 0 } + qrz::DIRECTIONS[i] + qrz::Qrz::Z;
            (hex, i)
        }).collect();

        let npc1 = Entity::from_raw_u32(1).unwrap();
        let npc2 = Entity::from_raw_u32(2).unwrap();

        let npcs = vec![
            (npc1, PositioningStrategy::Surround),
            (npc2, PositioningStrategy::Surround),
        ];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &[],
        );

        assert_eq!(assignments.len(), 2);

        let dirs: Vec<usize> = assignments.values()
            .map(|hex| available.iter().find(|(h, _)| h == hex).unwrap().1)
            .collect();

        // 2 Juggernauts should be opposite (angular distance 3 = 180°)
        assert_eq!(angular_distance(dirs[0], dirs[1]), 3,
            "Two Juggernauts should be opposite (180°)");
    }

    #[test]
    fn surround_3_juggernauts_spread() {
        // 3 Juggernauts: first two at distance 3 (opposite), third maximizes min
        let available: Vec<(Qrz, usize)> = (0..6).map(|i| {
            let hex = Qrz { q: 0, r: 0, z: 0 } + qrz::DIRECTIONS[i] + qrz::Qrz::Z;
            (hex, i)
        }).collect();

        let npc1 = Entity::from_raw_u32(1).unwrap();
        let npc2 = Entity::from_raw_u32(2).unwrap();
        let npc3 = Entity::from_raw_u32(3).unwrap();

        let npcs = vec![
            (npc1, PositioningStrategy::Surround),
            (npc2, PositioningStrategy::Surround),
            (npc3, PositioningStrategy::Surround),
        ];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &[],
        );

        assert_eq!(assignments.len(), 3);

        // All 3 should get unique hexes
        let hexes: Vec<Qrz> = assignments.values().copied().collect();
        assert_ne!(hexes[0], hexes[1]);
        assert_ne!(hexes[1], hexes[2]);
        assert_ne!(hexes[0], hexes[2]);
    }

    #[test]
    fn cluster_3_berserkers_adjacent() {
        let available: Vec<(Qrz, usize)> = (0..6).map(|i| {
            let hex = Qrz { q: 0, r: 0, z: 0 } + qrz::DIRECTIONS[i] + qrz::Qrz::Z;
            (hex, i)
        }).collect();

        let npc1 = Entity::from_raw_u32(1).unwrap();
        let npc2 = Entity::from_raw_u32(2).unwrap();
        let npc3 = Entity::from_raw_u32(3).unwrap();

        let npcs = vec![
            (npc1, PositioningStrategy::Cluster),
            (npc2, PositioningStrategy::Cluster),
            (npc3, PositioningStrategy::Cluster),
        ];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &[],
        );

        assert_eq!(assignments.len(), 3);

        let mut dirs: Vec<usize> = assignments.values()
            .map(|hex| available.iter().find(|(h, _)| h == hex).unwrap().1)
            .collect();
        dirs.sort();

        // All 3 should be adjacent to each other (max angular distance 2)
        for i in 0..dirs.len() {
            for j in (i + 1)..dirs.len() {
                let dist = angular_distance(dirs[i], dirs[j]);
                assert!(dist <= 2, "Berserkers should cluster, got angular distance {}", dist);
            }
        }
    }

    #[test]
    fn mixed_cluster_then_surround() {
        let available: Vec<(Qrz, usize)> = (0..6).map(|i| {
            let hex = Qrz { q: 0, r: 0, z: 0 } + qrz::DIRECTIONS[i] + qrz::Qrz::Z;
            (hex, i)
        }).collect();

        let berserker1 = Entity::from_raw_u32(1).unwrap();
        let berserker2 = Entity::from_raw_u32(2).unwrap();
        let juggernaut = Entity::from_raw_u32(3).unwrap();

        let npcs = vec![
            (berserker1, PositioningStrategy::Cluster),
            (berserker2, PositioningStrategy::Cluster),
            (juggernaut, PositioningStrategy::Surround),
        ];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &[],
        );

        assert_eq!(assignments.len(), 3);

        // Get berserker directions
        let b1_dir = available.iter().find(|(h, _)| *h == assignments[&berserker1]).unwrap().1;
        let b2_dir = available.iter().find(|(h, _)| *h == assignments[&berserker2]).unwrap().1;
        let j_dir = available.iter().find(|(h, _)| *h == assignments[&juggernaut]).unwrap().1;

        // Berserkers should be adjacent (angular distance <= 1)
        assert!(angular_distance(b1_dir, b2_dir) <= 1,
            "Berserkers should cluster: angular distance = {}", angular_distance(b1_dir, b2_dir));

        // Juggernaut should be away from the berserker cluster
        let dist_to_b1 = angular_distance(j_dir, b1_dir);
        let dist_to_b2 = angular_distance(j_dir, b2_dir);
        let min_dist = dist_to_b1.min(dist_to_b2);
        assert!(min_dist >= 2,
            "Juggernaut should be spread from cluster: min angular distance = {}", min_dist);
    }

    #[test]
    fn overflow_npcs_get_secondary() {
        // Only 2 adjacent hexes available, 3 NPCs
        let available = vec![
            (Qrz { q: 1, r: 0, z: 1 }, 3),
            (Qrz { q: 0, r: 1, z: 1 }, 2),
        ];
        let secondary = vec![
            Qrz { q: 2, r: 0, z: 1 },
        ];

        let npc1 = Entity::from_raw_u32(1).unwrap();
        let npc2 = Entity::from_raw_u32(2).unwrap();
        let npc3 = Entity::from_raw_u32(3).unwrap();

        let npcs = vec![
            (npc1, PositioningStrategy::Surround),
            (npc2, PositioningStrategy::Surround),
            (npc3, PositioningStrategy::Surround),
        ];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &secondary,
        );

        assert_eq!(assignments.len(), 3);

        // Two should be on adjacent hexes, one on secondary
        let adjacent_count = assignments.values()
            .filter(|hex| available.iter().any(|(h, _)| h == *hex))
            .count();
        assert_eq!(adjacent_count, 2);
    }

    #[test]
    fn no_available_hexes_uses_secondary() {
        let available: Vec<(Qrz, usize)> = vec![];
        let secondary = vec![Qrz { q: 2, r: 0, z: 1 }];

        let npc = Entity::from_raw_u32(1).unwrap();
        let npcs = vec![(npc, PositioningStrategy::Surround)];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &secondary,
        );

        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[&npc], Qrz { q: 2, r: 0, z: 1 });
    }

    #[test]
    fn perimeter_uses_secondary_not_adjacent() {
        let available = vec![
            (Qrz { q: 1, r: 0, z: 1 }, 3),
        ];
        let secondary = vec![
            Qrz { q: 2, r: 0, z: 1 },
        ];

        let npc = Entity::from_raw_u32(1).unwrap();
        let npcs = vec![(npc, PositioningStrategy::Perimeter)];

        let assignments = calculate_assignments(
            Qrz { q: 0, r: 0, z: 0 },
            &npcs,
            &available,
            &secondary,
        );

        // Perimeter should use secondary, not adjacent
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[&npc], Qrz { q: 2, r: 0, z: 1 });
    }
}
