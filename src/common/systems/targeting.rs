//! Directional Targeting System
//!
//! This module implements heading-based targeting for abilities:
//! - Converts 6-direction heading to 120° facing cone
//! - Determines if targets are within facing direction
//! - Automatic target selection based on proximity and facing
//!
//! # Design
//!
//! The system uses a directional targeting approach where:
//! - Each heading (NE, E, SE, SW, W, NW) maps to a specific angle
//! - A 120° facing cone extends ±60° from the heading angle (covers 3 forward hex faces)
//! - Targets within the cone and nearest to the caster are selected
//!
//! # Heading Angles
//!
//! - NE (Northeast): 30°
//! - E (East): 90°
//! - SE (Southeast): 150°
//! - SW (Southwest): 210°
//! - W (West): 270°
//! - NW (Northwest): 330°

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::common::{
    components::{heading::*, entity_type::*, *},
    plugins::nntree::*,
};

#[cfg(feature = "server")]
use crate::server::components::target_lock::TargetLock;

impl Heading {
    /// Convert heading to angle in degrees
    ///
    /// Returns the angle in degrees (0-360) for the heading direction.
    /// Uses a standard coordinate system where:
    /// - 0° = North
    /// - 90° = East
    /// - 180° = South
    /// - 270° = West
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East
    /// assert_eq!(heading.to_angle(), 90.0);
    /// ```
    pub fn to_angle(&self) -> f32 {
        match (self.q, self.r) {
            (1, -1) => 30.0,   // Northeast
            (1, 0) => 90.0,    // East
            (0, 1) => 150.0,   // Southeast
            (-1, 1) => 210.0,  // Southwest
            (-1, 0) => 270.0,  // West
            (0, -1) => 330.0,  // Northwest
            _ => 0.0,          // Default/invalid heading
        }
    }
}

/// Check if a target location is within the caster's facing cone
///
/// The facing cone is 120° wide (±60° from the heading angle).
/// This covers the three "forward" hex faces in the hex grid.
///
/// # Arguments
///
/// * `caster_heading` - The heading direction of the caster
/// * `caster_loc` - The location of the caster
/// * `target_loc` - The location of the target
///
/// # Returns
///
/// `true` if the target is within the 120° facing cone, `false` otherwise
///
/// # Examples
///
/// ```ignore
/// let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East
/// let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
/// let target = Loc::new(Qrz { q: 1, r: 0, z: 0 }); // Directly east
///
/// assert!(is_in_facing_cone(heading, caster, target));
/// ```
pub fn is_in_facing_cone(
    caster_heading: Heading,
    caster_loc: Loc,
    target_loc: Loc,
) -> bool {
    // Targets on the same tile are always in the facing cone
    // (e.g., multiple enemies standing on the same hex)
    if *caster_loc == *target_loc {
        return true;
    }

    let heading_angle = caster_heading.to_angle();
    let target_angle = angle_between_locs(caster_loc, target_loc);

    // Calculate the angular difference
    let mut delta = (target_angle - heading_angle).abs();

    // Normalize to 0-180 range (shortest angular distance)
    if delta > 180.0 {
        delta = 360.0 - delta;
    }

    // Check if within ±60° (120° cone)
    // This covers the three forward hex faces
    delta <= 60.0
}

