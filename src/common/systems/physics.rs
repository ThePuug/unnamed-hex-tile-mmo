use std::cmp::min;

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::common::{
    components::{
        entity_type::{decorator::*, *},
        heading::*,
        keybits::*,
        offset::*,
        *
    },
    message::Event,
    plugins::nntree::*,
    resources::{map::*, *}
};

// ===== Physics Constants =====

/// Gravity acceleration in world units per millisecond²
/// Applied as velocity change per tick when falling
const GRAVITY: f32 = 0.005;

/// Jump ascent multiplier - jumping is 5x faster than falling
/// This creates the characteristic jump arc
const JUMP_ASCENT_MULTIPLIER: f32 = 5.0;

/// Jump duration in milliseconds
/// Entity ascends for this duration, then begins falling
const JUMP_DURATION_MS: i16 = 125;

/// Physics update timestep in milliseconds
/// Physics is simulated in discrete 125ms chunks
const PHYSICS_TIMESTEP_MS: i16 = 125;

/// Movement speed in world units per millisecond
/// Controls how quickly entities move horizontally
const MOVEMENT_SPEED: f32 = 0.005;

/// Terrain slope following speed (0.0 = no following, 1.0 = instant)
/// Higher values make entities snap to terrain more quickly
const SLOPE_FOLLOW_SPEED: f32 = 0.95;

/// Ledge grab threshold in world units
/// Set to 0.0 to disable ledge grabbing - entities must be exactly at or above the target floor
const LEDGE_GRAB_THRESHOLD: f32 = 0.0;

/// Vertical search range for floor detection (downward)
/// How many Z levels to search below when finding floor
const FLOOR_SEARCH_RANGE_DOWN: i8 = -60;

/// Vertical search offset for floor detection (upward)
/// Initial Z offset added before searching for floor
const FLOOR_SEARCH_OFFSET_UP: i16 = 30;

/// Maximum entity count per tile before considering it solid
/// Prevents excessive entity stacking
const MAX_ENTITIES_PER_TILE: usize = 7;

/// Position smoothing factor for prev_step updates
/// Higher values = more smoothing (reduces jitter when changing directions)
/// Lower values = more responsive (closer to direct assignment)
/// Range: 0.0 (no smoothing) to 1.0 (maximum smoothing)
const PREV_STEP_SMOOTHING: f32 = 0.5;

pub fn update(
    mut query: Query<(&Loc, &mut Heading, &mut Offset, &mut AirTime, Option<&ActorAttributes>), With<Physics>>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
    nntree: Res<NNTree>,
) {
    for (ent, buffer) in buffers.iter() {
        // Queue invariant: all queues must have at least 1 input
        assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");

        let Ok((&loc, mut heading, mut offset0, mut airtime0, attrs)) = query.get_mut(ent) else { continue; };
        let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(MOVEMENT_SPEED);

        let (mut offset, mut airtime) = (offset0.state, airtime0.state);
        let mut new_heading = *heading;
        for input in buffer.queue.iter().rev() {
            let Event::Input { key_bits, dt, .. } = input else { unreachable!() };
            let heading = Heading::from(*key_bits);
            if *heading != default() {
                new_heading = heading;
            }
            let dest = Loc::new(*heading + *loc);
            if key_bits.is_pressed(KB_JUMP) && airtime.is_none() { airtime = Some(JUMP_DURATION_MS); }
            (offset, airtime) = apply(dest, *dt as i16, loc, offset, airtime, movement_speed, new_heading, &map, &nntree);
        }

        // Update heading component to last non-default heading
        *heading = new_heading;

        // Smooth prev_step transition to reduce jitter when changing directions
        // Instead of snapping prev_step to step, lerp it for smoother interpolation
        offset0.prev_step = offset0.prev_step.lerp(offset0.step, 1.0 - PREV_STEP_SMOOTHING);
        (offset0.step, airtime0.step) = (offset,airtime);
    }
}

