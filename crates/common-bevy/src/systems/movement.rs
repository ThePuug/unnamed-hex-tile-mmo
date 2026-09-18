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

/// How far short of a refused face a walk stops, in world units: a hair, so
/// the position still converts to its own tile and never the refused one. A
/// distance from the face, not per step, so the stop is the same however
/// the time is sliced.
const FACE_MARGIN: f32 = 0.01;

/// Faces one walk may meet. A sub-step is shorter than a tile, so a few
/// crossings and slides cover it; the cap only ends the bounce between the
/// two walls of a concave corner, which makes no progress.
const FACES_PER_WALK: usize = 16;

/// Displacement of a walk of `reach` world units along the unit `heading`
/// from `from`, a ground-plane point in `here` whose floor is `floor`. The
/// walk crosses the faces [`is_tile_blocked`] allows and slides along the
/// ones it refuses: against a face only the heading's component along it is
/// kept, so a graze keeps most of its speed, a head-on push stands, and past
/// the face's end the heading resumes.
fn walk(
    from: Vec2,
    heading: Vec2,
    reach: f32,
    mut here: Qrz,
    mut floor: Option<Qrz>,
    world_y: f32,
    airtime: Option<i16>,
    map: &Map,
    nntree: &NNTree,
) -> Vec2 {
    let mut pos = from;
    let mut left = reach;
    let mut dir = heading;
    // Speed along `dir` as a fraction of full: the cosine of the incidence
    // while sliding, so a slide spends `left` faster than it covers ground.
    let mut rate = 1.0;
    for _ in 0..FACES_PER_WALK {
        let (to_face, next) = map.exit(pos, dir, here);
        let run = left * rate;
        if run <= to_face {
            pos += dir * run;
            break;
        }
        if is_tile_blocked(floor, next, world_y, airtime, map, nntree) {
            let short = (to_face - FACE_MARGIN).max(0.0);
            pos += dir * short;
            left -= short / rate;
            let along = map.face(here, next).0.perp();
            let kept = heading.dot(along);
            if kept.abs() < 1e-6 {
                break;
            }
            dir = along * kept.signum();
            rate = kept.abs();
        } else {
            pos += dir * to_face;
            left -= to_face / rate;
            here = next;
            floor = map.get_by_qr(next.q, next.r).map(|(floor, _)| floor);
            dir = heading;
            rate = 1.0;
        }
    }
    pos - from
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
            let moved = walk(
                (px0 + offset).xz(), dir, input.movement_speed * dt as f32,
                here, floor, px0.y + offset.y, airtime, map, nntree,
            );
            offset.x += moved.x;
            offset.z += moved.y;
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

    fn cliff(map: &Map, q: i32, r: i32) {
        let ground = EntityType::Decorator(Decorator { index: 0, is_solid: false });
        map.insert(Qrz { q, r, z: 2 }, ground);
    }

    /// A wall met at an angle is slid along: the heading's component along
    /// the face is kept and the rest is lost, so the run along the wall grows
    /// with the angle of incidence from nothing head-on, and the wall is
    /// never entered.
    #[test]
    fn a_wall_is_slid_along_by_the_angle_of_incidence() {
        let map = create_test_map();
        flat_ground(&map, 3);
        cliff(&map, 1, 0);
        let nntree = create_test_nntree();
        let here = Qrz { q: 0, r: 0, z: 1 };
        let wall = Qrz { q: 1, r: 0, z: 1 };
        let (normal, mid) = map.face(here, wall);
        let along = normal.perp();
        let centre: Vec3 = map.convert(here);
        let start = mid - normal * 0.1 - centre.xz();

        let run = |slot: u8| {
            let position = Position::new(here, Vec3::new(start.x, 0.0, start.y));
            let input = MovementInput { position, ..walking(Heading::from_slot(slot), true) };
            let out = calculate_movement(input, 50, &map, &nntree);
            let ended: Qrz = map.convert(centre + out.position.offset);
            assert_ne!((ended.q, ended.r), (wall.q, wall.r), "slot {slot} entered the wall");
            let moved = out.position.offset.xz() - start;
            assert!(moved.dot(normal) <= 0.1 + 1e-5, "slot {slot} passed the face");
            moved.dot(along)
        };

        // The face's normal is the slot-8 bearing; slots 3 to 13 press toward it.
        let runs: Vec<f32> = (3..=13).map(run).collect();
        assert!(runs[5].abs() < 1e-4, "head-on stands: {}", runs[5]);
        for pair in runs.windows(2) {
            assert!(pair[0] < pair[1], "the run along the face grows with the incidence: {runs:?}");
        }
    }

    /// A walk into a wall at an angle, along it and round its end reaches
    /// the same place in 16 ms and 125 ms slices, and walks on past it.
    #[test]
    fn a_slide_is_independent_of_partition() {
        let map = create_test_map();
        flat_ground(&map, 6);
        cliff(&map, 1, 0);
        let nntree = create_test_nntree();
        let input = walking(Heading::from_slot(9), true);

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

        let centre: Vec3 = map.convert(input.position.tile);
        let ended: Qrz = map.convert(centre + whole.position.offset);
        assert!(ended.q >= 1 && (ended.q, ended.r) != (1, 0), "walked on past the wall: {ended:?}");
    }

    /// Two walls meeting at a corner hold an entity pushed into it: each
    /// face's slide runs into the other, and the walk ends without progress.
    #[test]
    fn a_concave_corner_holds() {
        let map = create_test_map();
        flat_ground(&map, 3);
        cliff(&map, 1, 0);
        cliff(&map, 0, 1);
        let nntree = create_test_nntree();
        let into_corner = Heading::from_slot(10);
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 1 });

        let first = calculate_movement(walking(into_corner, true), 250, &map, &nntree);
        let input = MovementInput { position: first.position, ..walking(into_corner, true) };
        let more = calculate_movement(input, 250, &map, &nntree);

        let ended: Qrz = map.convert(centre + more.position.offset);
        assert_eq!((ended.q, ended.r), (0, 0), "held in the corner: {:?}", more.position.offset);
        assert!(more.position.offset.xz().abs_diff_eq(first.position.offset.xz(), 1e-4), "no progress once held");
    }
}
