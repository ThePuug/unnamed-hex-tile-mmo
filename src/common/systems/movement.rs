//! Pure Movement Calculation Functions
//!
//! This module contains the canonical physics implementation as pure functions.
//! `physics::apply` delegates to `calculate_movement` in this module.
//!
//! # Architecture (ADR-019)
//!
//! Movement calculation is separated into pure functions that:
//! 1. Take explicit inputs (no hidden state)
//! 2. Return explicit outputs (no side effects)
//! 3. Are easily unit tested
//!
//! Standalone helpers (`apply_horizontal_movement`, `apply_vertical_movement`, etc.)
//! are decomposed building blocks used in tests and available for future callers.

// Many public helpers/constants are currently only consumed by tests in this
// module and in physics.rs; suppress warnings until additional callers exist.
#![allow(dead_code)]

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::common::{
    components::{
        entity_type::{decorator::*, EntityType},
        heading::{Heading, HERE, THERE},
        keybits::*,
        position::Position,
        Loc,
    },
    plugins::nntree::NNTree,
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

/// Terrain slope following speed (0.0 = no following, 1.0 = instant)
pub const SLOPE_FOLLOW_SPEED: f32 = 0.95;

/// Ledge grab threshold in world units
/// Set to 0.0 to disable ledge grabbing
pub const LEDGE_GRAB_THRESHOLD: f32 = 0.0;

/// Maximum entity count per tile before considering it solid
pub const MAX_ENTITIES_PER_TILE: usize = 7;

// ===== Terrain Helpers =====

/// Compute terrain height at the entity's tile, given the floor Qrz.
/// Extracts the repeated `map.convert(floor_qrz + Qrz { z: 1 - tile.z, ..tile }).y` pattern.
pub fn terrain_y_at(floor_qrz: Qrz, entity_tile: Qrz, map: &Map) -> f32 {
    let adjusted: Vec3 = map.convert(floor_qrz + Qrz { z: 1 - entity_tile.z, ..entity_tile });
    adjusted.y
}

/// Compute terrain height blended between the current tile and the nearest neighbor.
/// Produces a smoothly-varying height as the entity moves between tiles, preventing
/// discrete "stepping" at tile boundaries.
///
/// Blends toward a fixed offset (±0.5 × rise) based on whether the neighbor is higher
/// or lower, matching the visual terrain mesh slopes. The neighbor's actual elevation
/// difference doesn't affect the slope amount - only the direction (up/down).
pub fn blended_terrain_y(world_xz: Vec2, current_hx: Qrz, terrain_y: f32, entity_tile: Qrz, current_floor_qrz: Qrz, map: &Map) -> f32 {
    let tile_center: Vec3 = map.convert(current_hx);
    let offset_xz = world_xz - tile_center.xz();

    if offset_xz.length_squared() < 0.001 {
        return terrain_y;
    }

    // Find the neighbor whose direction best matches the entity's offset from tile center
    let mut best_alignment = 0.0_f32;
    let mut best_neighbor = None;
    for neighbor in current_hx.neighbors() {
        let nc: Vec3 = map.convert(neighbor);
        let to_neighbor = nc.xz() - tile_center.xz();
        let alignment = offset_xz.dot(to_neighbor);
        if alignment > best_alignment {
            best_alignment = alignment;
            best_neighbor = Some((neighbor, to_neighbor));
        }
    }

    let Some((neighbor, to_neighbor)) = best_neighbor else {
        return terrain_y;
    };

    let to_neighbor_len = to_neighbor.length();
    if to_neighbor_len < 0.001 {
        return terrain_y;
    }

    // Blend from 0 at tile center to 0.5 at boundary. Using the full center-to-center
    // distance ensures both tiles agree on the same height at the crossing point:
    // from A's side: A + (B-A)*0.5, from B's side: B + (A-B)*0.5 — both equal (A+B)/2.
    let projection = offset_xz.dot(to_neighbor / to_neighbor_len);
    let blend = (projection / to_neighbor_len).clamp(0.0, 0.5);

    if blend < 0.01 {
        return terrain_y;
    }

    let Some((nf_qrz, _)) = map.find(neighbor + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN) else {
        return terrain_y;
    };

    // Blend toward a full rise offset (will be scaled by blend factor)
    // At boundary (blend = 0.5), this produces rise * 0.5 offset, matching terrain mesh
    let elevation_diff = nf_qrz.z - current_floor_qrz.z;
    let rise = map.rise();
    let target_y = if elevation_diff > 0 {
        // Neighbor is higher - target is one full rise above
        terrain_y + rise
    } else if elevation_diff < 0 {
        // Neighbor is lower - target is one full rise below
        terrain_y - rise
    } else {
        // Same height - no slope
        terrain_y
    };

    terrain_y + (target_y - terrain_y) * blend
}

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

/// Clamp Y position to terrain floor with slope following.
///
/// - Grounded: blends terrain height with nearest non-cliff neighbor for smooth slopes
/// - Airborne: hard clamps against actual floor height
///
/// # Returns
/// (clamped_y, should_land) where should_land is true if entity landed
pub fn clamp_to_floor(
    current_tile: Qrz,
    offset: Vec3,
    airtime: Option<i16>,
    map: &Map,
) -> (f32, bool) {
    let px0: Vec3 = map.convert(current_tile);
    let world_pos = px0 + offset;
    let current_hex: Qrz = map.convert(world_pos);

    let floor = map.find(
        current_hex + Qrz::Z * FLOOR_SEARCH_OFFSET_UP,
        FLOOR_SEARCH_RANGE_DOWN,
    );

    if let Some((floor_qrz, _)) = floor {
        let terrain_y = terrain_y_at(floor_qrz, current_tile, map);

        if airtime.is_none() {
            let slope_y = blended_terrain_y(world_pos.xz(), current_hex, terrain_y, current_tile, floor_qrz, map);
            let mut y = offset.y + (slope_y - offset.y) * SLOPE_FOLLOW_SPEED;
            y = y.max(slope_y);
            return (y, false);
        } else {
            return (offset.y.max(terrain_y), false);
        }
    }

    (offset.y, false)
}

/// Check if the next tile toward a destination is blocked.
///
/// Computes `step_hx` from the entity's world position (not just tile), then
/// `next_hx = step_hx + move_heading` to find the immediate next tile.
///
/// Returns true if the tile is blocked by:
/// - Cliff transition (elevation diff > 1 going upward, unless jumping high enough)
/// - Solid decorator with no valid floor nearby
/// - Entity stacking (>= MAX_ENTITIES_PER_TILE entities)
pub fn is_tile_blocked(
    current_tile: Qrz,
    current_offset: Vec3,
    destination: Qrz,
    airtime: Option<i16>,
    map: &Map,
    nntree: &NNTree,
) -> bool {
    let px0: Vec3 = map.convert(current_tile);
    let step_hx: Qrz = map.convert(px0 + current_offset);
    let floor = map.find(step_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

    // Compute direction toward destination as a move heading
    let rel_px: Vec3 = map.convert(destination) - px0;
    let rel_hx: Qrz = map.convert(rel_px);
    let move_heading = Heading::from(KeyBits::from(Heading::new(rel_hx)));
    let next_hx = step_hx + *move_heading;

    let next_floor = map.find(next_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

    // Check cliff transition (elevation diff > 1 going upward)
    let is_cliff_transition = if let (Some((current_floor_qrz, _)), Some((next_floor_qrz, _))) = (floor, next_floor) {
        let elevation_diff = next_floor_qrz.z - current_floor_qrz.z;

        if elevation_diff > 1 {
            if airtime.is_some() {
                let current_y = px0.y + current_offset.y;
                let target_floor_y = terrain_y_at(next_floor_qrz, current_tile, map);
                current_y + LEDGE_GRAB_THRESHOLD < target_floor_y
            } else {
                true
            }
        } else {
            false
        }
    } else {
        false
    };

    // Check if next tile has a solid obstacle
    let exact_is_solid = match map.get(next_hx) {
        Some(EntityType::Decorator(Decorator { is_solid, .. })) => is_solid,
        _ => nntree.locate_all_at_point(&Loc::new(next_hx)).count() >= MAX_ENTITIES_PER_TILE,
    };

    let is_blocked_by_solid = if exact_is_solid {
        next_floor.is_none()
    } else {
        false
    };

    is_cliff_transition || is_blocked_by_solid
}

/// Process multiple physics timesteps.
///
/// Breaks total_dt into PHYSICS_TIMESTEP_MS chunks and processes each.
/// Uses `while dt0 >= 0` with jump apex splitting to match physics.rs exactly.
///
/// # Arguments
/// * `input` - Initial movement input state
/// * `total_dt` - Total delta time in milliseconds
/// * `map` - Game map for terrain queries
/// * `nntree` - Nearest neighbor tree for entity stacking checks
///
/// # Returns
/// Final MovementOutput after all timesteps
pub fn calculate_movement(
    input: MovementInput,
    mut dt0: i16,
    map: &Map,
    nntree: &NNTree,
) -> MovementOutput {
    let tile = input.position.tile;
    let mut offset = input.position.offset;
    let mut airtime = input.airtime;

    while dt0 >= 0 {
        dt0 -= PHYSICS_TIMESTEP_MS;
        let mut dt = std::cmp::min(PHYSICS_TIMESTEP_MS + dt0, PHYSICS_TIMESTEP_MS);

        let px0: Vec3 = map.convert(tile);
        let step_hx: Qrz = map.convert(px0 + offset);
        let floor = map.find(step_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

        // Check if we should start falling
        if airtime.is_none() {
            if floor.is_none() || map.convert(map.convert(tile) + Vec3::Y * offset.y).z > floor.unwrap().0.z + 1 {
                airtime = Some(0);
            }
        }

        // Vertical movement with jump apex splitting
        if let Some(mut air) = airtime {
            if air > 0 {
                // Ascending — split at apex
                if air < dt {
                    dt0 += dt - air;
                    dt = air;
                }
                air -= dt;
                airtime = Some(air);
                offset.y += dt as f32 * GRAVITY * JUMP_ASCENT_MULTIPLIER;
            } else {
                // Falling
                air -= dt;
                airtime = Some(air);
                let dy = -dt as f32 * GRAVITY;
                if floor.is_none() || map.convert(map.convert(tile) + Vec3::Y * (offset.y + dy)).z > floor.unwrap().0.z + 1 {
                    offset.y += dy;
                } else {
                    offset.y = terrain_y_at(floor.unwrap().0, tile, map);
                    airtime = None;
                }
            }
        }

        // Calculate destination with heading offset
        let dest_px = if *input.heading != Qrz::default() {
            let dest_center: Vec3 = map.convert(input.destination);
            let dest_heading_neighbor: Vec3 = map.convert(input.destination + *input.heading);
            let direction = dest_heading_neighbor - dest_center;
            let heading_offset_xz = (direction * HERE).xz();
            dest_center + Vec3::new(heading_offset_xz.x, 0.0, heading_offset_xz.y)
        } else {
            map.convert(input.destination)
        };

        let rel_px = dest_px - px0;

        // Calculate movement target
        let target_px = if input.destination == tile {
            rel_px
        } else {
            let rel_hx: Qrz = map.convert(rel_px);
            let move_heading = Heading::from(KeyBits::from(Heading::new(rel_hx)));
            let next_hx = step_hx + *move_heading;

            let next_floor = map.find(next_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

            let is_cliff_transition = if let (Some((current_floor_qrz, _)), Some((next_floor_qrz, _))) = (floor, next_floor) {
                let elevation_diff = next_floor_qrz.z - current_floor_qrz.z;
                if elevation_diff > 1 {
                    if airtime.is_some() {
                        let current_y = px0.y + offset.y;
                        let target_floor_y = terrain_y_at(next_floor_qrz, tile, map);
                        current_y + LEDGE_GRAB_THRESHOLD < target_floor_y
                    } else {
                        true
                    }
                } else {
                    false
                }
            } else {
                false
            };

            let exact_is_solid = match map.get(next_hx) {
                Some(EntityType::Decorator(Decorator { is_solid, .. })) => is_solid,
                _ => nntree.locate_all_at_point(&Loc::new(next_hx)).count() >= MAX_ENTITIES_PER_TILE,
            };

            let is_blocked_by_solid = exact_is_solid && next_floor.is_none();
            let is_blocked = is_cliff_transition || is_blocked_by_solid;

            if is_blocked {
                rel_px * HERE
            } else if *input.heading != Qrz::default() {
                rel_px
            } else {
                rel_px * THERE
            }
        };

        // Apply horizontal movement
        let delta_px = offset.distance(target_px);
        let ratio = 0_f32.max((delta_px - input.movement_speed * dt as f32) / delta_px);
        let lerp_xz = offset.xz().lerp(target_px.xz(), 1.0 - ratio);
        offset = Vec3::new(lerp_xz.x, offset.y, lerp_xz.y);

        // Terrain following / floor clamping
        let current_hx: Qrz = map.convert(px0 + offset);
        let current_floor = map.find(current_hx + Qrz::Z * FLOOR_SEARCH_OFFSET_UP, FLOOR_SEARCH_RANGE_DOWN);

        if let Some((floor_qrz, _)) = current_floor {
            let terrain_y = terrain_y_at(floor_qrz, tile, map);

            if airtime.is_none() {
                // Grounded: blend terrain height for smooth slopes.
                // Blends with downward slopes but not upward cliffs (prevents oscillation)
                let slope_y = blended_terrain_y((px0 + offset).xz(), current_hx, terrain_y, tile, floor_qrz, map);
                offset.y += (slope_y - offset.y) * SLOPE_FOLLOW_SPEED;
                offset.y = offset.y.max(slope_y);
            } else {
                // Airborne: hard clamp against raw terrain height
                offset.y = offset.y.max(terrain_y);
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
    use crate::common::systems::physics;

    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8))
    }

    fn create_test_nntree() -> NNTree {
        NNTree::new_for_test()
    }

    // ===== Determinism Tests =====

    #[test]
    fn test_movement_is_deterministic() {
        let map = create_test_map();
        let nntree = create_test_nntree();

        let input = MovementInput {
            position: Position::new(Qrz { q: 0, r: 0, z: 0 }, Vec3::new(0.5, 1.0, 0.3)),
            heading: Heading::new(Qrz { q: 1, r: 0, z: 0 }),
            destination: Qrz { q: 1, r: 0, z: 0 },
            airtime: Some(50),
            movement_speed: MOVEMENT_SPEED,
        };

        let output1 = calculate_movement(input, 125, &map, &nntree);
        let output2 = calculate_movement(input, 125, &map, &nntree);

        assert_eq!(output1, output2, "Movement calculation must be deterministic");
    }

    /// Cross-implementation determinism test:
    /// physics::apply and movement::calculate_movement must produce identical results.
    #[test]
    fn test_movement_matches_physics_apply() {
        let map = create_test_map();
        let nntree = create_test_nntree();

        // Test multiple scenarios
        let test_cases: Vec<(Qrz, Vec3, Qrz, Option<i16>, Heading, i16)> = vec![
            // Stationary, no heading
            (Qrz { q: 0, r: 0, z: 0 }, Vec3::ZERO, Qrz { q: 0, r: 0, z: 0 }, None, Heading::default(), 125),
            // Moving east
            (Qrz { q: 0, r: 0, z: 0 }, Vec3::ZERO, Qrz { q: 1, r: 0, z: 0 }, None, Heading::default(), 125),
            // Moving with heading
            (Qrz { q: 0, r: 0, z: 0 }, Vec3::ZERO, Qrz { q: 1, r: 0, z: 0 }, None, Heading::new(Qrz { q: 1, r: 0, z: 0 }), 125),
            // Jumping
            (Qrz { q: 0, r: 0, z: 0 }, Vec3::ZERO, Qrz { q: 0, r: 0, z: 0 }, Some(JUMP_DURATION_MS), Heading::default(), 125),
            // Falling
            (Qrz { q: 0, r: 0, z: 5 }, Vec3::new(0.0, 5.0, 0.0), Qrz { q: 0, r: 0, z: 5 }, Some(-100), Heading::default(), 125),
            // Stationary with heading
            (Qrz { q: 5, r: 5, z: 0 }, Vec3::ZERO, Qrz { q: 5, r: 5, z: 0 }, None, Heading::new(Qrz { q: 1, r: 0, z: 0 }), 125),
            // Multi-step (250ms)
            (Qrz { q: 0, r: 0, z: 0 }, Vec3::ZERO, Qrz { q: 1, r: 0, z: 0 }, None, Heading::default(), 250),
            // With offset
            (Qrz { q: 0, r: 0, z: 0 }, Vec3::new(0.5, 0.0, 0.3), Qrz { q: 1, r: 0, z: 0 }, None, Heading::default(), 125),
        ];

        for (i, (tile, offset, dest, airtime, heading, dt)) in test_cases.iter().enumerate() {
            let loc = Loc::new(*tile);

            // physics::apply
            let (phys_offset, phys_airtime) = physics::apply(
                Loc::new(*dest), *dt, loc, *offset, *airtime, MOVEMENT_SPEED, *heading, &map, &nntree,
            );

            // movement::calculate_movement
            let input = MovementInput {
                position: Position::new(*tile, *offset),
                heading: *heading,
                destination: *dest,
                airtime: *airtime,
                movement_speed: MOVEMENT_SPEED,
            };
            let output = calculate_movement(input, *dt, &map, &nntree);

            assert_eq!(
                phys_offset, output.position.offset,
                "Case {}: offset mismatch. physics={:?}, movement={:?}",
                i, phys_offset, output.position.offset
            );
            assert_eq!(
                phys_airtime, output.airtime,
                "Case {}: airtime mismatch. physics={:?}, movement={:?}",
                i, phys_airtime, output.airtime
            );
        }
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
        let nntree = create_test_nntree();

        let input = MovementInput {
            position: Position::at_tile(Qrz { q: 0, r: 0, z: 0 }),
            heading: Heading::new(Qrz { q: 1, r: 0, z: 0 }), // East
            destination: Qrz { q: 0, r: 0, z: 0 }, // Same tile (stationary)
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
        };

        let output = calculate_movement(input, 125, &map, &nntree);

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
        let nntree = create_test_nntree();

        // Start at origin, move east
        let input = MovementInput {
            position: Position::at_tile(Qrz { q: 0, r: 0, z: 0 }),
            heading: Heading::new(Qrz { q: 1, r: 0, z: 0 }),
            destination: Qrz { q: 1, r: 0, z: 0 },
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
        };

        let output = calculate_movement(input, 250, &map, &nntree);

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
        let nntree = create_test_nntree();

        let input = MovementInput {
            position: Position::new(Qrz { q: 0, r: 0, z: 5 }, Vec3::new(0.0, 10.0, 0.0)),
            heading: Heading::default(),
            destination: Qrz { q: 0, r: 0, z: 5 },
            airtime: Some(200),
            movement_speed: MOVEMENT_SPEED,
        };

        let output = calculate_movement(input, 125, &map, &nntree);

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
