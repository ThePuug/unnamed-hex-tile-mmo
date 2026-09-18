//! Movement physics as pure functions: a position, a heading, whether the
//! entity moves, and a duration in, a position and airborne state out.
//!
//! The result does not depend on how a duration is split across calls.
//! Every sub-step is linear in time and the ground height is a function of
//! position, so a client replaying its inputs and a server applying them as
//! they arrive reach the same place. Anything added here must keep that:
//! no per-call smoothing, no per-step constants that are not scaled by `dt`.

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{
    components::{
        entity_type::{decorator::*, EntityType},
        heading::Heading,
        position::Position,
        Loc,
    },
    plugins::nntree::NNTree,
    resources::map::Map,
};

/// Gravity acceleration in world units per millisecond squared
pub const GRAVITY: f32 = 0.005;

/// Jump ascent multiplier - jumping is 5x faster than falling
pub const JUMP_ASCENT_MULTIPLIER: f32 = 5.0;

/// Jump duration in milliseconds
pub const JUMP_DURATION_MS: i16 = 125;

/// Longest sub-step in milliseconds. Bounds how far a step can carry
/// before blocking and landing are checked again.
pub const PHYSICS_TIMESTEP_MS: i16 = 125;

/// Base movement speed in world units per millisecond
pub const MOVEMENT_SPEED: f32 = 0.0075;

/// Ledge grab threshold in world units
/// Set to 0.0 to disable ledge grabbing
pub const LEDGE_GRAB_THRESHOLD: f32 = 0.0;

/// Maximum entity count per tile before considering it solid
pub const MAX_ENTITIES_PER_TILE: usize = 7;

/// The depth of water a walker wades, in z-levels: one, the same step that
/// bounds a climb. Deeper water is not entered; crossing it, by swimming, a
/// ford, a bridge or a boat, is undesigned, so it blocks.
pub const WADE_DEPTH: i32 = 1;

/// Whether a tile stands under more water than a walker wades.
pub fn is_deep_water(map: &Map, q: i32, r: i32) -> bool {
    match (map.water_at(q, r), map.get_by_qr(q, r)) {
        (Some(surface), Some((floor, _))) => surface - floor.z > WADE_DEPTH,
        _ => false,
    }
}

/// World height of the standing level over `floor`: one z-level up.
pub fn standing_y(floor: Qrz, map: &Map) -> f32 {
    let standing: Vec3 = map.convert(Qrz { z: floor.z + 1, ..floor });
    standing.y
}

/// Height of the terrain surface under `world_xz`, standing on `floor` — the
/// same fan surface the mesh draws (`surface::surface_y`), so an entity's
/// feet stay on what is rendered, across tile edges and up to a cliff's
/// face. Whether a tile may be entered is decided on tile z elsewhere; this
/// only says how high the ground is.
pub fn surface_y(world_xz: Vec2, floor: Qrz, map: &Map) -> f32 {
    let floor_centre: Vec3 = map.convert(floor);
    crate::surface::surface_y(world_xz, floor, floor_centre, |q, r| {
        map.get_by_qr(q, r).map(|(qrz, _)| qrz.z)
    })
}

