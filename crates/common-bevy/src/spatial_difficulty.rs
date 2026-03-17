//! # Spatial Difficulty System (ADR-014)
//!
//! Level-based enemy system with:
//! - Distance-based difficulty scaling (base 10, +1 per 100 tiles, max 20)
//! - Directional enemy archetypes (Berserker/Juggernaut/Kiter/Defender)
//! - Attribute distribution per archetype
//! - Dynamic engagement spawning

use qrz::Qrz;
use crate::{components::ActorAttributes, message::AbilityType};

/// Haven location - world origin where players spawn
pub const HAVEN_LOCATION: Qrz = Qrz { q: 0, r: 0, z: 0 };

/// Calculate enemy level based on distance from haven
///
/// Base level 10 near haven, +1 per 100 tiles, capped at 20.
///
/// # Examples
/// ```
/// # use qrz::Qrz;
/// # use common_bevy::spatial_difficulty::*;
/// let spawn = Qrz { q: 5, r: 0, z: 0 };  // 5 tiles from origin
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 10);
///
/// let spawn = Qrz { q: 100, r: 0, z: 0 };  // 100 tiles from origin
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 11);
///
/// let spawn = Qrz { q: 500, r: 0, z: 0 };  // 500 tiles from origin
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 15);
///
/// let spawn = Qrz { q: 1000, r: 0, z: 0 };  // 1000+ tiles
/// assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 20);  // Clamped at 20
/// ```
pub fn calculate_enemy_level(spawn_location: Qrz, haven_location: Qrz) -> u8 {
    let distance = haven_location.flat_distance(&spawn_location) as f32;

    // Base 10, +1 per 100 tiles, clamped to 10-20
    (10.0 + (distance / 100.0)).min(20.0) as u8
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
/// # use common_bevy::spatial_difficulty::*;
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

/// Positioning strategy determines hex preference ordering for each archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositioningStrategy {
    /// Maximize angular spread — Juggernauts surround from all sides
    Surround,
    /// Minimize angular spread — Berserkers cluster on one side
    Cluster,
    /// Hold at 2-3 hex range — Defenders don't compete for adjacent hexes
    Perimeter,
    /// Hold at 3-6 hex range — Kiters orbit at distance
    Orbital,
}

/// Enemy archetypes with distinct combat profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyArchetype {
    Berserker,   // North (45°-135°) - Aggressive melee burst (pure Might)
    Juggernaut,  // East (315°-45°) - Tanky melee pressure (pure Vitality)
    Kiter,       // South (225°-315°) - Ranged harassment (pure Grace)
    Defender,    // West (135°-225°) - Reactive counter-attacks (pure Focus)
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

    /// Get the positioning strategy for this archetype.
    ///
    /// All melee archetypes (Chase behavior) use adjacent strategies.
    /// Perimeter/Orbital are reserved for future ranged archetypes.
    pub fn positioning_strategy(&self) -> PositioningStrategy {
        match self {
            EnemyArchetype::Berserker => PositioningStrategy::Cluster,
            EnemyArchetype::Juggernaut => PositioningStrategy::Surround,
            EnemyArchetype::Defender => PositioningStrategy::Surround,
            EnemyArchetype::Kiter => PositioningStrategy::Orbital,
        }
    }

    /// Get signature ability for this archetype (None = auto-attack only)
    pub fn ability(&self) -> Option<AbilityType> {
        match self {
            EnemyArchetype::Berserker => Some(AbilityType::Lunge),
            EnemyArchetype::Juggernaut => Some(AbilityType::Overpower),
            EnemyArchetype::Kiter => None,  // Kites + ranged auto-attack only
            EnemyArchetype::Defender => Some(AbilityType::Counter),
        }
    }

    /// Get NPC model type for this archetype (ADR-014)
    pub fn npc_type(&self) -> crate::components::entity_type::actor::NpcType {
        use crate::components::entity_type::actor::NpcType;
        match self {
            EnemyArchetype::Berserker => NpcType::WildDog,
            EnemyArchetype::Juggernaut => NpcType::Juggernaut,
            EnemyArchetype::Kiter => NpcType::ForestSprite,
            EnemyArchetype::Defender => NpcType::Defender,
        }
    }

    /// Get Approach for this archetype (ADR-014)
    pub fn approach(&self) -> crate::components::entity_type::actor::Approach {
        use crate::components::entity_type::actor::Approach;
        match self {
            EnemyArchetype::Berserker => Approach::Direct,
            EnemyArchetype::Juggernaut => Approach::Overwhelming,
            EnemyArchetype::Kiter => Approach::Distant,
            EnemyArchetype::Defender => Approach::Patient,
        }
    }

    /// Get Resilience for this archetype (ADR-014)
    pub fn resilience(&self) -> crate::components::entity_type::actor::Resilience {
        use crate::components::entity_type::actor::Resilience;
        match self {
            EnemyArchetype::Berserker => Resilience::Primal,
            EnemyArchetype::Juggernaut => Resilience::Vital,
            EnemyArchetype::Kiter => Resilience::Mental,
            EnemyArchetype::Defender => Resilience::Hardened,
        }
    }
}