/// Updates Loc for entities that have crossed tile boundaries
/// This handles NPCs and other non-player entities that don't have InputQueues
pub fn update_tile_crossings(
    mut query: Query<(Entity, &mut Loc, &Offset), With<Physics>>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
) {
    for (ent, mut loc, offset) in &mut query {
        // Skip entities with input queues (handled by main physics::update)
        if buffers.get(&ent).is_some() {
            continue;
        }

        // Calculate world position and check if it's on a different tile
        let world_pos = map.convert(**loc) + offset.state;
        let new_tile = map.convert(world_pos);

        if new_tile != **loc {
            *loc = Loc::new(new_tile);
        }
    }
}

pub fn apply(
    dest: Loc,
    mut dt0: i16,
    loc0: Loc,
    offset0: Vec3,
    airtime0: Option<i16>,
    movement_speed: f32,
    heading: Heading,
    map: &Map,
    nntree: &NNTree,
) -> (Vec3, Option<i16>) {
    let mut offset0 = offset0;
    let mut airtime0 = airtime0;

    while dt0 >= 0 {
        // step physics forward in PHYSICS_TIMESTEP_MS chunks
        dt0 -= PHYSICS_TIMESTEP_MS;
        let mut dt = min(PHYSICS_TIMESTEP_MS + dt0, PHYSICS_TIMESTEP_MS);

        let px0 = map.convert(*loc0);                                       // current px of loc
        let step_hx = map.convert(px0 + offset0);                           // current offset from loc
        let floor = map.find(step_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);
        
        if airtime0.is_none() {
            if floor.is_none() || map.convert(map.convert(*loc0) + Vec3::Y * offset0.y).z > floor.unwrap().0.z+1 {
                airtime0 = Some(0); 
            }
        }
            
        if let Some(mut airtime) = airtime0 {
            if airtime > 0 {
                // ensure we ascend to the apex
                if airtime < dt {
                    dt0 += dt-airtime;
                    dt = airtime;
                }
                airtime -= dt;
                airtime0 = Some(airtime);
                offset0.y += dt as f32 * GRAVITY * JUMP_ASCENT_MULTIPLIER;
            } else {
                // falling
                airtime -= dt;
                airtime0 = Some(airtime);
                let dy = -dt as f32 * GRAVITY;
                if floor.is_none() || map.convert(map.convert(*loc0) + Vec3::Y * (offset0.y + dy)).z > floor.unwrap().0.z+1 {
                    offset0.y += dy;
                } else {
                    offset0.y = map.convert(floor.unwrap().0 + Qrz { z: 1-loc0.z, ..*loc0 }).y;
                    airtime0 = None;
                }
            }
        }

        // Calculate destination with heading offset
        let dest_px = if *heading != Default::default() {
            // Use heading to offset from tile center
            let dest_center = map.convert(*dest);
            let dest_heading_neighbor = map.convert(*dest + *heading);
            let direction = dest_heading_neighbor - dest_center;
            let heading_offset_xz = (direction * HERE).xz();
            dest_center + Vec3::new(heading_offset_xz.x, 0.0, heading_offset_xz.y)
        } else {
            // No heading - use tile center
            map.convert(*dest)
        };

        let rel_px = dest_px - px0;                                             // destination px relative to current px

        // When at destination (stationary), target is the heading-offset position
        let target_px = if *dest == *loc0 {
            rel_px
        } else {
            // Player is moving - use normal movement logic
            let rel_hx = map.convert(rel_px);                                   // destination tile relative to loc
            let move_heading = Heading::from(KeyBits::from(Heading::new(rel_hx)));   // direction towards destination tile
            let next_hx = step_hx + *move_heading;                                   // next tile towards destination

            // Search for next floor tile
            let next_floor = map.find(next_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

            // Check if trying to walk UP a cliff (elevation diff > 1 going upward)
            // Allow walking off cliffs (downward) - player will fall
            // Now also considers player's current vertical position to allow jumping up small cliffs
            let is_cliff_transition = if let (Some((current_floor_qrz, _)), Some((next_floor_qrz, _))) = (floor, next_floor) {
                let elevation_diff = next_floor_qrz.z - current_floor_qrz.z;

                if elevation_diff > 1 {
                    // Only allow traversal if player is jumping AND high enough
                    if airtime0.is_some() {
                        // Calculate actual world Y positions
                        let current_y = map.convert(*loc0).y + offset0.y;
                        let target_floor_y = map.convert(next_floor_qrz + Qrz { z: 1-loc0.z, ..*loc0 }).y;

                        // Block if player's current Y position cannot reach the target floor
                        // Allow a small threshold for ledge grabbing
                        current_y + LEDGE_GRAB_THRESHOLD < target_floor_y
                    } else {
                        // On ground - block all cliff traversal
                        true
                    }
                } else {
                    false  // Not a cliff or downward - allow movement
                }
            } else {
                false  // Can't determine elevation, allow movement
            };

            // Check if next tile has a solid obstacle
            let exact_is_solid = match map.get(next_hx) {
                Some(EntityType::Decorator(Decorator{is_solid, .. })) => *is_solid,
                _ => nntree.locate_all_at_point(&Loc::new(next_hx)).count() >= MAX_ENTITIES_PER_TILE
            };

            let is_blocked_by_solid = if exact_is_solid {
                // If solid, check if there's a valid floor nearby
                next_floor.is_none()
            } else {
                false
            };

            let is_blocked = is_cliff_transition || is_blocked_by_solid;

            // With heading offset, target exact position (no scaling)
            // Without heading, use HERE/THERE scaling for tile-center targeting
            if *heading != Default::default() {
                rel_px  // Target exact heading-offset position
            } else {
                // Legacy tile-center targeting: scale by HERE/THERE
                if is_blocked { rel_px * HERE } else { rel_px * THERE }
            }
        };

        let delta_px = offset0.distance(target_px);
        let ratio = 0_f32.max((delta_px - movement_speed * dt as f32) / delta_px);
        let lerp_xz = offset0.xz().lerp(target_px.xz(), 1. - ratio);
        offset0 = Vec3::new(lerp_xz.x, offset0.y, lerp_xz.y);
        
        // IMPORTANT: Always clamp Y position to at least terrain height + 1.
        // This prevents entities from ever clipping below the terrain surface.
        // DO NOT REVERT - this is intentional behavior for both grounded and airborne entities.
        let current_hx = map.convert(px0 + offset0);
        let current_floor = map.find(current_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

        if let Some((floor_qrz, _)) = current_floor {
            let terrain_y = map.convert(floor_qrz + Qrz { z: 1-loc0.z, ..*loc0 }).y;
            // Always enforce minimum height above terrain (no interpolation, direct clamp)
            offset0.y = offset0.y.max(terrain_y);
        }
    }

    (offset0, airtime0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use qrz::Qrz;

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
            Offset { state: offset, step: offset, prev_step: offset, interp_elapsed: 0.0, interp_duration: 0.0 },
            AirTime { state: None, step: None },
            Physics,
        )).id()
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants (ADR-015)

    /// INV-002: InputQueue Never Empty
    /// All InputQueues MUST contain ≥1 input at all times.
    /// Empty queue causes panic in physics system.
    #[test]
    #[should_panic(expected = "Queue invariant violation")]
    fn test_physics_panics_on_empty_queue() {
        use crate::common::resources::InputQueues;

        let mut app = App::new();
        app.insert_resource(create_test_map());
        app.insert_resource(create_test_nntree());

        let entity = spawn_physics_entity(&mut app, Qrz { q: 0, r: 0, z: 0 }, Vec3::ZERO);

        // Create empty queue (violates invariant)
        let mut queues = InputQueues::default();
        let queue = crate::common::resources::InputQueue {
            queue: std::collections::VecDeque::new(), // EMPTY - violates invariant
        };
        queues.insert(entity, queue);
        app.insert_resource(queues);

        // Run physics update - should panic
        app.add_systems(bevy::app::Update, update);
        app.update();
    }

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
