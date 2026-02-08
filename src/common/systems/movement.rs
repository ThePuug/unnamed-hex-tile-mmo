//! Pure Movement Calculation Functions
//!
//! This module contains pure functions for calculating movement physics.
//! These functions are deterministic: same inputs always produce same outputs.
//!
//! # Architecture (ADR-019)
//!
//! Movement calculation is separated into pure functions that:
//! 1. Take explicit inputs (no hidden state)
//! 2. Return explicit outputs (no side effects)
//! 3. Are easily unit tested
//!
//! The ECS systems call these pure functions and apply results to components.

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::common::{
    components::{
        heading::{Heading, HERE, THERE},
        position::Position,
    },
    resources::map::Map,
};

// ===== Physics Constants =====

/// Gravity acceleration in world units per millisecond squared
pub const GRAVITY: f32 = 0.005;

/// Jump ascent multiplier - jumping is 5x faster than falling
pub const JUMP_ASCENT_MULTIPLIER: f32 = 5.0;

/// Jump duration in milliseconds
pub const JUMP_DURATION_MS: i16 = 125;

/// Physics update timestep in milliseconds
pub const PHYSICS_TIMESTEP_MS: i16 = 125;

/// Base movement speed in world units per millisecond
pub const MOVEMENT_SPEED: f32 = 0.005;

/// Vertical search range for floor detection (downward)
pub const FLOOR_SEARCH_RANGE_DOWN: i8 = -60;

/// Vertical search offset for floor detection (upward)
pub const FLOOR_SEARCH_OFFSET_UP: i16 = 30;

// ===== Movement Input =====

/// Input state for movement calculation
#[derive(Clone, Copy, Debug)]
pub struct MovementInput {
    /// Current position
    pub position: Position,
    /// Movement heading direction
    pub heading: Heading,
    /// Destination tile (same as position.tile if stationary)
    pub destination: Qrz,
    /// Air time state: Some(positive) = ascending, Some(negative) = falling, None = grounded
    pub airtime: Option<i16>,
    /// Movement speed in world units per millisecond
    pub movement_speed: f32,
}

/// Output from movement calculation
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementOutput {
    /// New position after movement
    pub position: Position,
    /// Updated air time state
    pub airtime: Option<i16>,
}

// ===== Pure Calculation Functions =====

/// Calculate horizontal movement target based on heading and destination.
///
/// # Returns
/// Target offset relative to current tile center.
pub fn calculate_movement_target(
    current_tile: Qrz,
    destination: Qrz,
    heading: Heading,
    map: &Map,
) -> Vec3 {
    let current_center = map.convert(current_tile);

    // Calculate destination with heading offset
    let dest_px = if *heading != Qrz::default() {
        let dest_center: Vec3 = map.convert(destination);
        let dest_heading_neighbor: Vec3 = map.convert(destination + *heading);
        let direction = dest_heading_neighbor - dest_center;
        let heading_offset_xz = (direction * HERE).xz();
        dest_center + Vec3::new(heading_offset_xz.x, 0.0, heading_offset_xz.y)
    } else {
        map.convert(destination)
    };

    // Return target relative to current tile center
    dest_px - current_center
}

/// Calculate new offset position after horizontal movement.
///
/// Uses lerp-based movement toward target at given speed.
///
/// # Arguments
/// * `current_offset` - Current sub-tile offset
/// * `target_offset` - Target offset (relative to current tile)
/// * `speed` - Movement speed in world units per millisecond
/// * `dt` - Delta time in milliseconds
///
/// # Returns
/// New offset position (XZ only, Y unchanged)
pub fn apply_horizontal_movement(
    current_offset: Vec3,
    target_offset: Vec3,
    speed: f32,
    dt: i16,
) -> Vec3 {
    let delta_px = current_offset.distance(target_offset);
    if delta_px < 0.001 {
        return target_offset; // Already at target
    }

    let ratio = 0_f32.max((delta_px - speed * dt as f32) / delta_px);
    let lerp_xz = current_offset.xz().lerp(target_offset.xz(), 1.0 - ratio);

    Vec3::new(lerp_xz.x, current_offset.y, lerp_xz.y)
}

