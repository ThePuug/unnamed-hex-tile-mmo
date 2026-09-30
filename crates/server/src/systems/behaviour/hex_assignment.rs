//! Assigns each melee NPC in an engagement its own hex to stand on, round
//! its target at its `AttackRange`, so each swings from its reach and never
//! closes past it. The NPCs of an engagement spread round the ring, each
//! taking the free place furthest from those taken and, among places alike,
//! the one nearest it. Recalculates when the target changes tile or an NPC
//! dies.

use bevy::prelude::*;
use bevy::platform::collections::HashMap;
use qrz::Qrz;

use common_bevy::{
    components::{
        Loc,
        behaviour::Side,
        engagement::{Engagement, EngagementMember},
        hex_assignment::{AssignedHex, HexAssignment},
        resources::Health,
        target::Target,
    },
    plugins::nntree::NNTree,
    resources::map::Map,
};
use crate::systems::behaviour::chase::Chase;

/// Steps between two entries of a ring `slots` long, the shorter way
/// round: `min(|a - b|, slots - |a - b|)`.
pub fn angular_distance(a: usize, b: usize, slots: usize) -> usize {
    let diff = if a > b { a - b } else { b - a };
    diff.min(slots - diff)
}

/// Whether an NPC standing on `place` reaches a target on `target`, by the
/// measure its swing checks, `Loc::distance`: on a slope a hex at flat reach
/// may stand too far above or below to swing from.
fn swings_from(place: Qrz, target: Qrz, reach: u32) -> bool {
    Loc::new(place).distance(&Loc::new(target)) <= reach as i32
}

/// The hex each of `npcs` stands on, each given with the tile it is on
/// now. `available_reach` holds the free hexes of the ring at reach, each
/// with its index on that ring, `slots` long; `available_secondary` the
/// ring beyond, where an NPC with no place at reach waits.
///
/// Each takes the place at reach furthest round the ring from those taken
/// and, among places alike, the nearest to it, so it closes from the side
/// it approaches on. A fixed first pick sends a lone NPC round to one face
/// of its target, and two actors chasing each other leapfrog across the map.
pub fn calculate_assignments(
    npcs: &[(Entity, Qrz)],
    available_reach: &[(Qrz, usize)],
    slots: usize,
    available_secondary: &[Qrz],
) -> HashMap<Entity, Qrz> {
    let mut assignments = HashMap::default();
    let mut taken: Vec<usize> = Vec::new();
    for &(npc, from) in npcs {
        let at_reach = available_reach.iter()
            .filter(|(_, place)| !taken.contains(place))
            .min_by_key(|(hex, place)| {
                let spread = taken.iter().map(|t| angular_distance(*place, *t, slots)).min().unwrap_or(0);
                (std::cmp::Reverse(spread), hex.flat_distance(&from))
            });
        let hex = match at_reach {
            Some(&(hex, place)) => {
                taken.push(place);
                Some(hex)
            }
            None => available_secondary.iter().find(|hex| !assignments.values().any(|held| held == *hex)).copied(),
        };
        if let Some(hex) = hex {
            assignments.insert(npc, hex);
        }
    }
    assignments
}

