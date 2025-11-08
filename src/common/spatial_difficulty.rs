//! # Spatial Difficulty System (ADR-014)
//!
//! Level-based enemy system with:
//! - Distance-based difficulty scaling (100 tiles per level, 0-10)
//! - Directional enemy archetypes (Berserker/Juggernaut/Kiter/Defender)
//! - Attribute distribution per archetype
//! - Dynamic engagement spawning

use qrz::Qrz;
use crate::common::{components::ActorAttributes, message::AbilityType};

/// Haven location - world origin where players spawn
pub const HAVEN_LOCATION: Qrz = Qrz { q: 0, r: 0, z: 0 };

/// Calculate enemy level based on distance from haven
///
/// - <100 tiles = level 0
/// - 1000+ tiles = level 10
/// - Linear scaling: level = floor(distance / 100)
///
/// # Examples
/// ```
/// # use qrz::Qrz;
/// # use unnamed_hex_tile_mmo::common::spatial_difficulty::*;
/// let spawn = Qrz { q: 5, r: 0, z: 0 };  // 5 tiles from origin
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 0);
///
/// let spawn = Qrz { q: 50, r: 0, z: -50 };  // ~100 tiles from origin
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 1);
///
/// let spawn = Qrz { q: 100, r: 0, z: 0 };  // 100+ tiles from origin
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 10);  // Clamped at 10
/// ```
pub fn calculate_enemy_level(spawn_location: Qrz, haven_location: Qrz) -> u8 {
    let distance = haven_location.flat_distance(&spawn_location) as f32;

    // Linear scaling: level = distance / 100, clamped to 0-10
    (distance / 100.0).min(10.0) as u8
}

/// Directional zones based on angle from haven
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionalZone {
    North,  // 45° - 135° (top) - Berserkers
    East,   // 315° - 45° (right) - Juggernauts
    South,  // 225° - 315° (bottom) - Kiters
    West,   // 135° - 225° (left) - Defenders
}

/// Get directional zone based on angle from haven to spawn point
///
/// # Examples
/// ```
/// # use qrz::Qrz;
/// # use unnamed_hex_tile_mmo::common::spatial_difficulty::*;
/// let north = Qrz { q: 0, r: -10, z: 0 };
/// assert_eq!(get_directional_zone(north, HAVEN_LOCATION), DirectionalZone::North);
///
/// let east = Qrz { q: 10, r: 0, z: 0 };
/// assert_eq!(get_directional_zone(east, HAVEN_LOCATION), DirectionalZone::East);
/// ```
pub fn get_directional_zone(spawn_location: Qrz, haven_location: Qrz) -> DirectionalZone {
    let delta = spawn_location - haven_location;

    // Convert to angle (using r as y-axis for visual "up", q as x-axis for "right")
    // Hex coordinate system: q increases east, r increases southeast
    // For visual top-down: treat -r as "north" (up), q as "east" (right)
    let angle = f32::atan2(-delta.r as f32, delta.q as f32).to_degrees();

    // Normalize to 0-360
    let angle = if angle < 0.0 { angle + 360.0 } else { angle };

    // Angle ranges (rotated to match hex coordinate system):
    // East (q+, r=0): 0° ± 45° = 315°-45°
    // North (q=0, r-): 90° ± 45° = 45°-135°
    // West (q-, r=0): 180° ± 45° = 135°-225°
    // South (q=0, r+): 270° ± 45° = 225°-315°
    match angle {
        a if a >= 315.0 || a < 45.0 => DirectionalZone::East,
        a if a >= 45.0 && a < 135.0 => DirectionalZone::North,
        a if a >= 135.0 && a < 225.0 => DirectionalZone::West,
        _ => DirectionalZone::South,
    }
}

/// Enemy archetypes with distinct combat profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyArchetype {
    Berserker,   // North (45°-135°) - Aggressive melee burst (Might + Instinct)
    Juggernaut,  // East (315°-45°) - Tanky melee pressure (Vitality + Presence)
    Kiter,       // South (225°-315°) - Ranged harassment (Grace + Focus)
    Defender,    // West (135°-225°) - Reactive counter-attacks (Focus + Instinct)
}