/// Calculate vertical movement (jumping/falling).
///
/// # Arguments
/// * `current_y` - Current Y offset
/// * `airtime` - Current air time state
/// * `dt` - Delta time in milliseconds
///
/// # Returns
/// (new_y, new_airtime)
pub fn apply_vertical_movement(
    current_y: f32,
    airtime: Option<i16>,
    dt: i16,
) -> (f32, Option<i16>) {
    let Some(mut air) = airtime else {
        return (current_y, None);
    };

    let mut new_y = current_y;

    if air > 0 {
        // Ascending
        let ascent_dt = dt.min(air);
        air -= ascent_dt;
        new_y += ascent_dt as f32 * GRAVITY * JUMP_ASCENT_MULTIPLIER;
    } else {
        // Falling
        air -= dt;
        new_y -= dt as f32 * GRAVITY;
    }

    (new_y, Some(air))
}

/// Clamp Y position to terrain floor.
///
/// Ensures entity never clips below terrain.
///
/// # Returns
/// (clamped_y, should_land) where should_land is true if entity landed
pub fn clamp_to_floor(
    current_tile: Qrz,
    offset: Vec3,
    map: &Map,
) -> (f32, bool) {
    let world_pos = map.convert(current_tile) + offset;
    let current_hex: Qrz = map.convert(world_pos);

    let floor = map.find(
        current_hex + Qrz::Z * FLOOR_SEARCH_OFFSET_UP,
        FLOOR_SEARCH_RANGE_DOWN,
    );

    if let Some((floor_qrz, _)) = floor {
        let terrain_y: f32 = map.convert(floor_qrz + Qrz { z: 1 - current_tile.z, ..current_tile }).y;
        if offset.y < terrain_y {
            return (terrain_y, true);
        }
    }

    (offset.y, false)
}

/// Check if destination tile is blocked.
///
/// Returns true if the tile is blocked by:
/// - Elevation difference > 1 (cliff)
/// - Solid obstacle with no valid floor nearby
pub fn is_tile_blocked(
    current_tile: Qrz,
    current_offset: Vec3,
    destination: Qrz,
    airtime: Option<i16>,
    map: &Map,
) -> bool {
    // Get current and destination floor positions
    let current_world = map.convert(current_tile) + current_offset;
    let current_hex: Qrz = map.convert(current_world);

    let current_floor = map.find(
        current_hex + Qrz::Z * FLOOR_SEARCH_OFFSET_UP,
        FLOOR_SEARCH_RANGE_DOWN,
    );
    let dest_floor = map.find(
        destination + Qrz::Z * FLOOR_SEARCH_OFFSET_UP,
        FLOOR_SEARCH_RANGE_DOWN,
    );

    // Check cliff transition (elevation diff > 1 going upward)
    if let (Some((current_qrz, _)), Some((dest_qrz, _))) = (current_floor, dest_floor) {
        let elevation_diff = dest_qrz.z - current_qrz.z;

        if elevation_diff > 1 {
            // Allow if jumping and high enough
            if airtime.is_some() {
                let current_y = map.convert(current_tile).y + current_offset.y;
                let target_floor_y: f32 = map.convert(dest_qrz + Qrz { z: 1 - current_tile.z, ..current_tile }).y;
                if current_y < target_floor_y {
                    return true; // Can't reach
                }
            } else {
                return true; // On ground, can't climb cliff
            }
        }
    }

    false
}