/// Calculate the angle in degrees from one location to another
///
/// Returns an angle in the range [0, 360) degrees.
/// Uses the hex grid's natural 6-direction system where:
/// - 30° = Northeast
/// - 90° = East
/// - 150° = Southeast
/// - 210° = Southwest
/// - 270° = West
/// - 330° = Northwest
///
/// # Arguments
///
/// * `from` - Starting location
/// * `to` - Target location
///
/// # Returns
///
/// Angle in degrees from `from` to `to`
fn angle_between_locs(from: Loc, to: Loc) -> f32 {
    // Calculate the difference vector in Qrz coordinates
    let dq = (to.q - from.q) as f32;
    let dr = (to.r - from.r) as f32;

    // Convert to Cartesian coordinates for flat-top hexes
    // Using standard flat-top hex conversion:
    // x = 3/2 * q
    // y = sqrt(3) * (r + q/2)
    let x = 1.5 * dq;
    let y = 1.732050808 * (dr + dq / 2.0); // sqrt(3) ≈ 1.732

    // Calculate angle using atan2 (returns radians, -π to π)
    // atan2(y, x) gives 0° for positive X axis
    let angle_rad = y.atan2(x);

    // Convert to degrees
    let mut angle_deg = angle_rad.to_degrees();

    // The hex grid's orientation relative to Cartesian coordinates means:
    // - East (q=1, r=0) in hex gives atan2 ≈ 30° in Cartesian
    // - We want East to be 90° in our system
    // So we need to add 60° to rotate the coordinate system
    angle_deg += 60.0;

    // Normalize to [0, 360) range
    if angle_deg < 0.0 {
        angle_deg += 360.0;
    } else if angle_deg >= 360.0 {
        angle_deg -= 360.0;
    }

    angle_deg
}

/// Range tiers for distance-based targeting
///
/// Used to categorize targets by distance for tier lock system (Phase 2+)
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RangeTier {
    /// Close range: 1-2 hexes
    Close,
    /// Mid range: 3-6 hexes
    Mid,
    /// Far range: 7+ hexes
    Far,
}

/// Get the range tier for a given distance
///
/// # Arguments
///
/// * `distance` - Distance in hexes (flat_distance)
///
/// # Returns
///
/// The range tier (Close, Mid, or Far)
pub fn get_range_tier(distance: u32) -> RangeTier {
    match distance {
        1..=2 => RangeTier::Close,
        3..=6 => RangeTier::Mid,
        _ => RangeTier::Far,
    }
}

/// Select the best target based on heading, distance, and optional tier lock
///
/// This is the core targeting function called by:
/// - Client target indicator (every frame)
/// - Client ability usage (on key press)
/// - Server ability validation (on Try::UseAbility)
/// - AI targeting (for NPCs)
///
/// # Algorithm
///
/// 1. Query entities within max range (20 hexes) using spatial index
/// 2. Filter to actors (NPCs and players only)
/// 3. Filter to entities within 120° facing cone
/// 4. Apply tier filter if locked (MVP passes None)
/// 5. Select nearest by distance
/// 6. Geometric tiebreaker: if multiple at same distance, pick closest to heading angle
///
/// # Performance
///
/// Designed to run every frame for target indicator:
/// - Uses spatial index (NNTree) for fast proximity queries
/// - Angular checks are cheap (dot product comparisons)
/// - No allocations in hot path
///
/// # Arguments
///
/// * `caster_ent` - Entity of the caster (to skip self)
/// * `caster_loc` - Location of the caster
/// * `caster_heading` - Heading direction of the caster
/// * `tier_lock` - Optional tier lock (None for automatic, Some for manual tier selection)
/// * `nntree` - Spatial index for proximity queries
/// * `get_entity_type` - Function to get EntityType for an entity
///
/// # Returns
///
/// `Some(Entity)` if a valid target is found, `None` otherwise
pub fn select_target<F>(
    caster_ent: Entity,
    caster_loc: Loc,
    caster_heading: Heading,
    tier_lock: Option<RangeTier>,
    nntree: &NNTree,
    get_entity_type: F,
) -> Option<Entity>
where
    F: Fn(Entity) -> Option<EntityType>,
{
    // Query entities within max range (20 hexes)
    // Using locate_within_distance with squared distance
    let max_range_sq = 20 * 20;
    let nearby = nntree.locate_within_distance(caster_loc, max_range_sq);

    // Build list of valid targets with their distances and angles
    let mut candidates: Vec<(Entity, Loc, u32, f32)> = Vec::new();

    for nn in nearby {
        let ent = nn.ent;
        let target_loc = nn.loc;

        // Skip self (by entity, not location - multiple entities can be on same tile!)
        if ent == caster_ent {
            continue;
        }

        // Filter to actors only (players and NPCs)
        let Some(entity_type) = get_entity_type(ent) else {
            continue;
        };
        if !matches!(entity_type, EntityType::Actor(_)) {
            continue;
        }

        // Check if in facing cone (120° cone = ±60°)
        if !is_in_facing_cone(caster_heading, caster_loc, target_loc) {
            continue;
        }

        // Calculate distance (use flat_distance for 2D hex grid)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        // Apply tier filter if locked
        if let Some(required_tier) = tier_lock {
            let target_tier = get_range_tier(distance);
            if target_tier != required_tier {
                continue;
            }
        }

        // Calculate angle to target for tiebreaker
        let target_angle = angle_between_locs(caster_loc, target_loc);

        candidates.push((ent, target_loc, distance, target_angle));
    }

    // No valid targets
    if candidates.is_empty() {
        return None;
    }

    // Sort by distance (nearest first)
    candidates.sort_by_key(|(_, _, dist, _)| *dist);
    let nearest_distance = candidates[0].2;

    // Find all targets at nearest distance
    let nearest_candidates: Vec<_> = candidates
        .iter()
        .filter(|(_, _, dist, _)| *dist == nearest_distance)
        .collect();

    // If only one at nearest distance, return it
    if nearest_candidates.len() == 1 {
        return Some(nearest_candidates[0].0);
    }

    // Geometric tiebreaker: pick target closest to exact heading angle
    let heading_angle = caster_heading.to_angle();
    let mut best_target = nearest_candidates[0].0;
    let mut smallest_delta = f32::MAX;

    for (ent, _, _, target_angle) in nearest_candidates {
        let mut delta = (*target_angle - heading_angle).abs();

        // Normalize to 0-180 range (shortest angular distance)
        if delta > 180.0 {
            delta = 360.0 - delta;
        }

        if delta < smallest_delta {
            smallest_delta = delta;
            best_target = *ent;
        }
    }

    Some(best_target)
}