/// Assigns the living melee NPCs of each engagement their hexes, when the
/// engagement first has a target, when that target changes tile, and while
/// any of its NPCs is dead.
pub fn assign_hexes(
    mut commands: Commands,
    mut engagement_query: Query<(&Engagement, &mut HexAssignment)>,
    npc_query: Query<(Entity, &Loc, Option<&Chase>, Option<&Target>), With<EngagementMember>>,
    player_query: Query<(Entity, &Loc), With<Side>>,
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
            continue; // Target isn't an actor or doesn't exist
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

        // Collect living melee NPCs, and the reach every one of them
        // swings from
        let mut reach = u32::MAX;
        let alive_npcs: Vec<(Entity, Qrz)> = engagement.spawned_npcs.iter()
            .filter_map(|&npc_ent| {
                // Check NPC is alive
                let health = health_query.get(npc_ent).ok()?;
                if health.current() <= 0.0 { return None; }

                // Only melee NPCs take a hex: one that fights from range
                // stands wherever its shots reach
                let (_, loc, chase, _) = npc_query.get(npc_ent).ok()?;
                let chase = chase.filter(|chase| chase.attack_range <= common_bevy::components::AttackRange::default().0)?;
                reach = reach.min(chase.attack_range.max(1) as u32);
                Some((npc_ent, **loc))
            })
            .collect();
        // With none alive the rings go unused and the assignments empty
        let reach = if alive_npcs.is_empty() { 1 } else { reach };

        // The standing tile over each floor of a ring round the target
        // that exists in terrain and is not crowded
        let standing = |ring: Vec<Qrz>| -> Vec<(Qrz, usize)> {
            ring.into_iter().enumerate()
                .filter_map(|(i, hex)| {
                    let (floor, _) = map.get_by_qr(hex.q, hex.r)?;
                    let entity_tile = floor + qrz::Qrz::Z;
                    let occupant_count = nntree.locate_all_at_point(&Loc::new(entity_tile)).count();
                    (occupant_count < 7).then_some((entity_tile, i))
                })
                .collect()
        };
        let available_reach: Vec<(Qrz, usize)> = standing(player_tile.ring(reach)).into_iter()
            .filter(|(hex, _)| swings_from(*hex, player_tile, reach))
            .collect();
        let available_secondary: Vec<Qrz> = standing(player_tile.ring(reach + 1)).into_iter().map(|(hex, _)| hex).collect();

        for (npc_ent, hex) in calculate_assignments(&alive_npcs, &available_reach, 6 * reach as usize, &available_secondary) {
            commands.entity(npc_ent).insert(AssignedHex(hex));
        }
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

    const TARGET: Qrz = Qrz { q: 0, r: 0, z: 0 };

    /// Every standing tile of the ring at `reach` round the target, each
    /// with its place on it
    fn ring(reach: u32) -> Vec<(Qrz, usize)> {
        TARGET.ring(reach).into_iter().map(|hex| hex + Qrz::Z).enumerate().map(|(i, hex)| (hex, i)).collect()
    }

    /// `count` NPCs, all standing on the target's tile
    fn pack(count: u32) -> Vec<(Entity, Qrz)> {
        (1..=count).map(|i| (Entity::from_raw_u32(i).unwrap(), TARGET + Qrz::Z)).collect()
    }

    fn place_of(hex: &Qrz, available: &[(Qrz, usize)]) -> usize {
        available.iter().find(|(h, _)| h == hex).unwrap().1
    }

    #[test]
    fn angular_distance_is_the_shorter_way_round() {
        assert_eq!(angular_distance(3, 3, 6), 0);
        assert_eq!(angular_distance(0, 1, 6), 1);
        assert_eq!(angular_distance(5, 0, 6), 1, "it wraps");
        assert_eq!(angular_distance(1, 4, 6), 3);
        for a in 0..6 {
            for b in 0..6 {
                assert_eq!(angular_distance(a, b, 6), angular_distance(b, a, 6));
            }
        }
    }

    #[test]
    fn a_place_too_far_up_or_down_the_slope_is_out_of_reach() {
        let level = Qrz { q: 2, r: 0, z: 0 };
        let one_step = Qrz { q: 2, r: 0, z: 1 };
        let above = Qrz { q: 1, r: 0, z: 3 };
        let below = Qrz { q: 1, r: 0, z: -3 };
        assert!(swings_from(level, TARGET, 2));
        assert!(swings_from(one_step, TARGET, 2));
        assert!(!swings_from(above, TARGET, 2));
        assert!(!swings_from(below, TARGET, 2));
    }

    #[test]
    fn first_pick_is_the_hex_nearest_the_npc() {
        let available = ring(1);
        let npc = Entity::from_raw_u32(1).unwrap();
        for (hex, place) in &available {
            let from = Qrz { q: hex.q * 4, r: hex.r * 4, z: 1 };
            let assignments = calculate_assignments(&[(npc, from)], &available, 6, &[]);
            assert_eq!(assignments[&npc], *hex, "from beyond place {place}");
        }
    }

    #[test]
    fn a_pair_stands_opposite_at_any_reach() {
        for reach in [1, 2] {
            let available = ring(reach);
            let slots = 6 * reach as usize;
            let assignments = calculate_assignments(&pack(2), &available, slots, &[]);
            let places: Vec<usize> = assignments.values().map(|hex| place_of(hex, &available)).collect();
            for hex in assignments.values() {
                assert_eq!(hex.flat_distance(&TARGET), reach as i32, "every place is at reach");
            }
            assert_eq!(angular_distance(places[0], places[1], slots), slots / 2, "reach {reach}");
        }
    }

    #[test]
    fn each_npc_takes_a_place_of_its_own() {
        let available = ring(1);
        let assignments = calculate_assignments(&pack(6), &available, 6, &[]);
        let mut places: Vec<usize> = assignments.values().map(|hex| place_of(hex, &available)).collect();
        places.sort();
        assert_eq!(places, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn with_no_place_at_reach_an_npc_waits_on_the_ring_beyond() {
        let available = vec![(Qrz { q: 1, r: 0, z: 1 }, 3), (Qrz { q: 0, r: 1, z: 1 }, 2)];
        let secondary = vec![Qrz { q: 2, r: 0, z: 1 }];
        let assignments = calculate_assignments(&pack(3), &available, 6, &secondary);
        assert_eq!(assignments.len(), 3);
        let at_reach = assignments.values().filter(|hex| available.iter().any(|(h, _)| h == *hex)).count();
        assert_eq!(at_reach, 2);
        assert!(assignments.values().any(|hex| *hex == secondary[0]));

        let alone = calculate_assignments(&pack(1), &[], 6, &secondary);
        assert_eq!(alone.values().next(), Some(&secondary[0]));
    }
}