/// Calculate one physics step of movement.
///
/// This is the core pure function that processes a single physics timestep.
/// It handles horizontal movement, vertical movement (jumping/falling),
/// and terrain collision.
///
/// # Arguments
/// * `input` - Movement input state
/// * `dt` - Delta time in milliseconds (typically PHYSICS_TIMESTEP_MS)
/// * `map` - Game map for terrain queries
///
/// # Returns
/// MovementOutput with updated position and airtime
pub fn calculate_movement_step(
    input: MovementInput,
    dt: i16,
    map: &Map,
) -> MovementOutput {
    let mut offset = input.position.offset;
    let mut airtime = input.airtime;
    let tile = input.position.tile;

    // Check if we should start falling (no floor below)
    if airtime.is_none() {
        let world_pos = map.convert(tile) + offset;
        let current_hex: Qrz = map.convert(world_pos);
        let floor = map.find(
            current_hex + Qrz::Z * FLOOR_SEARCH_OFFSET_UP,
            FLOOR_SEARCH_RANGE_DOWN,
        );

        if floor.is_none() {
            airtime = Some(0); // Start falling
        } else if let Some((floor_qrz, _)) = floor {
            let floor_z = floor_qrz.z;
            let current_z: Qrz = map.convert(map.convert(tile) + Vec3::Y * offset.y);
            if current_z.z > floor_z + 1 {
                airtime = Some(0); // Too high above floor, start falling
            }
        }
    }

    // Apply vertical movement
    let (new_y, new_airtime) = apply_vertical_movement(offset.y, airtime, dt);
    offset.y = new_y;
    airtime = new_airtime;

    // Calculate movement target
    let is_stationary = input.destination == tile;
    let target = if is_stationary {
        // Stationary: move toward heading-based position
        calculate_movement_target(tile, tile, input.heading, map)
    } else {
        // Moving: check if destination is blocked
        let blocked = is_tile_blocked(tile, offset, input.destination, airtime, map);
        let effective_dest = if blocked { tile } else { input.destination };

        // Calculate target with HERE/THERE scaling for tile-center targeting
        let target = calculate_movement_target(tile, effective_dest, input.heading, map);

        if *input.heading != Qrz::default() {
            target // Use exact heading-offset position
        } else if blocked {
            target * HERE // Blocked: stay at HERE distance
        } else {
            target * THERE // Moving: allow THERE distance (cross tile)
        }
    };

    // Apply horizontal movement
    offset = apply_horizontal_movement(offset, target, input.movement_speed, dt);

    // Clamp to floor
    let (clamped_y, landed) = clamp_to_floor(tile, offset, map);
    offset.y = clamped_y;
    if landed {
        airtime = None;
    }

    MovementOutput {
        position: Position::new(tile, offset),
        airtime,
    }
}

