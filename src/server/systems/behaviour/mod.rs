pub mod face_target;
pub mod find_target;
pub mod pathto;
pub mod use_ability;

use bevy::prelude::*;
use bevy_behave::prelude::*;
use rand::seq::IteratorRandom;
use qrz::Qrz;

use crate::common::{
    components::{entity_type::EntityType, heading::Heading, *},
    message::{AbilityType, Try, Event as GameEvent},
    plugins::nntree::*,
    systems::targeting::*,
};

#[derive(Clone, Component, Copy, Deref, DerefMut)]
pub struct Target(Entity);

impl Target {
    pub fn new(ent: Entity) -> Self { Self(ent) }
}

/// Origin for Nearby component - where to measure distance from
#[derive(Clone, Component, Copy, Debug)]
pub enum NearbyOrigin {
    /// Use the current Target entity's location
    Target,
    /// Use the current Dest location
    Dest,
    /// Use a specific Loc
    Loc(Loc),
}

/// Picks a random hex location within [min, max] distance from origin and sets it as Dest
/// To set Dest exactly to Target location, use: Nearby { origin: NearbyOrigin::Target, min: 0, max: 0 }
#[derive(Clone, Component, Copy, Debug)]
pub struct Nearby {
    pub min: u16,
    pub max: u16,
    pub origin: NearbyOrigin,
}

#[derive(Clone, Component, Copy, Default)]
pub struct FindSomethingInterestingWithin {
    pub dist: u16,
}

pub fn find_something_interesting_within(
    mut commands: Commands,
    mut query: Query<(&FindSomethingInterestingWithin, &BehaveCtx)>,
    q_target: Query<(&Loc, &NearestNeighbor)>,
    q_other: Query<(Entity, &EntityType, &NearestNeighbor)>,
    nntree: Res<NNTree>,
) {
    for (&behaviour, &ctx) in &mut query {
        let Ok((&loc, &nn)) = q_target.get(ctx.target_entity()) else { continue };
        let dist = behaviour.dist as i32;
        let others = nntree.locate_within_distance(loc, dist*dist).map(
            |it| q_other.get(it.ent).expect("missing other entity")
        );
        let Some((o_ent, _, _)) = others.filter(|it| {
            let &(_, _, &o_nn) = it;
            o_nn != nn
        }).choose(&mut rand::rng()) else {
            continue
        };
        commands.entity(ctx.target_entity()).insert(Target::new(o_ent));
        commands.trigger(ctx.success());
    }
}

/// System that processes Nearby components - picks random hex near origin and sets as Dest
pub fn nearby(
    mut commands: Commands,
    mut query: Query<(&Nearby, &BehaveCtx)>,
    q_target: Query<&Loc>,
    q_entity: Query<(&Loc, Option<&Target>, Option<&crate::common::components::Dest>)>,
) {
    for (&nearby_comp, &ctx) in &mut query {
        let target_entity = ctx.target_entity();

        // Determine origin location based on NearbyOrigin variant
        let origin_loc = match nearby_comp.origin {
            NearbyOrigin::Target => {
                // Get Target component and resolve to Loc
                let Ok((_, Some(target), _)) = q_entity.get(target_entity) else { continue };
                let Ok(&target_loc) = q_target.get(**target) else { continue };
                target_loc
            }
            NearbyOrigin::Dest => {
                // Get current Dest
                let Ok((_, _, Some(dest))) = q_entity.get(target_entity) else { continue };
                Loc::new(**dest)
            }
            NearbyOrigin::Loc(loc) => loc,
        };

        // Pick random distance in range [min, max]
        let distance = rand::random::<u16>() % (nearby_comp.max - nearby_comp.min + 1) + nearby_comp.min;

        // Get all hexes at that distance
        let origin_qrz = *origin_loc;
        let candidates: Vec<Qrz> = generate_hexes_at_distance(origin_qrz, distance as i16);

        // Pick random hex from candidates
        if let Some(&chosen) = candidates.iter().choose(&mut rand::rng()) {
            commands.entity(target_entity).insert(crate::common::components::Dest(chosen));
            commands.trigger(ctx.success());
        }
    }
}

/// Generate all hexes at exactly the given distance from origin
fn generate_hexes_at_distance(origin: Qrz, distance: i16) -> Vec<Qrz> {
    let mut hexes = Vec::new();

    // For hexagonal grids, we walk around a ring at the given distance
    // Using cube coordinates: q, r, s where s = -q - r
    for dq in -distance..=distance {
        let min_dr = (-distance).max(-dq - distance);
        let max_dr = distance.min(-dq + distance);
        for dr in min_dr..=max_dr {
            let candidate = Qrz {
                q: origin.q + dq,
                r: origin.r + dr,
                z: origin.z, // Keep same elevation
            };

            // Only include if exactly at target distance
            if origin.distance(&candidate) == distance {
                hexes.push(candidate);
            }
        }
    }

    hexes
}

/// Attack behavior - turns to face target and uses BasicAttack if in range
#[derive(Clone, Component, Copy, Default)]
pub struct AttackTarget;