/// Which ActorAttributes field to invest in (6 investable fields, shift excluded)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeField {
    MightGraceAxis,
    MightGraceSpectrum,
    VitalityFocusAxis,
    VitalityFocusSpectrum,
    InstinctPresenceAxis,
    InstinctPresenceSpectrum,
}

/// A single allocation target: field + relative weight + axis direction
#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    pub field: AttributeField,
    pub weight: u8,
    /// -1/+1 for axis fields; ignored for spectrum
    pub direction: i8,
}

/// Complete NPC attribute build definition
#[derive(Debug, Clone)]
pub struct NpcBuild {
    pub allocations: &'static [Allocation],
    pub might_grace_shift: i8,
    pub vitality_focus_shift: i8,
    pub instinct_presence_shift: i8,
}

// Single-stat build constants
static BERSERKER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::MightGraceAxis, weight: 1, direction: -1 },
];
static JUGGERNAUT_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::VitalityFocusAxis, weight: 1, direction: -1 },
];
static KITER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::MightGraceAxis, weight: 1, direction: 1 },
];
static DEFENDER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::VitalityFocusAxis, weight: 1, direction: 1 },
];

impl EnemyArchetype {
    /// Get the attribute build for this archetype
    pub fn build(&self) -> NpcBuild {
        match self {
            EnemyArchetype::Berserker => NpcBuild {
                allocations: BERSERKER_BUILD,
                might_grace_shift: 0,
                vitality_focus_shift: 0,
                instinct_presence_shift: 0,
            },
            EnemyArchetype::Juggernaut => NpcBuild {
                allocations: JUGGERNAUT_BUILD,
                might_grace_shift: 0,
                vitality_focus_shift: 0,
                instinct_presence_shift: 0,
            },
            EnemyArchetype::Kiter => NpcBuild {
                allocations: KITER_BUILD,
                might_grace_shift: 0,
                vitality_focus_shift: 0,
                instinct_presence_shift: 0,
            },
            EnemyArchetype::Defender => NpcBuild {
                allocations: DEFENDER_BUILD,
                might_grace_shift: 0,
                vitality_focus_shift: 0,
                instinct_presence_shift: 0,
            },
        }
    }
}

/// Distribute `level` points across allocations using largest-remainder method.
///
/// Stack-only: uses fixed-size arrays (max 6 investable fields).
fn distribute_points(level: u8, allocations: &[Allocation]) -> [u8; 6] {
    let mut result = [0u8; 6];
    let n = allocations.len().min(6);
    if n == 0 || level == 0 {
        return result;
    }

    let total_weight: u16 = allocations[..n].iter().map(|a| a.weight as u16).sum();
    if total_weight == 0 {
        return result;
    }

    // Integer quotients
    let mut sum = 0u8;
    let mut remainders = [0u32; 6]; // scaled fractional remainders
    for i in 0..n {
        let w = allocations[i].weight as u32;
        let base = (level as u32 * w / total_weight as u32) as u8;
        result[i] = base;
        sum += base;
        // Fractional remainder scaled by total_weight to avoid floats
        remainders[i] = (level as u32 * w) % total_weight as u32;
    }

    // Distribute remainder by largest fractional remainder (ties: earlier slot wins)
    let mut leftover = level - sum;
    while leftover > 0 {
        let mut best_idx = 0;
        let mut best_rem = 0;
        for i in 0..n {
            if remainders[i] > best_rem {
                best_rem = remainders[i];
                best_idx = i;
            }
        }
        result[best_idx] += 1;
        remainders[best_idx] = 0; // consumed
        leftover -= 1;
    }

    result
}