#[derive(Clone, Copy, Debug)]
pub struct MovementInput {
    pub position: Position,
    /// The direction a move travels along.
    pub heading: Heading,
    pub moving: bool,
    /// Some(positive) = ascending, Some(negative or zero) = falling, None = grounded
    pub airtime: Option<i16>,
    /// World units per millisecond
    pub movement_speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementOutput {
    pub position: Position,
    pub airtime: Option<i16>,
}

/// Whether a step from the tile over `here_floor` may land in `next`, for an
/// entity whose feet are at world height `world_y`. Refused by a rise of more
/// than one level unless airborne at or above its standing height, by a
/// solid decorator with no floor, by a tile at its entity capacity, and by
/// water deeper than a walker wades.
pub fn is_tile_blocked(
    here_floor: Option<Qrz>,
    next: Qrz,
    world_y: f32,
    airtime: Option<i16>,
    map: &Map,
    nntree: &NNTree,
) -> bool {
    let next_floor = map.get_by_qr(next.q, next.r).map(|(floor, _)| floor);

    let cliff = match (here_floor, next_floor) {
        (Some(here), Some(there)) if there.z - here.z > 1 => {
            airtime.is_none() || world_y + LEDGE_GRAB_THRESHOLD < standing_y(there, map)
        }
        _ => false,
    };

    let solid = match map.get(next) {
        Some(EntityType::Decorator(Decorator { is_solid, .. })) => is_solid,
        _ => nntree.locate_all_at_point(&Loc::new(next)).count() >= MAX_ENTITIES_PER_TILE,
    };

    cliff || (solid && next_floor.is_none()) || is_deep_water(map, next.q, next.r)
}

/// Advance `input` by `dt0` milliseconds in sub-steps of at most
/// [`PHYSICS_TIMESTEP_MS`]. The tile stays `input.position.tile`; the offset
/// may leave it, and the caller re-bases when the world position crosses
/// into another tile.
pub fn calculate_movement(
    input: MovementInput,
    mut dt0: i16,
    map: &Map,
    nntree: &NNTree,
) -> MovementOutput {
    let tile = input.position.tile;
    let px0: Vec3 = map.convert(tile);
    let mut offset = input.position.offset;
    let mut airtime = input.airtime;
    let dir = input.heading.to_world_dir();

    while dt0 > 0 {
        let mut dt = dt0.min(PHYSICS_TIMESTEP_MS);
        dt0 -= dt;

        let world = px0 + offset;
        let here: Qrz = map.convert(world);
        let floor = map.get_by_qr(here.q, here.r).map(|(floor, _)| floor);

        // Over nothing, or more than a level above the floor, a grounded
        // entity starts to fall.
        if airtime.is_none() && floor.map_or(true, |floor| here.z > floor.z + 1) {
            airtime = Some(0);
        }

        if let Some(mut air) = airtime {
            if air > 0 {
                // Ascending: split the sub-step at the apex so the descent
                // starts on the exact millisecond whatever the partition.
                if air < dt {
                    dt0 += dt - air;
                    dt = air;
                }
                air -= dt;
                airtime = Some(air);
                offset.y += dt as f32 * GRAVITY * JUMP_ASCENT_MULTIPLIER;
            } else {
                air = air.saturating_sub(dt);
                airtime = Some(air);
                let dy = -(dt as f32) * GRAVITY;
                let fallen: Qrz = map.convert(Vec3::new(world.x, world.y + dy, world.z));
                match floor {
                    Some(floor) if fallen.z <= floor.z + 1 => {
                        offset.y = standing_y(floor, map) - px0.y;
                        airtime = None;
                    }
                    _ => offset.y += dy,
                }
            }
        }

        if input.moving {
            let step = dir * input.movement_speed * dt as f32;
            let landing: Qrz = map.convert(px0 + offset + Vec3::new(step.x, 0.0, step.y));
            let crossing = (landing.q, landing.r) != (here.q, here.r);
            if !crossing || !is_tile_blocked(floor, landing, px0.y + offset.y, airtime, map, nntree) {
                offset.x += step.x;
                offset.z += step.y;
            }
        }

        let world = px0 + offset;
        let here: Qrz = map.convert(world);
        if let Some((floor, _)) = map.get_by_qr(here.q, here.r) {
            if airtime.is_none() {
                offset.y = surface_y(world.xz(), floor, map) - px0.y;
            } else {
                offset.y = offset.y.max(standing_y(floor, map) - px0.y);
            }
        }
    }

    MovementOutput {
        position: Position::new(tile, offset),
        airtime,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop))
    }

    fn create_test_nntree() -> NNTree {
        NNTree::new_for_test()
    }

    fn flat_ground(map: &Map, radius: i32) {
        let ground = EntityType::Decorator(Decorator { index: 0, is_solid: false });
        for q in -radius..=radius {
            for r in -radius..=radius {
                map.insert(Qrz { q, r, z: 0 }, ground);
            }
        }
    }

    fn walking(heading: Heading, moving: bool) -> MovementInput {
        MovementInput {
            position: Position::at_tile(Qrz { q: 0, r: 0, z: 1 }),
            heading,
            moving,
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
        }
    }

    #[test]
    fn movement_is_deterministic() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let input = MovementInput { airtime: Some(50), ..walking(Heading::from_degrees(90.0), true) };
        assert_eq!(calculate_movement(input, 125, &map, &nntree), calculate_movement(input, 125, &map, &nntree));
    }

    #[test]
    fn a_move_travels_along_its_heading() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        for slot in 0..crate::components::heading::HEADING_SLOTS {
            let heading = Heading::from_slot(slot);
            let out = calculate_movement(walking(heading, true), 100, &map, &nntree);
            let travelled = out.position.offset.xz();
            let expected = heading.to_world_dir() * MOVEMENT_SPEED * 100.0;
            assert!(travelled.abs_diff_eq(expected, 1e-4), "slot {slot}: {travelled} vs {expected}");
        }
    }

    #[test]
    fn standing_still_stays_put() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let start = Position::new(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(0.4, 0.0, -0.3));
        let input = MovementInput { position: start, ..walking(Heading::from_slot(5), false) };
        let out = calculate_movement(input, 500, &map, &nntree);
        assert_eq!(out.position.offset.xz(), start.offset.xz(), "nothing pulls an idle entity anywhere");
    }

    /// The same duration, split into 16 ms and 250 ms slices, reaches the
    /// same place: what lets the client and the server reproduce each other.
    #[test]
    fn result_is_independent_of_partition() {
        let map = create_test_map();
        flat_ground(&map, 6);
        let nntree = create_test_nntree();
        let input = walking(Heading::from_slot(7), true);

        let whole = calculate_movement(input, 1000, &map, &nntree);

        let mut sliced = input;
        let mut left = 1000;
        while left > 0 {
            let dt = left.min(16);
            let out = calculate_movement(sliced, dt, &map, &nntree);
            sliced.position = out.position;
            sliced.airtime = out.airtime;
            left -= dt;
        }

        assert!(whole.position.offset.abs_diff_eq(sliced.position.offset, 1e-3),
            "whole {:?} vs sliced {:?}", whole.position.offset, sliced.position.offset);
        assert_eq!(whole.airtime, sliced.airtime);
    }

    /// A jump reaches the same apex and lands on the same millisecond
    /// however the time is sliced.
    #[test]
    fn jump_is_independent_of_partition() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let input = MovementInput { airtime: Some(JUMP_DURATION_MS), ..walking(Heading::NORTH, false) };

        let whole = calculate_movement(input, 500, &map, &nntree);
        let mut sliced = input;
        for _ in 0..50 {
            let out = calculate_movement(sliced, 10, &map, &nntree);
            sliced.position = out.position;
            sliced.airtime = out.airtime;
        }
        assert!((whole.position.offset.y - sliced.position.offset.y).abs() < 1e-3);
        assert_eq!(whole.airtime, sliced.airtime);
    }

    /// A rise of two levels is a cliff: the step toward it is refused and
    /// the entity keeps its place.
    #[test]
    fn cliffs_block_and_steps_do_not() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let ground = EntityType::Decorator(Decorator { index: 0, is_solid: false });
        let nntree = create_test_nntree();
        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });

        map.insert(Qrz { q: 1, r: 0, z: 1 }, ground);
        let step = calculate_movement(walking(east, true), 250, &map, &nntree);
        assert!(step.position.offset.x > 0.5, "a one-level step is walked: {:?}", step.position.offset);

        map.remove(Qrz { q: 1, r: 0, z: 1 });
        map.insert(Qrz { q: 1, r: 0, z: 2 }, ground);
        let cliff = calculate_movement(walking(east, true), 250, &map, &nntree);
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let where_it_stands: Qrz = map.convert(centre + cliff.position.offset);
        assert_eq!((where_it_stands.q, where_it_stands.r), (0, 0), "held at the cliff: {:?}", cliff.position.offset);
    }

    /// A step of water is waded and deeper water blocks: the next tile
    /// under one step of water is entered, under two it is not, and a dry
    /// tile beside a flooded one is unaffected.
    #[test]
    fn deep_water_blocks_and_shallow_water_is_waded() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        map.set_water(1, 0, Some(1));
        map.set_water(2, 0, Some(2));
        assert!(!is_deep_water(&map, 1, 0), "a step of water is waded");
        assert!(is_deep_water(&map, 2, 0), "two steps of water block");
        assert!(!is_deep_water(&map, -1, 0), "dry ground is dry");

        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let wade = calculate_movement(walking(east, true), 250, &map, &nntree);
        assert!(wade.position.offset.x > 0.5, "walks into a step of water");

        map.set_water(1, 0, Some(2));
        let blocked = calculate_movement(walking(east, true), 250, &map, &nntree);
        assert!(blocked.position.offset.x < wade.position.offset.x, "held back by deep water");
    }

    #[test]
    fn feet_stay_on_the_surface() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let out = calculate_movement(walking(Heading::from_slot(9), true), 300, &map, &nntree);
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let world = centre + out.position.offset;
        let here: Qrz = map.convert(world);
        let (floor, _) = map.get_by_qr(here.q, here.r).unwrap();
        assert!((world.y - surface_y(world.xz(), floor, &map)).abs() < 1e-5);
    }
}
