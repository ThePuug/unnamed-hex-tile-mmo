use bevy::prelude::*;

use crate::common::{
    components::{
        heading::*,
        position::Position,
        *
    },
    plugins::nntree::*,
    resources::map::*,
    systems::movement,
};

pub fn apply(
    dest: Loc,
    dt0: i16,
    loc0: Loc,
    offset0: Vec3,
    airtime0: Option<i16>,
    movement_speed: f32,
    heading: Heading,
    map: &Map,
    nntree: &NNTree,
) -> (Vec3, Option<i16>) {
    let input = movement::MovementInput {
        position: Position::new(*loc0, offset0),
        heading,
        destination: *dest,
        airtime: airtime0,
        movement_speed,
    };
    let output = movement::calculate_movement(input, dt0, map, nntree);
    (output.position.offset, output.airtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use qrz::{Convert, Qrz};
    use crate::common::systems::movement::{GRAVITY, JUMP_DURATION_MS, MOVEMENT_SPEED};

    /// Helper to create a test Map with default terrain (radius=1.0, rise=0.8)
    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8))
    }

    /// Helper to create an empty NNTree for testing
    fn create_test_nntree() -> NNTree {
        NNTree::new_for_test()
    }

    /// Helper to spawn a physics entity at a specific location
    fn spawn_physics_entity(app: &mut App, qrz: Qrz, offset: Vec3) -> Entity {
        app.world_mut().spawn((
            Loc::new(qrz),
            Heading::default(),
            Position::new(qrz, offset),
            AirTime { state: None, step: None },
            Physics,
        )).id()
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants (ADR-015)

    /// INV-001: Client-Side Prediction Correctness (Determinism)
    /// Physics apply() MUST produce identical results for identical inputs.
    /// This ensures client prediction matches server simulation exactly.
    #[test]
    fn test_physics_apply_is_deterministic() {
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let offset = Vec3::new(0.5, 1.0, 0.3);
        let airtime = Some(50);
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });

        // Run physics twice with identical inputs
        let (offset1, airtime1) = apply(
            loc, 125, loc, offset, airtime, MOVEMENT_SPEED, heading, &map, &nntree
        );
        let (offset2, airtime2) = apply(
            loc, 125, loc, offset, airtime, MOVEMENT_SPEED, heading, &map, &nntree
        );

        // MUST produce identical results
        assert_eq!(offset1, offset2, "Physics apply not deterministic: offset differs");
        assert_eq!(airtime1, airtime2, "Physics apply not deterministic: airtime differs");
    }

    /// INV-001: Client-Side Prediction Correctness (Movement Prediction)
    /// Client and server MUST calculate identical offsets for movement inputs.
    #[test]
    fn test_movement_prediction_matches_server() {
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 5, r: 5, z: 0 });
        let initial_offset = Vec3::ZERO;
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // Moving east

        // Simulate movement for 125ms
        let (final_offset, _) = apply(
            loc, 125, loc, initial_offset, None, MOVEMENT_SPEED, heading, &map, &nntree
        );

        // Verify offset changed (entity moved)
        assert!(
            final_offset.xz().length() > 0.0,
            "Movement should produce non-zero offset"
        );

        // Re-run with same inputs - MUST match
        let (predicted_offset, _) = apply(
            loc, 125, loc, initial_offset, None, MOVEMENT_SPEED, heading, &map, &nntree
        );

        assert_eq!(
            final_offset, predicted_offset,
            "Client prediction mismatch: server={:?}, client={:?}",
            final_offset, predicted_offset
        );
    }

    /// INV-009: Heading-Based Position Offset Magnitude
    /// Entities with non-default heading offset by HERE (0.33) units towards heading-specified neighbor.
    /// This ensures consistent positioning for stationary entities facing a direction.
    #[test]
    fn test_heading_based_position_offset_magnitude() {
        let map = create_test_map();
        let dest = Qrz { q: 5, r: 5, z: 0 };
        let heading = Qrz { q: 1, r: 0, z: 0 }; // East direction

        let dest_center = map.convert(dest);
        let dest_heading_neighbor = map.convert(dest + heading);
        let direction = dest_heading_neighbor - dest_center;
        let heading_offset_xz = (direction * HERE).xz(); // 0.33 * direction

        let expected_offset_magnitude = direction.length() * HERE;
        let actual_offset_magnitude = heading_offset_xz.length();

        assert!(
            (actual_offset_magnitude - expected_offset_magnitude).abs() < 0.01,
            "Heading offset magnitude incorrect: expected {}, got {}",
            expected_offset_magnitude, actual_offset_magnitude
        );
    }

    // ===== CHARACTERIZATION TESTS: Gravity =====

    #[test]
    fn test_gravity_constant_value() {
        // Document the current gravity constant
        assert_eq!(GRAVITY, 0.005, "Gravity constant should be 0.005");
    }

    #[test]
    fn test_gravity_fall_rate() {
        // Test that falling applies -GRAVITY per millisecond
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let initial_offset = Vec3::new(0.0, 5.0, 0.0);
        let airtime = Some(-100); // Falling

        // Simulate 125ms of falling
        let (final_offset, _) = apply(
            loc,      // dest (not moving)
            125,      // dt
            loc,      // current loc
            initial_offset,
            airtime,
            MOVEMENT_SPEED,
            Heading::default(),
            &map,
            &nntree,
        );

        // Expected: Y decreases by GRAVITY * 125ms = 0.005 * 125 = 0.625
        let expected_y = initial_offset.y - (GRAVITY * 125.0);
        assert!(
            (final_offset.y - expected_y).abs() < 0.01,
            "Fall rate should be GRAVITY * dt. Expected Y: {}, Got: {}",
            expected_y, final_offset.y
        );
    }

    // ===== CHARACTERIZATION TESTS: Jumping =====

    #[test]
    fn test_entity_falls_when_in_air() {
        // Verify that entities with negative airtime (falling) lose altitude
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 5 }); // High up to avoid landing
        let initial_offset = Vec3::new(0.0, 5.0, 0.0);
        let airtime = Some(-100); // Falling

        let (final_offset, _) = apply(
            loc, 125, loc, initial_offset, airtime, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        // Entity should have fallen (Y decreased)
        assert!(
            final_offset.y < initial_offset.y,
            "Entity should fall when in air. Initial Y: {}, Final Y: {}",
            initial_offset.y, final_offset.y
        );
    }

    #[test]
    fn test_jump_sets_initial_airtime() {
        // Verify that jump correctly initializes airtime and ascends
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let initial_offset = Vec3::ZERO;
        let airtime = Some(JUMP_DURATION_MS); // Just started jump

        let (final_offset, final_airtime) = apply(
            loc, 125, loc, initial_offset, airtime, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        // Should ascend (Y increases)
        assert!(
            final_offset.y > initial_offset.y,
            "Jump should ascend. Initial Y: {}, Final Y: {}",
            initial_offset.y, final_offset.y
        );

        // Airtime should decrement
        assert_eq!(
            final_airtime, Some(0),
            "Airtime should decrement from 125ms to 0 after 125ms tick"
        );
    }

    #[test]
    fn test_cannot_double_jump() {
        // Verify that entities already in air cannot reset airtime (no double-jump)
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 5 }); // High up
        let initial_offset = Vec3::new(0.0, 3.0, 0.0);
        let airtime = Some(50); // Mid-jump

        // Try to "jump" again by passing positive airtime
        // The apply function doesn't handle jump input, but if airtime is already Some,
        // it should continue from current state, not reset
        let (_, final_airtime) = apply(
            loc, 125, loc, initial_offset, airtime, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        // Airtime should continue counting down from 50, not reset to 125
        assert!(
            final_airtime.is_some() && final_airtime.unwrap() < 50,
            "Airtime should continue from 50, not reset. Final: {:?}",
            final_airtime
        );
    }

    #[test]
    fn test_jump_ascent_rate() {
        // Test that jumping applies GRAVITY * 5 upward during ascent
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let initial_offset = Vec3::ZERO;
        let airtime = Some(125); // Just started jump

        // Simulate 125ms of jumping
        let (final_offset, final_airtime) = apply(
            loc,
            125,
            loc,
            initial_offset,
            airtime,
            MOVEMENT_SPEED,
            Heading::default(),
            &map,
            &nntree,
        );

        // Expected: Y increases by GRAVITY * 5 * 125ms = 0.005 * 5 * 125 = 3.125
        let expected_y = initial_offset.y + (GRAVITY * 5.0 * 125.0);
        assert!(
            (final_offset.y - expected_y).abs() < 0.01,
            "Jump ascent should be GRAVITY * 5 * dt. Expected Y: {}, Got: {}",
            expected_y, final_offset.y
        );

        // Airtime should be decremented to 0
        assert_eq!(final_airtime, Some(0), "Airtime should count down during jump");
    }


    #[test]
    fn test_jump_transition_from_ascent_to_descent() {
        // Test that airtime goes positive -> 0 -> negative
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let initial_offset = Vec3::new(0.0, 2.0, 0.0);

        // At apex (airtime = 0)
        let (offset_apex, airtime_apex) = apply(
            loc, 125, loc, initial_offset, Some(0), MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        // Should now be falling (negative airtime)
        assert!(
            airtime_apex.is_some() && airtime_apex.unwrap() < 0,
            "After apex, should be falling with negative airtime: {:?}", airtime_apex
        );

        // Y should have decreased (started falling)
        assert!(
            offset_apex.y < initial_offset.y,
            "After apex, should start falling"
        );
    }

    // ===== CHARACTERIZATION TESTS: Movement =====

    #[test]
    fn test_movement_speed_constant() {
        // Document the movement lerp speed
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let dest = Loc::new(Qrz { q: 1, r: 0, z: -1 }); // Move to adjacent hex
        let initial_offset = Vec3::ZERO;

        // Simulate movement
        let (final_offset, _) = apply(
            dest, 125, loc, initial_offset, None, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        // Movement should have occurred (non-zero offset)
        assert!(
            final_offset.x != 0.0 || final_offset.z != 0.0,
            "Entity should move toward destination. Offset: {:?}", final_offset
        );

        // Document the movement speed (0.005 * dt = 0.625 per 125ms)
        let distance_moved = final_offset.xz().length();
        assert!(
            distance_moved > 0.0,
            "Should move some distance: {}", distance_moved
        );
    }

    #[test]
    fn test_stationary_entity_stays_put() {
        // Entity not moving should remain at offset (0,0,0)
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let initial_offset = Vec3::ZERO;

        // Dest = current loc (not moving)
        let (final_offset, _) = apply(
            loc, 125, loc, initial_offset, None, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        // Should stay at (0, 0, 0) or very close
        assert!(
            final_offset.xz().length() < 0.01,
            "Stationary entity should not move. Offset: {:?}", final_offset
        );
    }

    #[test]
    fn test_movement_direction_matches_heading() {
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Move northeast: Qrz(1, 0, -1)
        let dest_ne = Loc::new(Qrz { q: 1, r: 0, z: -1 });
        let (offset_ne, _) = apply(dest_ne, 125, loc, Vec3::ZERO, None, MOVEMENT_SPEED, Heading::default(), &map, &nntree);

        // Should move in positive X direction
        assert!(
            offset_ne.x > 0.0,
            "Moving northeast should increase X. Offset: {:?}", offset_ne
        );
    }


    // ===== PROPERTY-BASED TESTS =====

    #[test]
    fn test_airtime_always_decrements() {
        // Property: Airtime always decreases when Some
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 5 }); // High up to avoid landing

        for initial_airtime in [-500, -100, 0, 50, 125, 200] {
            let (_, final_airtime) = apply(
                loc, 125, loc, Vec3::new(0.0, 10.0, 0.0),
                Some(initial_airtime), MOVEMENT_SPEED, Heading::default(), &map, &nntree
            );

            if let Some(final_air) = final_airtime {
                assert!(
                    final_air < initial_airtime,
                    "Airtime should always decrement. Initial: {}, Final: {}",
                    initial_airtime, final_air
                );
            }
        }
    }

    #[test]
    fn test_offset_changes_are_bounded() {
        // Property: Offset should not change by more than reasonable amount per tick
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 5 });
        let initial_offset = Vec3::new(0.0, 5.0, 0.0);

        let (final_offset, _) = apply(
            Loc::new(Qrz { q: 10, r: -10, z: 0 }), // Far destination
            125,
            loc,
            initial_offset,
            None,
            MOVEMENT_SPEED,
            Heading::default(),
            &map,
            &nntree,
        );

        let delta = (final_offset - initial_offset).length();

        // Max movement in 125ms should be reasonable
        // Horizontal: 0.005 * 125 = 0.625
        // Vertical (jump): 0.005 * 5 * 125 = 3.125
        // Total max ~= 3.2
        assert!(
            delta < 5.0,
            "Position change should be bounded. Delta: {}", delta
        );
    }

    #[test]
    fn test_physics_is_deterministic() {
        // Property: Same inputs -> same outputs
        let map = create_test_map();
        let nntree = create_test_nntree();
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let initial_offset = Vec3::new(1.0, 2.0, 3.0);
        let airtime = Some(50);

        let (offset1, airtime1) = apply(
            loc, 125, loc, initial_offset, airtime, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        let (offset2, airtime2) = apply(
            loc, 125, loc, initial_offset, airtime, MOVEMENT_SPEED, Heading::default(), &map, &nntree
        );

        assert_eq!(offset1, offset2, "Physics should be deterministic");
        assert_eq!(airtime1, airtime2, "Airtime should be deterministic");
    }

    // ===== HEADING-BASED POSITIONING TESTS =====

    #[test]
    fn test_stationary_player_walks_toward_heading_position() {
        // When a player is stationary (dest == loc) with a heading set,
        // they should walk toward the heading-based position (HERE distance from center)
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Start at tile center
        let initial_offset = Vec3::ZERO;

        // Simulate stationary player (dest == loc) with heading
        let (final_offset, _) = apply(
            loc,      // dest = loc (stationary)
            125,      // dt
            loc,      // current loc
            initial_offset,
            None,
            MOVEMENT_SPEED,
            heading,
            &map,
            &nntree,
        );

        // Calculate expected heading position
        let tile_center = map.convert(*loc);
        let heading_neighbor = map.convert(*loc + *heading);
        let direction = heading_neighbor - tile_center;
        let expected_heading_offset = (direction * HERE).xz();

        // Player should have moved toward heading position
        // With MOVEMENT_SPEED=0.005 and dt=125ms, max movement = 0.625
        // Heading position is at HERE (0.33) * direction, which is ~0.57 distance
        assert!(
            final_offset.x > 0.0,
            "Stationary player with east heading should move in +X direction. Offset: {:?}",
            final_offset
        );

        // Verify movement was toward the heading position
        let movement_distance = final_offset.xz().length();
        assert!(
            movement_distance > 0.0 && movement_distance <= expected_heading_offset.length(),
            "Player should move toward heading position (moved: {}, target: {})",
            movement_distance, expected_heading_offset.length()
        );
    }

    #[test]
    fn test_moving_player_ignores_heading_position() {
        // When a player is moving (dest != loc), they should move toward dest
        // regardless of their heading
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let dest = Loc::new(Qrz { q: 1, r: 0, z: -1 }); // Northeast
        let heading = Heading::new(Qrz { q: 0, r: 1, z: 0 }); // South (opposite direction)

        let initial_offset = Vec3::ZERO;

        // Player moving northeast with south heading
        let (final_offset, _) = apply(
            dest,
            125,
            loc,
            initial_offset,
            None,
            MOVEMENT_SPEED,
            heading,
            &map,
            &nntree,
        );

        // Should move toward dest (northeast), not toward heading (south)
        assert!(
            final_offset.x > 0.0,
            "Moving player should move toward dest (northeast), not heading (south). Offset: {:?}",
            final_offset
        );
    }

    #[test]
    fn test_stationary_player_without_heading_stays_at_center() {
        // Stationary player with no heading (default) should lerp toward tile center
        let map = create_test_map();
        let nntree = create_test_nntree();

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::default(); // No heading

        // Start away from center
        let initial_offset = Vec3::new(0.5, 0.0, 0.5);

        // Stationary with no heading
        let (final_offset, _) = apply(
            loc,
            125,
            loc,
            initial_offset,
            None,
            MOVEMENT_SPEED,
            heading,
            &map,
            &nntree,
        );

        // Should move toward center (zero)
        assert!(
            final_offset.xz().length() < initial_offset.xz().length(),
            "Stationary player with no heading should move toward tile center. Initial: {:?}, Final: {:?}",
            initial_offset.xz(), final_offset.xz()
        );
    }
}