/// Process multiple physics timesteps.
///
/// Breaks total_dt into PHYSICS_TIMESTEP_MS chunks and processes each.
/// This ensures consistent physics regardless of frame rate.
///
/// # Arguments
/// * `input` - Initial movement input state
/// * `total_dt` - Total delta time in milliseconds
/// * `map` - Game map for terrain queries
///
/// # Returns
/// Final MovementOutput after all timesteps
pub fn calculate_movement(
    mut input: MovementInput,
    mut total_dt: i16,
    map: &Map,
) -> MovementOutput {
    while total_dt > 0 {
        let dt = total_dt.min(PHYSICS_TIMESTEP_MS);
        total_dt -= PHYSICS_TIMESTEP_MS;

        let output = calculate_movement_step(input, dt, map);
        input.position = output.position;
        input.airtime = output.airtime;
    }

    MovementOutput {
        position: input.position,
        airtime: input.airtime,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8))
    }

    // ===== Determinism Tests =====

    #[test]
    fn test_movement_is_deterministic() {
        let map = create_test_map();

        let input = MovementInput {
            position: Position::new(Qrz { q: 0, r: 0, z: 0 }, Vec3::new(0.5, 1.0, 0.3)),
            heading: Heading::new(Qrz { q: 1, r: 0, z: 0 }),
            destination: Qrz { q: 1, r: 0, z: 0 },
            airtime: Some(50),
            movement_speed: MOVEMENT_SPEED,
        };

        let output1 = calculate_movement(input, 125, &map);
        let output2 = calculate_movement(input, 125, &map);

        assert_eq!(output1, output2, "Movement calculation must be deterministic");
    }

    // ===== Horizontal Movement Tests =====

    #[test]
    fn test_horizontal_movement_toward_target() {
        let current = Vec3::ZERO;
        let target = Vec3::new(1.0, 0.0, 0.0);

        let result = apply_horizontal_movement(current, target, MOVEMENT_SPEED, 125);

        assert!(result.x > 0.0, "Should move toward target");
        assert!(result.x < target.x, "Should not overshoot target");
    }

    #[test]
    fn test_horizontal_movement_preserves_y() {
        let current = Vec3::new(0.0, 5.0, 0.0);
        let target = Vec3::new(1.0, 0.0, 0.0);

        let result = apply_horizontal_movement(current, target, MOVEMENT_SPEED, 125);

        assert_eq!(result.y, current.y, "Y should be preserved");
    }

    #[test]
    fn test_stationary_entity_moves_toward_heading() {
        let map = create_test_map();

        let input = MovementInput {
            position: Position::at_tile(Qrz { q: 0, r: 0, z: 0 }),
            heading: Heading::new(Qrz { q: 1, r: 0, z: 0 }), // East
            destination: Qrz { q: 0, r: 0, z: 0 }, // Same tile (stationary)
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
        };

        let output = calculate_movement(input, 125, &map);

        assert!(
            output.position.offset.x > 0.0,
            "Stationary entity with east heading should move in +X direction"
        );
    }

    // ===== Vertical Movement Tests =====

    #[test]
    fn test_jump_ascent() {
        let (new_y, new_airtime) = apply_vertical_movement(0.0, Some(JUMP_DURATION_MS), 125);

        assert!(new_y > 0.0, "Should ascend during jump");
        assert_eq!(new_airtime, Some(0), "Airtime should decrement to 0");
    }

    #[test]
    fn test_fall_descent() {
        let (new_y, new_airtime) = apply_vertical_movement(5.0, Some(-100), 125);

        assert!(new_y < 5.0, "Should descend while falling");
        assert!(new_airtime.unwrap() < -100, "Airtime should continue decrementing");
    }

    #[test]
    fn test_grounded_no_vertical_movement() {
        let (new_y, new_airtime) = apply_vertical_movement(0.0, None, 125);

        assert_eq!(new_y, 0.0, "Grounded entity should not move vertically");
        assert!(new_airtime.is_none(), "Airtime should stay None");
    }

    // ===== Target Calculation Tests =====

    #[test]
    fn test_target_calculation_with_heading() {
        let map = create_test_map();
        let tile = Qrz { q: 0, r: 0, z: 0 };
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        let target = calculate_movement_target(tile, tile, heading, &map);

        // With heading, should offset toward heading direction by HERE
        assert!(target.x > 0.0, "Target should be offset east");
        assert!(target.length() < 1.0, "Target should be within tile");
    }

    #[test]
    fn test_target_calculation_no_heading() {
        let map = create_test_map();
        let tile = Qrz { q: 0, r: 0, z: 0 };
        let heading = Heading::default();

        let target = calculate_movement_target(tile, tile, heading, &map);

        // No heading, stationary = target is tile center (zero offset)
        assert!(
            target.length() < 0.01,
            "Target with no heading should be tile center"
        );
    }

    // ===== Integration Tests =====

    #[test]
    fn test_full_movement_cycle() {
        let map = create_test_map();

        // Start at origin, move east
        let input = MovementInput {
            position: Position::at_tile(Qrz { q: 0, r: 0, z: 0 }),
            heading: Heading::new(Qrz { q: 1, r: 0, z: 0 }),
            destination: Qrz { q: 1, r: 0, z: 0 },
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
        };

        let output = calculate_movement(input, 250, &map);

        // Should have moved in positive X direction
        assert!(
            output.position.offset.x > 0.0,
            "Should move east: {:?}",
            output.position
        );
    }

    #[test]
    fn test_airtime_decrements_each_step() {
        let map = create_test_map();

        let input = MovementInput {
            position: Position::new(Qrz { q: 0, r: 0, z: 5 }, Vec3::new(0.0, 10.0, 0.0)),
            heading: Heading::default(),
            destination: Qrz { q: 0, r: 0, z: 5 },
            airtime: Some(200),
            movement_speed: MOVEMENT_SPEED,
        };

        let output = calculate_movement(input, 125, &map);

        assert!(
            output.airtime.is_some(),
            "Airtime should still be Some"
        );
        assert!(
            output.airtime.unwrap() < 200,
            "Airtime should have decremented"
        );
    }
}