/// System that makes NPCs attack their target using directional targeting
pub fn attack_target(
    mut commands: Commands,
    mut query: Query<(&AttackTarget, &BehaveCtx)>,
    mut npc_query: Query<(&Loc, &mut Heading, Option<&Target>)>,
    player_query: Query<(&EntityType, &Loc)>,
    nntree: Res<NNTree>,
    mut writer: EventWriter<Try>,
) {
    for (_attack_comp, &ctx) in &mut query {
        let target_entity = ctx.target_entity();

        // Get NPC's location, heading, and target
        let Ok((npc_loc, mut npc_heading, target_opt)) = npc_query.get_mut(target_entity) else {
            commands.trigger(ctx.success());
            continue;
        };

        // Get the target entity from Target component
        let Some(target) = target_opt else {
            commands.trigger(ctx.success());
            continue;
        };

        // Get target's location
        let Ok((_target_type, target_loc)) = player_query.get(**target) else {
            continue;
        };

        // Calculate direction vector from NPC to target
        let direction = Qrz {
            q: target_loc.q - npc_loc.q,
            r: target_loc.r - npc_loc.r,
            z: 0, // Ignore elevation for heading
        };

        // Normalize to one of the 6 cardinal hex directions
        // Find the closest cardinal direction
        let new_heading = if direction.q.abs() > direction.r.abs() {
            // Q is dominant
            if direction.q > 0 {
                Heading::new(Qrz { q: 1, r: 0, z: 0 }) // East
            } else {
                Heading::new(Qrz { q: -1, r: 0, z: 0 }) // West
            }
        } else if direction.r.abs() > 0 {
            // R is dominant
            if direction.r > 0 {
                Heading::new(Qrz { q: 0, r: 1, z: 0 }) // Southeast
            } else {
                Heading::new(Qrz { q: 0, r: -1, z: 0 }) // Northwest
            }
        } else if direction.q > 0 && direction.r < 0 {
            Heading::new(Qrz { q: 1, r: -1, z: 0 }) // Northeast
        } else {
            Heading::new(Qrz { q: -1, r: 1, z: 0 }) // Southwest
        };

        *npc_heading = new_heading;

        // Check if target is in attack range (adjacent = distance 1)
        let distance = npc_loc.distance(target_loc);
        if distance == 1 {
            // Use select_target to verify target is in facing cone
            let selected = select_target(
                target_entity, // NPC entity
                *npc_loc,
                *npc_heading,
                None, // No tier lock
                &nntree,
                |ent| player_query.get(ent).ok().map(|(et, _)| *et),
            );

            // If target is in facing cone, attack!
            if selected == Some(**target) {
                writer.write(Try {
                    event: GameEvent::UseAbility {
                        ent: target_entity,
                        ability: AbilityType::BasicAttack,
                    },
                });
            }
        }

        // Always mark behavior as success so the tree can loop
        // (Whether we attacked or not, move on to the wait/repeat)
        commands.trigger(ctx.success());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    #[test]
    fn nearby_generates_hexes_at_correct_distance() {
        // Test the helper function
        let origin = Qrz { q: 0, r: 0, z: 0 };

        // Test distance=2
        let hexes = generate_hexes_at_distance(origin, 2);

        // All hexes should be exactly distance 2 from origin
        for hex in &hexes {
            assert_eq!(origin.distance(hex), 2, "Hex {:?} should be distance 2 from origin", hex);
        }

        // Should have multiple hexes at this distance
        assert!(hexes.len() > 6, "Should have multiple hexes at distance 2");
    }

    #[test]
    fn nearby_generates_origin_for_distance_zero() {
        // Test that distance=0 returns only the origin tile
        let origin = Qrz { q: 5, r: 3, z: 0 };

        let hexes = generate_hexes_at_distance(origin, 0);

        println!("Distance 0 hexes: {:?}", hexes);
        println!("Hex count: {}", hexes.len());

        // Distance 0 should return exactly [origin]
        assert_eq!(hexes.len(), 1, "Distance 0 should return exactly 1 hex (the origin)");
        assert_eq!(hexes[0], origin, "Distance 0 should return the origin tile itself");
    }

    #[test]
    fn nearby_picks_random_hex_within_range() {
        // Test the core logic without bevy_behave integration
        let origin_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Run multiple times to test randomness
        let mut destinations = std::collections::HashSet::new();
        for _ in 0..50 {
            // Pick random distance in range [2, 3]
            let min = 2;
            let max = 3;
            let distance = rand::random::<u16>() % (max - min + 1) + min;

            // Get all hexes at that distance
            let candidates: Vec<Qrz> = generate_hexes_at_distance(*origin_loc, distance as i16);

            // Pick random hex from candidates
            let chosen = candidates.iter().choose(&mut rand::rng())
                .expect("Should have candidates");

            let dist = origin_loc.distance(chosen);

            // Verify distance is within range [min, max]
            assert!(dist >= 2, "Distance {} should be >= min (2)", dist);
            assert!(dist <= 3, "Distance {} should be <= max (3)", dist);

            destinations.insert(*chosen);
        }

        // Verify randomness: should have picked multiple different destinations
        assert!(destinations.len() > 5, "Should pick varied destinations, got {}", destinations.len());
    }
}