impl EnemyArchetype {
    /// Get archetype from directional zone
    pub fn from_zone(zone: DirectionalZone) -> Self {
        match zone {
            DirectionalZone::North => EnemyArchetype::Berserker,
            DirectionalZone::East => EnemyArchetype::Juggernaut,
            DirectionalZone::South => EnemyArchetype::Kiter,
            DirectionalZone::West => EnemyArchetype::Defender,
        }
    }

    /// Get signature ability for this archetype
    pub fn ability(&self) -> AbilityType {
        match self {
            EnemyArchetype::Berserker => AbilityType::Lunge,
            EnemyArchetype::Juggernaut => AbilityType::Overpower,
            EnemyArchetype::Kiter => AbilityType::Volley,
            EnemyArchetype::Defender => AbilityType::Counter,  // Will be implemented
        }
    }

    /// Get NPC model type for this archetype (ADR-014)
    pub fn npc_type(&self) -> crate::common::components::entity_type::actor::NpcType {
        use crate::common::components::entity_type::actor::NpcType;
        match self {
            EnemyArchetype::Berserker => NpcType::WildDog,
            EnemyArchetype::Juggernaut => NpcType::Juggernaut,
            EnemyArchetype::Kiter => NpcType::ForestSprite,
            EnemyArchetype::Defender => NpcType::Defender,
        }
    }

    /// Get Approach for this archetype (ADR-014)
    pub fn approach(&self) -> crate::common::components::entity_type::actor::Approach {
        use crate::common::components::entity_type::actor::Approach;
        match self {
            EnemyArchetype::Berserker => Approach::Direct,
            EnemyArchetype::Juggernaut => Approach::Overwhelming,
            EnemyArchetype::Kiter => Approach::Distant,
            EnemyArchetype::Defender => Approach::Patient,
        }
    }

    /// Get Resilience for this archetype (ADR-014)
    pub fn resilience(&self) -> crate::common::components::entity_type::actor::Resilience {
        use crate::common::components::entity_type::actor::Resilience;
        match self {
            EnemyArchetype::Berserker => Resilience::Primal,
            EnemyArchetype::Juggernaut => Resilience::Vital,
            EnemyArchetype::Kiter => Resilience::Mental,
            EnemyArchetype::Defender => Resilience::Hardened,
        }
    }
}

/// Axis pairs for attribute distribution
#[derive(Debug, Clone, Copy)]
pub enum AxisPair {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}

impl EnemyArchetype {
    /// Get which axes this archetype invests in
    /// Returns (primary_axis, secondary_axis) where primary gets odd levels
    pub fn primary_axes(&self) -> (AxisPair, AxisPair) {
        match self {
            // Berserker: Might (odd) / Instinct (even)
            EnemyArchetype::Berserker => (AxisPair::MightGrace, AxisPair::InstinctPresence),
            // Juggernaut: Vitality (odd) / Presence (even)
            EnemyArchetype::Juggernaut => (AxisPair::VitalityFocus, AxisPair::InstinctPresence),
            // Kiter: Grace (odd) / Focus (even)
            EnemyArchetype::Kiter => (AxisPair::MightGrace, AxisPair::VitalityFocus),
            // Defender: Focus (odd) / Instinct (even)
            EnemyArchetype::Defender => (AxisPair::VitalityFocus, AxisPair::InstinctPresence),
        }
    }

    /// Get attribute direction on each axis (-1 for left, +1 for right)
    /// Returns (primary_direction, secondary_direction)
    pub fn primary_directions(&self) -> (i8, i8) {
        match self {
            // Berserker: -Might (left), -Instinct (left)
            EnemyArchetype::Berserker => (-1, -1),
            // Juggernaut: -Vitality (left), +Presence (right)
            EnemyArchetype::Juggernaut => (-1, 1),
            // Kiter: +Grace (right), +Focus (right)
            EnemyArchetype::Kiter => (1, 1),
            // Defender: +Focus (right), -Instinct (left)
            EnemyArchetype::Defender => (1, -1),
        }
    }
}