/// Select the nearest ally based on heading and distance
///
/// Similar to select_target but filters for allies (PlayerControlled) instead of hostiles.
/// Used for ally targeting and ally target frame display.
///
/// # Algorithm
///
/// 1. Query entities within max range (20 hexes) using spatial index
/// 2. Filter to allies (PlayerControlled) only
/// 3. Skip self (by entity)
/// 4. Filter to entities within 120° facing cone
/// 5. Select nearest by distance
/// 6. Geometric tiebreaker: if multiple at same distance, pick closest to heading angle
///
/// # Arguments
///
/// * `caster_ent` - Entity of the caster (to skip self)
/// * `caster_loc` - Location of the caster
/// * `caster_heading` - Heading direction of the caster
/// * `nntree` - Spatial index for proximity queries
/// * `is_player_controlled` - Function to check if entity is player-controlled
///
/// # Returns
///
/// `Some(Entity)` if a valid ally target is found, `None` otherwise
pub fn select_ally_target<F>(
    caster_ent: Entity,
    caster_loc: Loc,
    caster_heading: Heading,
    nntree: &NNTree,
    is_player_controlled: F,
) -> Option<Entity>
where
    F: Fn(Entity) -> bool,
{
    // Query entities within max range (20 hexes)
    let max_range_sq = 20 * 20;
    let nearby = nntree.locate_within_distance(caster_loc, max_range_sq);

    // Build list of valid ally targets with their distances and angles
    let mut candidates: Vec<(Entity, Loc, u32, f32)> = Vec::new();

    for nn in nearby {
        let ent = nn.ent;
        let target_loc = nn.loc;

        // Skip self
        if ent == caster_ent {
            continue;
        }

        // Filter to allies only (PlayerControlled)
        if !is_player_controlled(ent) {
            continue;
        }

        // Check if in facing cone (120° cone = ±60°)
        if !is_in_facing_cone(caster_heading, caster_loc, target_loc) {
            continue;
        }

        // Calculate distance (use flat_distance for 2D hex grid)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        // Calculate angle to target for tiebreaker
        let target_angle = angle_between_locs(caster_loc, target_loc);

        candidates.push((ent, target_loc, distance, target_angle));
    }

    // No valid allies
    if candidates.is_empty() {
        return None;
    }

    // Sort by distance (nearest first)
    candidates.sort_by_key(|(_, _, dist, _)| *dist);
    let nearest_distance = candidates[0].2;

    // Find all allies at nearest distance
    let nearest_candidates: Vec<_> = candidates
        .iter()
        .filter(|(_, _, dist, _)| *dist == nearest_distance)
        .collect();

    // If only one at nearest distance, return it
    if nearest_candidates.len() == 1 {
        return Some(nearest_candidates[0].0);
    }

    // Geometric tiebreaker: pick ally closest to exact heading angle
    let heading_angle = caster_heading.to_angle();
    let mut best_target = nearest_candidates[0].0;
    let mut smallest_delta = f32::MAX;

    for (ent, _, _, target_angle) in nearest_candidates {
        let mut delta = (*target_angle - heading_angle).abs();

        // Normalize to 0-180 range (shortest angular distance)
        if delta > 180.0 {
            delta = 360.0 - delta;
        }

        if delta < smallest_delta {
            smallest_delta = delta;
            best_target = *ent;
        }
    }

    Some(best_target)
}