/// Calculate ActorAttributes for an enemy based on level and archetype
///
/// Points are distributed proportionally across the archetype's build allocations
/// using largest-remainder allocation. Direction is applied to axis fields;
/// spectrum fields are always positive. Fixed shift values come from the build.
///
/// # Examples
/// ```
/// # use common_bevy::spatial_difficulty::*;
/// let attrs = calculate_enemy_attributes(10, EnemyArchetype::Berserker);
/// // Level 10 Berserker: all 10 points to MightGraceAxis, direction -1
/// assert_eq!(attrs.might_grace_axis(), -10);
/// assert_eq!(attrs.vitality_focus_axis(), 0);
/// assert_eq!(attrs.instinct_presence_axis(), 0);
/// ```
pub fn calculate_enemy_attributes(
    level: u8,
    archetype: EnemyArchetype,
) -> ActorAttributes {
    let build = archetype.build();
    let points = distribute_points(level, build.allocations);

    let mut mg_axis: i8 = 0;
    let mut mg_spectrum: i8 = 0;
    let mut vf_axis: i8 = 0;
    let mut vf_spectrum: i8 = 0;
    let mut ip_axis: i8 = 0;
    let mut ip_spectrum: i8 = 0;

    for (i, alloc) in build.allocations.iter().enumerate().take(6) {
        let p = points[i] as i8;
        match alloc.field {
            AttributeField::MightGraceAxis => mg_axis += p * alloc.direction,
            AttributeField::MightGraceSpectrum => mg_spectrum += p,
            AttributeField::VitalityFocusAxis => vf_axis += p * alloc.direction,
            AttributeField::VitalityFocusSpectrum => vf_spectrum += p,
            AttributeField::InstinctPresenceAxis => ip_axis += p * alloc.direction,
            AttributeField::InstinctPresenceSpectrum => ip_spectrum += p,
        }
    }

    ActorAttributes::new(
        mg_axis,
        mg_spectrum,
        build.might_grace_shift,
        vf_axis,
        vf_spectrum,
        build.vitality_focus_shift,
        ip_axis,
        ip_spectrum,
        build.instinct_presence_shift,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== LEVEL CALCULATION TESTS =====

    #[test]
    fn test_level_calculation_origin() {
        let spawn = HAVEN_LOCATION;
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 10);
    }

    #[test]
    fn test_level_calculation_under_100() {
        let spawn = Qrz { q: 50, r: 0, z: 0 };  // 50 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 10);
    }

    #[test]
    fn test_level_calculation_100_to_199() {
        let spawn = Qrz { q: 100, r: 0, z: 0 };  // 100 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 11);

        let spawn = Qrz { q: 150, r: 0, z: 0 };  // 150 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 11);
    }

    #[test]
    fn test_level_calculation_500_tiles() {
        let spawn = Qrz { q: 500, r: 0, z: 0 };  // 500 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 15);
    }

    #[test]
    fn test_level_calculation_clamped_at_20() {
        let spawn = Qrz { q: 1000, r: 0, z: 0 };  // 1000 tiles away
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 20);

        let spawn = Qrz { q: 2000, r: 0, z: 0 };  // 2000 tiles away (way beyond)
        assert_eq!(calculate_enemy_level(spawn, HAVEN_LOCATION), 20);  // Still clamped
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
        assert_eq!(EnemyArchetype::Berserker.ability(), Some(AbilityType::Lunge));
        assert_eq!(EnemyArchetype::Juggernaut.ability(), Some(AbilityType::Overpower));
        assert_eq!(EnemyArchetype::Kiter.ability(), None);
        assert_eq!(EnemyArchetype::Defender.ability(), Some(AbilityType::Counter));
    }

    // ===== DISTRIBUTE POINTS TESTS =====

    #[test]
    fn test_distribute_single_slot() {
        let allocs = [Allocation { field: AttributeField::MightGraceAxis, weight: 1, direction: -1 }];
        let result = distribute_points(10, &allocs);
        assert_eq!(result[0], 10);
    }

    #[test]
    fn test_distribute_equal_weights() {
        let allocs = [
            Allocation { field: AttributeField::MightGraceAxis, weight: 1, direction: -1 },
            Allocation { field: AttributeField::VitalityFocusAxis, weight: 1, direction: -1 },
        ];
        let result = distribute_points(10, &allocs);
        assert_eq!(result[0], 5);
        assert_eq!(result[1], 5);
    }

    #[test]
    fn test_distribute_odd_level_equal_weights() {
        // 7 points / 2 slots → 3 + 4, remainder goes to first slot
        let allocs = [
            Allocation { field: AttributeField::MightGraceAxis, weight: 1, direction: -1 },
            Allocation { field: AttributeField::VitalityFocusAxis, weight: 1, direction: -1 },
        ];
        let result = distribute_points(7, &allocs);
        assert_eq!(result[0] + result[1], 7);
        // Both have equal remainder, earlier slot wins
        assert_eq!(result[0], 4);
        assert_eq!(result[1], 3);
    }

    #[test]
    fn test_distribute_75_25_split() {
        let allocs = [
            Allocation { field: AttributeField::MightGraceAxis, weight: 3, direction: -1 },
            Allocation { field: AttributeField::VitalityFocusAxis, weight: 1, direction: -1 },
        ];
        let result = distribute_points(10, &allocs);
        // 10 * 3/4 = 7.5 → 7, 10 * 1/4 = 2.5 → 2, remainder 1 → slot 0 (larger remainder)
        assert_eq!(result[0], 8);
        assert_eq!(result[1], 2);
    }

    #[test]
    fn test_distribute_zero_level() {
        let allocs = [Allocation { field: AttributeField::MightGraceAxis, weight: 1, direction: -1 }];
        let result = distribute_points(0, &allocs);
        assert_eq!(result[0], 0);
    }

    #[test]
    fn test_distribute_empty_allocations() {
        let result = distribute_points(10, &[]);
        assert_eq!(result, [0u8; 6]);
    }

    // ===== ATTRIBUTE CALCULATION TESTS =====

    #[test]
    fn test_berserker_level_0() {
        let attrs = calculate_enemy_attributes(0, EnemyArchetype::Berserker);
        assert_eq!(attrs.might_grace_axis(), 0);
        assert_eq!(attrs.vitality_focus_axis(), 0);
        assert_eq!(attrs.instinct_presence_axis(), 0);
    }

    #[test]
    fn test_berserker_single_stat() {
        // Berserker: pure Might (MightGraceAxis, direction -1)
        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Berserker);
        assert_eq!(attrs.might_grace_axis(), -10);
        assert_eq!(attrs.vitality_focus_axis(), 0);
        assert_eq!(attrs.instinct_presence_axis(), 0);
    }

    #[test]
    fn test_juggernaut_single_stat() {
        // Juggernaut: pure Vitality (VitalityFocusAxis, direction -1)
        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Juggernaut);
        assert_eq!(attrs.might_grace_axis(), 0);
        assert_eq!(attrs.vitality_focus_axis(), -10);
        assert_eq!(attrs.instinct_presence_axis(), 0);
    }

    #[test]
    fn test_kiter_single_stat() {
        // Kiter: pure Grace (MightGraceAxis, direction +1)
        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Kiter);
        assert_eq!(attrs.might_grace_axis(), 10);
        assert_eq!(attrs.vitality_focus_axis(), 0);
        assert_eq!(attrs.instinct_presence_axis(), 0);
    }

    #[test]
    fn test_defender_single_stat() {
        // Defender: pure Focus (VitalityFocusAxis, direction +1)
        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Defender);
        assert_eq!(attrs.might_grace_axis(), 0);
        assert_eq!(attrs.vitality_focus_axis(), 10);
        assert_eq!(attrs.instinct_presence_axis(), 0);
    }

    #[test]
    fn test_npc_has_zero_shift() {
        // All current archetypes have 0 shift
        for archetype in [
            EnemyArchetype::Berserker,
            EnemyArchetype::Juggernaut,
            EnemyArchetype::Kiter,
            EnemyArchetype::Defender,
        ] {
            let attrs = calculate_enemy_attributes(10, archetype);
            assert_eq!(attrs.might_grace_shift(), 0);
            assert_eq!(attrs.vitality_focus_shift(), 0);
            assert_eq!(attrs.instinct_presence_shift(), 0);
        }
    }

    #[test]
    fn test_single_stat_no_cunning() {
        // Single-stat builds should produce 0 cunning (no instinct investment)
        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Berserker);
        assert_eq!(attrs.cunning(), 0);

        let attrs = calculate_enemy_attributes(10, EnemyArchetype::Defender);
        assert_eq!(attrs.cunning(), 0);
    }

    #[test]
    fn test_all_points_allocated() {
        // Total absolute axis + spectrum values should equal level for single-stat builds
        for level in [1, 5, 10, 15, 20] {
            let attrs = calculate_enemy_attributes(level, EnemyArchetype::Berserker);
            let total = attrs.might_grace_axis().unsigned_abs()
                + attrs.vitality_focus_axis().unsigned_abs()
                + attrs.instinct_presence_axis().unsigned_abs()
                + attrs.might_grace_spectrum().unsigned_abs()
                + attrs.vitality_focus_spectrum().unsigned_abs()
                + attrs.instinct_presence_spectrum().unsigned_abs();
            assert_eq!(total, level, "level {level}: all points should be allocated");
        }
    }
}