/// Calculate ActorAttributes for an enemy based on level and archetype
///
/// Each level grants 1 attribute point, alternating between two attributes:
/// - Odd levels (1, 3, 5, 7, 9) → primary axis
/// - Even levels (2, 4, 6, 8, 10) → secondary axis
///
/// NPCs have 0 spectrum and 0 shift (fixed axis positions)
///
/// # Examples
/// ```
/// # use unnamed_hex_tile_mmo::common::spatial_difficulty::*;
/// let attrs = calculate_enemy_attributes(5, EnemyArchetype::Berserker);
/// // Level 5 Berserker: 3 points to Might (odd: 1,3,5), 2 points to Instinct (even: 2,4)
/// // Berserker goes left (-1) on both axes
/// // might_grace_axis = -3, instinct_presence_axis = -2
/// ```
pub fn calculate_enemy_attributes(
    level: u8,
    archetype: EnemyArchetype,
) -> ActorAttributes {
    let (axis1, axis2) = archetype.primary_axes();
    let (dir1, dir2) = archetype.primary_directions();

    // Calculate how many points go to each axis
    // Odd levels (1,3,5,7,9) go to axis1: (level+1)/2 = 1,2,3,4,5
    // Even levels (2,4,6,8,10) go to axis2: level/2 = 1,2,3,4,5
    let points_axis1 = ((level + 1) / 2) as i8;
    let points_axis2 = (level / 2) as i8;

    // Apply direction multiplier
    let value_axis1 = points_axis1 * dir1;
    let value_axis2 = points_axis2 * dir2;

    // Assign to correct axis pairs
    let mut might_grace = 0;
    let mut vitality_focus = 0;
    let mut instinct_presence = 0;

    match axis1 {
        AxisPair::MightGrace => might_grace += value_axis1,
        AxisPair::VitalityFocus => vitality_focus += value_axis1,
        AxisPair::InstinctPresence => instinct_presence += value_axis1,
    }

    match axis2 {
        AxisPair::MightGrace => might_grace += value_axis2,
        AxisPair::VitalityFocus => vitality_focus += value_axis2,
        AxisPair::InstinctPresence => instinct_presence += value_axis2,
    }

    ActorAttributes::new(
        might_grace,
        0,  // spectrum = 0 for NPCs
        0,  // shift = 0 for NPCs
        vitality_focus,
        0,
        0,
        instinct_presence,
        0,
        0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== LEVEL CALCULATION TESTS =====

    #[test]
    fn test_level_calculation_origin() {
        let spawn = HAVEN_LOCATION;
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 0);
    }

    #[test]
    fn test_level_calculation_under_100() {
        let spawn = Qrz { q: 50, r: 0, z: 0 };  // 50 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 0);
    }

    #[test]
    fn test_level_calculation_100_to_199() {
        let spawn = Qrz { q: 100, r: 0, z: 0 };  // 100 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 1);

        let spawn = Qrz { q: 150, r: 0, z: 0 };  // 150 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 1);
    }

    #[test]
    fn test_level_calculation_500_tiles() {
        let spawn = Qrz { q: 500, r: 0, z: 0 };  // 500 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 5);
    }

    #[test]
    fn test_level_calculation_clamped_at_10() {
        let spawn = Qrz { q: 1000, r: 0, z: 0 };  // 1000 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 10);

        let spawn = Qrz { q: 2000, r: 0, z: 0 };  // 2000 tiles away (way beyond)
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 10);  // Still clamped
    }

    // ===== DIRECTIONAL ZONE TESTS =====

    #[test]
    fn test_directional_zone_north() {
        // North: -r direction (visual "up")
        let spawn = Qrz { q: 0, r: -10, z: 0 };
        assert_eq!(get_directional_zone(spawn, HAVEN_LOCATION), DirectionalZone::North);
    }

    #[test]
    fn test_directional_zone_east() {
        // East: +q direction (visual "right")
        let spawn = Qrz { q: 10, r: 0, z: 0 };
        assert_eq!(get_directional_zone(spawn, HAVEN_LOCATION), DirectionalZone::East);
    }

    #[test]
    fn test_directional_zone_south() {
        // South: +r direction (visual "down")
        let spawn = Qrz { q: 0, r: 10, z: 0 };
        assert_eq!(get_directional_zone(spawn, HAVEN_LOCATION), DirectionalZone::South);
    }

    #[test]
    fn test_directional_zone_west() {
        // West: -q direction (visual "left")
        let spawn = Qrz { q: -10, r: 0, z: 0 };
        assert_eq!(get_directional_zone(spawn, HAVEN_LOCATION), DirectionalZone::West);
    }

    // ===== ARCHETYPE MAPPING TESTS =====

    #[test]
    fn test_archetype_from_zone() {
        assert_eq!(EnemyArchetype::from_zone(DirectionalZone::North), EnemyArchetype::Berserker);
        assert_eq!(EnemyArchetype::from_zone(DirectionalZone::East), EnemyArchetype::Juggernaut);
        assert_eq!(EnemyArchetype::from_zone(DirectionalZone::South), EnemyArchetype::Kiter);
        assert_eq!(EnemyArchetype::from_zone(DirectionalZone::West), EnemyArchetype::Defender);
    }

    #[test]
    fn test_archetype_abilities() {
        assert_eq!(EnemyArchetype::Berserker.ability(), AbilityType::Lunge);
        assert_eq!(EnemyArchetype::Juggernaut.ability(), AbilityType::Overpower);
        assert_eq!(EnemyArchetype::Kiter.ability(), AbilityType::Volley);
        assert_eq!(EnemyArchetype::Defender.ability(), AbilityType::Counter);
    }

    // ===== ATTRIBUTE CALCULATION TESTS =====

    #[test]
    fn test_berserker_level_0() {
        let attrs = calculate_enemy_attributes(0, EnemyArchetype::Berserker);
        assert_eq!(attrs.might_grace_axis, 0);
        assert_eq!(attrs.instinct_presence_axis, 0);
    }

    #[test]
    fn test_berserker_level_5() {
        // Level 5: odd levels 1,3,5 = 3 points to Might (primary)
        //          even levels 2,4 = 2 points to Instinct (secondary)
        // Berserker goes -1 on both
        let attrs = calculate_enemy_attributes(5, EnemyArchetype::Berserker);
        assert_eq!(attrs.might_grace_axis, -3);
        assert_eq!(attrs.instinct_presence_axis, -2);
        assert_eq!(attrs.vitality_focus_axis, 0);
    }

    #[test]
    fn test_juggernaut_level_5() {
        // Level 5: 3 points to Vitality (primary, -1)
        //          2 points to Presence (secondary, +1)
        let attrs = calculate_enemy_attributes(5, EnemyArchetype::Juggernaut);
        assert_eq!(attrs.vitality_focus_axis, -3);
        assert_eq!(attrs.instinct_presence_axis, 2);  // +2 (right direction)
        assert_eq!(attrs.might_grace_axis, 0);
    }

    #[test]
    fn test_kiter_level_5() {
        // Level 5: 3 points to Grace (primary, +1)
        //          2 points to Focus (secondary, +1)
        let attrs = calculate_enemy_attributes(5, EnemyArchetype::Kiter);
        assert_eq!(attrs.might_grace_axis, 3);  // Grace is +
        assert_eq!(attrs.vitality_focus_axis, 2);  // Focus is +
        assert_eq!(attrs.instinct_presence_axis, 0);
    }

    #[test]
    fn test_defender_level_5() {
        // Level 5: 3 points to Focus (primary, +1)
        //          2 points to Instinct (secondary, -1)
        let attrs = calculate_enemy_attributes(5, EnemyArchetype::Defender);
        assert_eq!(attrs.vitality_focus_axis, 3);  // Focus is +
        assert_eq!(attrs.instinct_presence_axis, -2);  // Instinct is -
        assert_eq!(attrs.might_grace_axis, 0);
    }

    #[test]
    fn test_npc_has_zero_spectrum_shift() {
        // All NPCs should have 0 spectrum and 0 shift
        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Berserker);
        assert_eq!(attrs.might_grace_spectrum, 0);
        assert_eq!(attrs.might_grace_shift, 0);
        assert_eq!(attrs.vitality_focus_spectrum, 0);
        assert_eq!(attrs.vitality_focus_shift, 0);
        assert_eq!(attrs.instinct_presence_spectrum, 0);
        assert_eq!(attrs.instinct_presence_shift, 0);
    }
}