/// Reactive system that updates Target component when heading or location changes
///
/// This system runs whenever an entity's Heading or Loc changes, automatically
/// recalculating what entity they are facing using select_target().
///
/// Used by:
/// - Players: Target updates as they turn or move
/// - NPCs WITHOUT TargetLock: Target updates reactively based on FOV
///
/// NPCs with TargetLock are excluded - behavior tree targeting (FindOrKeepTarget)
/// is the source of truth for their targets, not reactive FOV targeting.
///
/// # Performance
///
/// Only runs for entities that actually changed (Bevy change detection).
/// No work done if no entities moved or turned.
#[cfg(feature = "server")]
pub fn update_targets_on_change(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut crate::common::components::target::Target),
        (Or<(Changed<Heading>, Changed<Loc>)>, Without<TargetLock>)
    >,
    entity_types: Query<&EntityType>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target) in &mut query {
        // Use select_target to find what this entity is facing
        let new_target = select_target(
            ent,
            *loc,
            *heading,
            None, // No tier lock (automatic targeting)
            &nntree,
            |e| entity_types.get(e).ok().copied(),
        );

        // Update the Target component
        match new_target {
            Some(target_ent) => target.set(target_ent),
            None => target.clear(),
        }
    }
}

/// Client version: reactive targeting without TargetLock filter (client doesn't have TargetLock)
#[cfg(not(feature = "server"))]
pub fn update_targets_on_change(
    mut query: Query<
        (Entity, &Loc, &Heading, &mut crate::common::components::target::Target),
        Or<(Changed<Heading>, Changed<Loc>)>
    >,
    entity_types: Query<&EntityType>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target) in &mut query {
        // Use select_target to find what this entity is facing
        let new_target = select_target(
            ent,
            *loc,
            *heading,
            None, // No tier lock (automatic targeting)
            &nntree,
            |e| entity_types.get(e).ok().copied(),
        );

        // Update the Target component
        match new_target {
            Some(target_ent) => target.set(target_ent),
            None => target.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    // ===== HEADING TO ANGLE CONVERSION TESTS =====

    #[test]
    fn test_heading_to_angle_all_six_directions() {
        let test_cases = vec![
            (Qrz { q: 1, r: -1, z: 0 }, 30.0, "Northeast"),
            (Qrz { q: 1, r: 0, z: 0 }, 90.0, "East"),
            (Qrz { q: 0, r: 1, z: 0 }, 150.0, "Southeast"),
            (Qrz { q: -1, r: 1, z: 0 }, 210.0, "Southwest"),
            (Qrz { q: -1, r: 0, z: 0 }, 270.0, "West"),
            (Qrz { q: 0, r: -1, z: 0 }, 330.0, "Northwest"),
        ];

        for (qrz, expected_angle, direction_name) in test_cases {
            let heading = Heading::new(qrz);
            let angle = heading.to_angle();
            assert_eq!(
                angle, expected_angle,
                "{} should map to {} degrees, got {}",
                direction_name, expected_angle, angle
            );
        }
    }

    #[test]
    fn test_heading_to_angle_default_heading() {
        let heading = Heading::default();
        let angle = heading.to_angle();
        assert_eq!(angle, 0.0, "Default heading should produce 0.0 degrees");
    }

    #[test]
    fn test_heading_to_angle_invalid_heading() {
        let heading = Heading::new(Qrz { q: 2, r: 0, z: 0 });
        let angle = heading.to_angle();
        assert_eq!(angle, 0.0, "Invalid heading should default to 0.0 degrees");
    }

    // ===== FACING CONE TESTS =====

    #[test]
    fn test_facing_cone_target_directly_ahead() {
        // Heading East, target directly east
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: 1, r: 0, z: 0 });

        assert!(
            is_in_facing_cone(heading, caster, target),
            "Target directly in front should be in facing cone"
        );
    }

    #[test]
    fn test_facing_cone_target_at_edge_of_cone() {
        // Heading East (90°), target at ~60° should be within ±30° cone
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East = 90°
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target_ne = Loc::new(Qrz { q: 1, r: -1, z: 0 }); // Northeast neighbor

        assert!(
            is_in_facing_cone(heading, caster, target_ne),
            "Target at edge of cone should be included"
        );
    }

    #[test]
    fn test_facing_cone_target_outside_cone() {
        // Heading East (90°), target to the west should be outside
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: -1, r: 0, z: 0 }); // West

        assert!(
            !is_in_facing_cone(heading, caster, target),
            "Target behind should not be in facing cone"
        );
    }

    #[test]
    fn test_facing_cone_target_at_same_tile() {
        // Target at same location as caster should return true
        // This handles cases where multiple entities occupy the same hex
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        assert!(
            is_in_facing_cone(heading, caster, target),
            "Should include targets on same tile (multiple entities on same hex)"
        );
    }

    #[test]
    fn test_facing_cone_all_six_headings() {
        // Test that each heading correctly identifies targets in front
        let test_cases = vec![
            (Qrz { q: 1, r: -1, z: 0 }, Qrz { q: 1, r: -1, z: 0 }, "Northeast"),
            (Qrz { q: 1, r: 0, z: 0 }, Qrz { q: 1, r: 0, z: 0 }, "East"),
            (Qrz { q: 0, r: 1, z: 0 }, Qrz { q: 0, r: 1, z: 0 }, "Southeast"),
            (Qrz { q: -1, r: 1, z: 0 }, Qrz { q: -1, r: 1, z: 0 }, "Southwest"),
            (Qrz { q: -1, r: 0, z: 0 }, Qrz { q: -1, r: 0, z: 0 }, "West"),
            (Qrz { q: 0, r: -1, z: 0 }, Qrz { q: 0, r: -1, z: 0 }, "Northwest"),
        ];

        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        for (heading_qrz, target_offset, direction_name) in test_cases {
            let heading = Heading::new(heading_qrz);
            let target = Loc::new(*caster + target_offset);

            assert!(
                is_in_facing_cone(heading, caster, target),
                "{}: Target directly ahead should be in cone",
                direction_name
            );
        }
    }

    #[test]
    fn test_facing_cone_perpendicular_targets() {
        // Heading East, targets to the north and south should be outside 120° cone
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East = 90°
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Northwest target (330°) is 120° away from East (90°) - outside ±60° cone
        let target_nw = Loc::new(Qrz { q: 0, r: -1, z: 0 });
        assert!(
            !is_in_facing_cone(heading, caster, target_nw),
            "Target at 120° delta should be outside 120° cone"
        );

        // Southeast target (150°) is 60° away from East (90°) - at edge of ±60° cone
        let target_se = Loc::new(Qrz { q: 0, r: 1, z: 0 });
        assert!(
            is_in_facing_cone(heading, caster, target_se),
            "Target at 60° delta should be at edge of 120° cone (included)"
        );
    }

    #[test]
    fn test_facing_cone_boundary_precision() {
        // Test the 120° cone (±60° from heading)
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East = 90°
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Northeast at 30° is 60° from East heading (90°) - at edge of cone
        let target_ne = Loc::new(Qrz { q: 1, r: -1, z: 0 }); // Northeast at 30°
        assert!(
            is_in_facing_cone(heading, caster, target_ne),
            "Northeast (60° delta) should be at edge of 120° cone"
        );

        // Target at 120° is 30° from East (90°) - well within cone
        let target_120 = Loc::new(Qrz { q: 1, r: 1, z: 0 }); // At 120°
        assert!(
            is_in_facing_cone(heading, caster, target_120),
            "Target at 30° delta should be well within 120° cone"
        );

        // Southwest at 210° is 120° from East (90°) - outside cone
        let target_sw = Loc::new(Qrz { q: -1, r: 1, z: 0 }); // Southwest at 210°
        assert!(
            !is_in_facing_cone(heading, caster, target_sw),
            "Southwest (120° delta) should be outside 120° cone"
        );
    }

    // ===== ANGLE CALCULATION TESTS =====

    #[test]
    fn test_angle_between_locs_cardinal_directions() {
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Test each cardinal direction
        let test_cases = vec![
            (Qrz { q: 1, r: -1, z: 0 }, 30.0, "Northeast"),
            (Qrz { q: 1, r: 0, z: 0 }, 90.0, "East"),
            (Qrz { q: 0, r: 1, z: 0 }, 150.0, "Southeast"),
            (Qrz { q: -1, r: 1, z: 0 }, 210.0, "Southwest"),
            (Qrz { q: -1, r: 0, z: 0 }, 270.0, "West"),
            (Qrz { q: 0, r: -1, z: 0 }, 330.0, "Northwest"),
        ];

        for (target_qrz, expected_angle, direction_name) in test_cases {
            let target = Loc::new(target_qrz);
            let angle = angle_between_locs(origin, target);

            // Allow small floating point error
            let diff = (angle - expected_angle).abs();
            assert!(
                diff < 5.0,
                "{}: Expected angle ~{}, got {} (diff: {})",
                direction_name, expected_angle, angle, diff
            );
        }
    }

    // ===== RANGE TIER TESTS =====

    #[test]
    fn test_get_range_tier_close() {
        assert_eq!(get_range_tier(1), RangeTier::Close);
        assert_eq!(get_range_tier(2), RangeTier::Close);
    }

    #[test]
    fn test_get_range_tier_mid() {
        assert_eq!(get_range_tier(3), RangeTier::Mid);
        assert_eq!(get_range_tier(4), RangeTier::Mid);
        assert_eq!(get_range_tier(5), RangeTier::Mid);
        assert_eq!(get_range_tier(6), RangeTier::Mid);
    }

    #[test]
    fn test_get_range_tier_far() {
        assert_eq!(get_range_tier(7), RangeTier::Far);
        assert_eq!(get_range_tier(10), RangeTier::Far);
        assert_eq!(get_range_tier(20), RangeTier::Far);
        assert_eq!(get_range_tier(100), RangeTier::Far);
    }

    #[test]
    fn test_get_range_tier_boundaries() {
        // Test tier boundaries
        assert_eq!(get_range_tier(2), RangeTier::Close);
        assert_eq!(get_range_tier(3), RangeTier::Mid);
        assert_eq!(get_range_tier(6), RangeTier::Mid);
        assert_eq!(get_range_tier(7), RangeTier::Far);
    }

    // ===== TARGET SELECTION TESTS =====

    // Helper function to create a test world with entities
    fn setup_test_world() -> (World, NNTree) {
        let mut world = World::new();
        let nntree = NNTree::new_for_test();
        (world, nntree)
    }

    // Helper to spawn an actor at a location
    fn spawn_actor(world: &mut World, nntree: &mut NNTree, loc: Loc) -> Entity {
        use crate::common::components::entity_type::actor::*;

        let entity = world.spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(NpcType::WildDog), // Test helper - generic NPC
            }),
            loc,
        )).id();

        nntree.insert(NearestNeighbor::new(entity, loc));
        entity
    }

    // Helper to spawn a decorator (non-targetable)
    fn spawn_decorator(world: &mut World, nntree: &mut NNTree, loc: Loc) -> Entity {
        use crate::common::components::entity_type::decorator::*;

        let entity = world.spawn((
            EntityType::Decorator(Decorator { index: 0, is_solid: true }),
            loc,
        )).id();

        nntree.insert(NearestNeighbor::new(entity, loc));
        entity
    }

    #[test]
    fn test_select_target_single_target_ahead() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn caster
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);

        // Spawn target directly ahead (east)
        let target = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 }));

        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| {
            world.get::<EntityType>(ent).copied()
        });

        assert_eq!(result, Some(target), "Should select the target directly ahead");
    }

    #[test]
    fn test_select_target_no_targets() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| {
            world.get::<EntityType>(ent).copied()
        });

        assert_eq!(result, None, "Should return None when no targets exist");
    }

    #[test]
    fn test_select_target_behind_caster() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn target behind (west)
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: -1, r: 0, z: 0 }));

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| {
            world.get::<EntityType>(ent).copied()
        });

        assert_eq!(result, None, "Should not select target behind caster");
    }

    #[test]
    fn test_select_target_nearest_wins() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn targets at different distances, all in front
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 3, r: 0, z: 0 })); // Far
        let nearest = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Near
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 2, r: 0, z: 0 })); // Mid

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| {
            world.get::<EntityType>(ent).copied()
        });

        assert_eq!(result, Some(nearest), "Should select the nearest target");
    }

    #[test]
    fn test_select_target_geometric_tiebreaker() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East = 90°

        // Spawn two targets at same distance (1 hex away)
        // One directly ahead (east), one at an angle (northeast)
        let directly_ahead = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // East
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: -1, z: 0 })); // Northeast

        // Query removed
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| world.get::<EntityType>(ent).copied());

        assert_eq!(
            result, Some(directly_ahead),
            "Should select target closest to exact heading angle"
        );
    }

    #[test]
    fn test_select_target_ignores_decorators() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn decorator (not targetable) and actor
        spawn_decorator(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Decorator directly ahead
        let actor = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 2, r: 0, z: 0 })); // Actor further away

        // Query removed
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| world.get::<EntityType>(ent).copied());

        assert_eq!(
            result, Some(actor),
            "Should ignore decorators and select actor"
        );
    }

    #[test]
    fn test_select_target_tier_lock_close() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn targets at different tiers
        let close_target = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Distance 1 (Close)
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 5, r: 0, z: 0 })); // Distance 5 (Mid)

        // Query removed
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, Some(RangeTier::Close), &nntree, |ent| world.get::<EntityType>(ent).copied());

        assert_eq!(
            result, Some(close_target),
            "Should only select targets in Close tier when locked"
        );
    }

    #[test]
    fn test_select_target_tier_lock_mid() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn targets at different tiers
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Distance 1 (Close)
        let mid_target = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 4, r: 0, z: 0 })); // Distance 4 (Mid)
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 8, r: 0, z: 0 })); // Distance 8 (Far)

        // Query removed
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, Some(RangeTier::Mid), &nntree, |ent| world.get::<EntityType>(ent).copied());

        assert_eq!(
            result, Some(mid_target),
            "Should only select targets in Mid tier when locked"
        );
    }

    #[test]
    fn test_select_target_tier_lock_no_matches() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn only close targets
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Distance 1 (Close)

        // Query removed
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, Some(RangeTier::Far), &nntree, |ent| world.get::<EntityType>(ent).copied());

        assert_eq!(
            result, None,
            "Should return None when no targets in locked tier"
        );
    }

    #[test]
    fn test_select_target_within_120_degree_cone() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 }); // East = 90°

        // Spawn targets at various angles
        let ne_target = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: -1, z: 0 })); // Northeast (30°) - within cone
        let se_target = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 0, r: 1, z: 0 })); // Southeast (150°) - within cone
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 0, r: -1, z: 0 })); // Northwest (330°) - outside cone

        // Query removed
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        let result = select_target(caster, caster_loc, heading, None, &nntree, |ent| world.get::<EntityType>(ent).copied());

        // Should select one of the targets within the cone (ne_target or se_target)
        assert!(
            result == Some(ne_target) || result == Some(se_target),
            "Should select a target within the 120° cone"
        );
    }
}
