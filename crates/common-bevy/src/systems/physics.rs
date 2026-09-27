use bevy::prelude::*;

use crate::{
    components::{heading::Heading, position::Position, Turn},
    plugins::nntree::*,
    resources::map::*,
    systems::movement,
};

/// Bearings short of its goal an NPC walks at: this near, it walks while it
/// finishes the turn; further, it turns on the spot.
pub const WALK_ARC: u8 = 3;

/// Advance an NPC by `dt` milliseconds as a player's keys would carry it,
/// through `movement::calculate_movement`, the canonical physics: the
/// heading in `turn` steps toward `goal` on the turn clock `turn` carries,
/// no faster than a held turn key, and stops there; the NPC walks forward,
/// when `walk`, only while the heading is within [`WALK_ARC`] of the goal,
/// so running from what it faces costs the turn. Returns the new offset
/// from `position.tile` and the new airborne state.
#[allow(clippy::too_many_arguments)]
pub fn steer(
    position: Position,
    turn: &mut Turn,
    goal: Heading,
    walk: bool,
    airtime: Option<i16>,
    movement_speed: f32,
    dt: i16,
    map: &Map,
    nntree: &NNTree,
) -> (Vec3, Option<i16>) {
    let (mut position, mut airtime, mut left) = (position, airtime, dt);
    while left > 0 {
        let (way, steps) = turn.heading.turn_toward(goal);
        let stepping = steps > 0 && turn.since_step_ms >= movement::TURN_REPEAT_MS;
        // A slice ends before the step after this one, so the heading stops at the goal.
        let slice = match (steps, stepping) {
            (0, _) => left,
            (_, true) => left.min(movement::TURN_REPEAT_MS as i16),
            (_, false) => left.min((movement::TURN_REPEAT_MS - turn.since_step_ms) as i16),
        };
        let facing = steps - stepping as u8;
        let output = movement::calculate_movement(movement::MovementInput {
            position,
            heading: turn.heading,
            moving: walk && facing <= WALK_ARC,
            back: false,
            turn: if stepping { way } else { 0 },
            since_step_ms: turn.since_step_ms,
            airtime,
            movement_speed,
            collides: false,
        }, slice, map, nntree);
        position.offset = output.position.offset;
        airtime = output.airtime;
        turn.heading = output.heading;
        turn.since_step_ms = output.since_step_ms;
        left -= slice;
    }
    (position.offset, airtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;
    use crate::components::entity_type::{decorator::Decorator, EntityType};
    use crate::systems::movement::{JUMP_ASCENT, JUMP_DURATION_MS, MOVEMENT_SPEED};
    use crate::components::heading::HEADING_SLOTS;

    fn create_test_map() -> Map {
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -3..=3 {
            for r in -3..=3 {
                map.insert(Qrz { q, r, z: 0 }, ground);
            }
        }
        map
    }

    fn create_test_nntree() -> NNTree {
        NNTree::new_for_test()
    }

    /// An NPC already facing its goal, rested, for `dt`.
    #[allow(clippy::too_many_arguments)]
    fn apply(position: Position, heading: Heading, walk: bool, airtime: Option<i16>, movement_speed: f32, dt: i16, map: &Map, nntree: &NNTree) -> (Vec3, Option<i16>) {
        let mut turn = Turn { heading, ..Turn::default() };
        steer(position, &mut turn, heading, walk, airtime, movement_speed, dt, map, nntree)
    }

    fn standing() -> Position {
        Position::at_tile(Qrz { q: 0, r: 0, z: 1 })
    }

    fn high_up() -> Position {
        Position::new(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(0.0, 5.0, 0.0))
    }

    /// A fall gathers speed: an older fall drops further over the same
    /// time, and a fall's age counts on below zero.
    #[test]
    fn a_fall_gathers_speed() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let (fresh, _) = apply(high_up(), Heading::NORTH, false, Some(0), MOVEMENT_SPEED, 125, &map, &nntree);
        let (old, airtime) = apply(high_up(), Heading::NORTH, false, Some(-100), MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(fresh.y < 5.0, "a fresh fall drops: {}", fresh.y);
        assert!(old.y < fresh.y, "an older fall drops further: {} vs {}", old.y, fresh.y);
        assert_eq!(airtime, Some(-225));
    }

    #[test]
    fn jump_ascends_and_counts_down() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let (offset, airtime) = apply(standing(), Heading::NORTH, false, Some(JUMP_DURATION_MS), MOVEMENT_SPEED, 125, &map, &nntree);
        let expected = JUMP_ASCENT * 125.0;
        assert!((offset.y - expected).abs() < 0.01, "ascent is JUMP_ASCENT per ms: {} vs {expected}", offset.y);
        assert_eq!(airtime, Some(0));
    }

    #[test]
    fn a_jump_in_progress_is_not_restarted() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let (_, airtime) = apply(high_up(), Heading::NORTH, false, Some(50), MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(airtime.is_some_and(|air| air < 50), "counts on from 50: {airtime:?}");
    }

    #[test]
    fn past_the_apex_the_entity_falls() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let start = Position::new(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(0.0, 2.0, 0.0));
        let (offset, airtime) = apply(start, Heading::NORTH, false, Some(0), MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(airtime.is_some_and(|air| air < 0), "{airtime:?}");
        assert!(offset.y < 2.0);
    }

    #[test]
    fn airtime_always_decrements() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let start = Position::new(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(0.0, 10.0, 0.0));
        for initial in [-500, -100, 0, 50, 125, 200] {
            let (_, airtime) = apply(start, Heading::NORTH, false, Some(initial), MOVEMENT_SPEED, 125, &map, &nntree);
            if let Some(air) = airtime {
                assert!(air < initial, "from {initial} to {air}");
            }
        }
    }

    #[test]
    fn idle_stays_and_moving_travels_the_heading() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let (idle, _) = apply(standing(), Heading::from_degrees(90.0), false, None, MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(idle.xz().length() < 1e-6, "{idle:?}");
        let (moved, _) = apply(standing(), Heading::from_degrees(90.0), true, None, MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(moved.x > 0.0 && moved.z.abs() < 1e-4, "east is +x: {moved:?}");
    }

    #[test]
    fn a_step_is_bounded_by_speed_and_time() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let (moved, _) = apply(standing(), Heading::from_slot(11), true, None, MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(moved.xz().length() <= MOVEMENT_SPEED * 125.0 + 1e-4);
    }

    #[test]
    fn running_from_what_it_faces_costs_the_turn() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let mut turn = Turn::default();
        let (moved, _) = steer(standing(), &mut turn, Heading::NORTH.reversed(), true, None, MOVEMENT_SPEED, 125, &map, &nntree);
        assert!(moved.xz().length() < 1e-6, "turns on the spot: {moved:?}");
        assert_ne!(turn.heading, Heading::NORTH, "and has begun to turn");
    }

    #[test]
    fn an_npc_turns_no_faster_than_a_held_key() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        for dt in [1, 79, 80, 81, 400, 1000] {
            let mut turn = Turn::default();
            steer(standing(), &mut turn, Heading::NORTH.reversed(), false, None, MOVEMENT_SPEED, dt, &map, &nntree);
            let held = movement::calculate_movement(movement::MovementInput {
                position: standing(), heading: Heading::NORTH, moving: false, back: false, turn: 1,
                since_step_ms: movement::TURN_REPEAT_MS, airtime: None, movement_speed: MOVEMENT_SPEED, collides: false,
            }, dt, &map, &nntree);
            let goal_steps = HEADING_SLOTS / 2;
            assert_eq!(turn.heading.slot(), held.heading.slot().min(goal_steps), "after {dt}ms, stopping at the goal");
        }
    }

    #[test]
    fn the_turn_stops_at_the_goal_and_the_walk_follows() {
        let (map, nntree) = (create_test_map(), create_test_nntree());
        let east = Heading::from_degrees(90.0);
        let mut turn = Turn::default();
        let (moved, _) = steer(standing(), &mut turn, east, true, None, MOVEMENT_SPEED, 1000, &map, &nntree);
        assert_eq!(turn.heading, east);
        assert!(Heading::from_world_dir(moved.xz()).is_some_and(|went| went.turn_toward(east).1 <= WALK_ARC), "walked toward the goal: {moved:?}");
    }
}
