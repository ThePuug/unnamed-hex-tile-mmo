use bevy::prelude::*;

use crate::{
    components::{heading::Heading, position::Position},
    plugins::nntree::*,
    resources::map::*,
    systems::movement,
};

/// Advance an entity that walks its heading by `dt` milliseconds: the new
/// offset from `position.tile` and the new airborne state. A thin wrapper
/// over `movement::calculate_movement`, the canonical physics, for NPCs,
/// whose heading is set by their behaviour; a player's keys go through
/// `calculate_movement` itself, which turns and backs.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    position: Position,
    heading: Heading,
    moving: bool,
    airtime: Option<i16>,
    movement_speed: f32,
    dt: i16,
    map: &Map,
    nntree: &NNTree,
) -> (Vec3, Option<i16>) {
    let input = movement::MovementInput {
        position, heading, moving, back: false, turn: 0,
        since_step_ms: movement::TURN_REPEAT_MS, airtime, movement_speed,
    };
    let output = movement::calculate_movement(input, dt, map, nntree);
    (output.position.offset, output.airtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;
    use crate::components::entity_type::{decorator::Decorator, EntityType};
    use crate::systems::movement::{JUMP_ASCENT, JUMP_DURATION_MS, MOVEMENT_SPEED};

    fn create_test_map() -> Map {
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        let ground = EntityType::Decorator(Decorator { index: 0, is_solid: false });
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
}
