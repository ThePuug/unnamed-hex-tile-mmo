//! Where the haven stands, and the enemy archetypes: what each is drawn
//! as, how it takes its place round a target, its signature ability, and
//! how an NPC of one spends the points its level gives it.

use qrz::Qrz;
use crate::{components::ActorAttributes, message::AbilityType};

/// Haven location, in hex coordinates.
///
/// Sited on the brink of a belt's plateau on the home continent: the
/// front falls away to a basin plain some 560 z-levels below over the next
/// 300 tiles in the -r direction, with the next belt rising beyond it, the
/// plateau top lies a few levels up behind, and no water stands within six
/// hundred tiles. The z is a placeholder — the server resolves the real one
/// from the terrain at startup, because elevation is generated, not
/// authored.
pub const HAVEN_LOCATION: Qrz = Qrz { q: 104289, r: -4677, z: 0 };

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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EnemyArchetype {
    #[default]
    Berserker,   // Highland - Aggressive melee burst (pure Might)
    Juggernaut,  // Foothills - Tanky melee pressure (pure Vitality)
    Kiter,       // Inland (flat) - Ranged harassment (pure Agility)
    Defender,    // Coast - Reactive counter-attacks (pure Resolve)
    Skirmisher,  // Evasive - dodges the blows aimed at it (pure Instinct)
    Ambusher,   // Ambushing - stuns and strikes from behind (pure Discipline)
}

impl EnemyArchetype {
    /// Get the positioning strategy for this archetype.

    /// All melee archetypes (Chase behavior) use adjacent strategies.
    /// Perimeter/Orbital are reserved for future ranged archetypes.
    pub fn positioning_strategy(&self) -> PositioningStrategy {
        match self {
            EnemyArchetype::Berserker => PositioningStrategy::Cluster,
            EnemyArchetype::Juggernaut => PositioningStrategy::Surround,
            EnemyArchetype::Defender => PositioningStrategy::Surround,
            EnemyArchetype::Skirmisher => PositioningStrategy::Surround,
            EnemyArchetype::Ambusher => PositioningStrategy::Surround,
            EnemyArchetype::Kiter => PositioningStrategy::Orbital,
        }
    }

    /// Get signature ability for this archetype (None = auto-attack only)
    pub fn ability(&self) -> Option<AbilityType> {
        match self {
            EnemyArchetype::Berserker => Some(AbilityType::Lunge),
            EnemyArchetype::Juggernaut => Some(AbilityType::Rattle),
            EnemyArchetype::Kiter => Some(AbilityType::Volley),
            EnemyArchetype::Defender => Some(AbilityType::Counter),
            EnemyArchetype::Skirmisher => Some(AbilityType::Disengage),
            EnemyArchetype::Ambusher => Some(AbilityType::Flank),
        }
    }

    /// Get NPC model type for this archetype
    pub fn npc_type(&self) -> crate::components::entity_type::actor::NpcType {
        use crate::components::entity_type::actor::NpcType;
        match self {
            EnemyArchetype::Berserker => NpcType::WildDog,
            EnemyArchetype::Juggernaut => NpcType::Juggernaut,
            EnemyArchetype::Kiter => NpcType::ForestSprite,
            EnemyArchetype::Defender => NpcType::Defender,
            EnemyArchetype::Skirmisher => NpcType::Skirmisher,
            EnemyArchetype::Ambusher => NpcType::Ambusher,
        }
    }

    /// The archetype an NPC of `npc_type` is: the inverse of [`npc_type`](Self::npc_type)
    pub fn of_npc(npc_type: crate::components::entity_type::actor::NpcType) -> Self {
        use crate::components::entity_type::actor::NpcType;
        match npc_type {
            NpcType::WildDog => EnemyArchetype::Berserker,
            NpcType::Juggernaut => EnemyArchetype::Juggernaut,
            NpcType::ForestSprite => EnemyArchetype::Kiter,
            NpcType::Defender => EnemyArchetype::Defender,
            NpcType::Skirmisher => EnemyArchetype::Skirmisher,
            NpcType::Ambusher => EnemyArchetype::Ambusher,
        }
    }

    /// Get Approach for this archetype
    pub fn approach(&self) -> crate::components::entity_type::actor::Approach {
        use crate::components::entity_type::actor::Approach;
        match self {
            EnemyArchetype::Berserker => Approach::Direct,
            EnemyArchetype::Juggernaut => Approach::Binding,
            EnemyArchetype::Kiter => Approach::Distant,
            EnemyArchetype::Defender => Approach::Patient,
            EnemyArchetype::Skirmisher => Approach::Evasive,
            EnemyArchetype::Ambusher => Approach::Ambushing,
        }
    }

    /// Get Resilience for this archetype
    pub fn resilience(&self) -> crate::components::entity_type::actor::Resilience {
        use crate::components::entity_type::actor::Resilience;
        match self {
            EnemyArchetype::Berserker => Resilience::Primal,
            EnemyArchetype::Juggernaut => Resilience::Vital,
            EnemyArchetype::Kiter => Resilience::Mental,
            EnemyArchetype::Defender => Resilience::Hardened,
            EnemyArchetype::Skirmisher => Resilience::Shielded,
            EnemyArchetype::Ambusher => Resilience::Blessed,
        }
    }
}

/// Which ActorAttributes field to invest in (6 investable fields, shift excluded)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeField {
    MightAgilityAxis,
    MightAgilitySpectrum,
    VitalityDisciplineAxis,
    VitalityDisciplineSpectrum,
    InstinctResolveAxis,
    InstinctResolveSpectrum,
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
    pub might_agility_shift: i8,
    pub vitality_discipline_shift: i8,
    pub instinct_resolve_shift: i8,
}

// Archetype builds, balanced against one another in the server's arena
static BERSERKER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::MightAgilityAxis, weight: 1, direction: -1 },
];
static JUGGERNAUT_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::VitalityDisciplineAxis, weight: 1, direction: -1 },
];
static KITER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::MightAgilityAxis, weight: 1, direction: 1 },
];
static DEFENDER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::InstinctResolveAxis, weight: 1, direction: 1 },
];
static SKIRMISHER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::InstinctResolveAxis, weight: 1, direction: -1 },
];
static AMBUSHER_BUILD: &[Allocation] = &[
    Allocation { field: AttributeField::VitalityDisciplineAxis, weight: 1, direction: 1 },
];

impl EnemyArchetype {
    /// Get the attribute build for this archetype
    pub fn build(&self) -> NpcBuild {
        match self {
            EnemyArchetype::Berserker => NpcBuild {
                allocations: BERSERKER_BUILD,
                might_agility_shift: 0,
                vitality_discipline_shift: 0,
                instinct_resolve_shift: 0,
            },
            EnemyArchetype::Juggernaut => NpcBuild {
                allocations: JUGGERNAUT_BUILD,
                might_agility_shift: 0,
                vitality_discipline_shift: 0,
                instinct_resolve_shift: 0,
            },
            EnemyArchetype::Kiter => NpcBuild {
                allocations: KITER_BUILD,
                might_agility_shift: 0,
                vitality_discipline_shift: 0,
                instinct_resolve_shift: 0,
            },
            EnemyArchetype::Defender => NpcBuild {
                allocations: DEFENDER_BUILD,
                might_agility_shift: 0,
                vitality_discipline_shift: 0,
                instinct_resolve_shift: 0,
            },
            EnemyArchetype::Skirmisher => NpcBuild {
                allocations: SKIRMISHER_BUILD,
                might_agility_shift: 0,
                vitality_discipline_shift: 0,
                instinct_resolve_shift: 0,
            },
            EnemyArchetype::Ambusher => NpcBuild {
                allocations: AMBUSHER_BUILD,
                might_agility_shift: 0,
                vitality_discipline_shift: 0,
                instinct_resolve_shift: 0,
            },
        }
    }
}

/// Distribute `level` points across allocations using largest-remainder method.

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

/// Points are distributed proportionally across the archetype's build allocations
/// using largest-remainder allocation. Direction is applied to axis fields;
/// spectrum fields are always positive. Fixed shift values come from the build.

/// # Examples
/// ```
/// # use common_bevy::spatial_difficulty::*;
/// let attrs = calculate_enemy_attributes(10, EnemyArchetype::Juggernaut);
/// // Level 10 Juggernaut: all 10 points to VitalityDisciplineAxis, direction -1
/// assert_eq!(attrs.might_agility_axis(), 0);
/// assert_eq!(attrs.vitality_discipline_axis(), -10);
/// assert_eq!(attrs.instinct_resolve_axis(), 0);
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
            AttributeField::MightAgilityAxis => mg_axis += p * alloc.direction,
            AttributeField::MightAgilitySpectrum => mg_spectrum += p,
            AttributeField::VitalityDisciplineAxis => vf_axis += p * alloc.direction,
            AttributeField::VitalityDisciplineSpectrum => vf_spectrum += p,
            AttributeField::InstinctResolveAxis => ip_axis += p * alloc.direction,
            AttributeField::InstinctResolveSpectrum => ip_spectrum += p,
        }
    }

    ActorAttributes::new(
        mg_axis,
        mg_spectrum,
        build.might_agility_shift,
        vf_axis,
        vf_spectrum,
        build.vitality_discipline_shift,
        ip_axis,
        ip_spectrum,
        build.instinct_resolve_shift,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archetype_abilities() {
        assert_eq!(EnemyArchetype::Berserker.ability(), Some(AbilityType::Lunge));
        assert_eq!(EnemyArchetype::Juggernaut.ability(), Some(AbilityType::Rattle));
        assert_eq!(EnemyArchetype::Kiter.ability(), Some(AbilityType::Volley));
        assert_eq!(EnemyArchetype::Defender.ability(), Some(AbilityType::Counter));
        assert_eq!(EnemyArchetype::Skirmisher.ability(), Some(AbilityType::Disengage));
        assert_eq!(EnemyArchetype::Ambusher.ability(), Some(AbilityType::Flank));
    }

    // ===== DISTRIBUTE POINTS TESTS =====

    #[test]
    fn test_distribute_single_slot() {
        let allocs = [Allocation { field: AttributeField::MightAgilityAxis, weight: 1, direction: -1 }];
        let result = distribute_points(10, &allocs);
        assert_eq!(result[0], 10);
    }

    #[test]
    fn test_distribute_equal_weights() {
        let allocs = [
            Allocation { field: AttributeField::MightAgilityAxis, weight: 1, direction: -1 },
            Allocation { field: AttributeField::VitalityDisciplineAxis, weight: 1, direction: -1 },
        ];
        let result = distribute_points(10, &allocs);
        assert_eq!(result[0], 5);
        assert_eq!(result[1], 5);
    }

    #[test]
    fn test_distribute_odd_level_equal_weights() {
        // 7 points / 2 slots → 3 + 4, remainder goes to first slot
        let allocs = [
            Allocation { field: AttributeField::MightAgilityAxis, weight: 1, direction: -1 },
            Allocation { field: AttributeField::VitalityDisciplineAxis, weight: 1, direction: -1 },
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
            Allocation { field: AttributeField::MightAgilityAxis, weight: 3, direction: -1 },
            Allocation { field: AttributeField::VitalityDisciplineAxis, weight: 1, direction: -1 },
        ];
        let result = distribute_points(10, &allocs);
        // 10 * 3/4 = 7.5 → 7, 10 * 1/4 = 2.5 → 2, remainder 1 → slot 0 (larger remainder)
        assert_eq!(result[0], 8);
        assert_eq!(result[1], 2);
    }

    #[test]
    fn test_distribute_zero_level() {
        let allocs = [Allocation { field: AttributeField::MightAgilityAxis, weight: 1, direction: -1 }];
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
        assert_eq!(attrs.might_agility_axis(), 0);
        assert_eq!(attrs.vitality_discipline_axis(), 0);
        assert_eq!(attrs.instinct_resolve_axis(), 0);
    }

    #[test]
    fn test_each_archetype_leads_with_its_attribute() {
        for (archetype, lead) in [
            (EnemyArchetype::Berserker, 0),
            (EnemyArchetype::Juggernaut, 2),
            (EnemyArchetype::Kiter, 1),
            (EnemyArchetype::Defender, 5),
            (EnemyArchetype::Skirmisher, 4),
            (EnemyArchetype::Ambusher, 3),
        ] {
            let attrs = calculate_enemy_attributes(10, archetype);
            let values = [attrs.might(), attrs.agility(), attrs.vitality(), attrs.discipline(), attrs.instinct(), attrs.resolve()];
            let top = (0..6).max_by_key(|&i| values[i]).unwrap();
            assert_eq!(top, lead, "{archetype:?} should lead with attribute {lead}, got {values:?}");
        }
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
            assert_eq!(attrs.might_agility_shift(), 0);
            assert_eq!(attrs.vitality_discipline_shift(), 0);
            assert_eq!(attrs.instinct_resolve_shift(), 0);
        }
    }

    #[test]
    fn test_only_the_skirmisher_invests_in_instinct() {
        // Instinct is the Skirmisher's alone, so only it has Reflex
        for archetype in [EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Kiter, EnemyArchetype::Defender, EnemyArchetype::Ambusher] {
            assert_eq!(calculate_enemy_attributes(10, archetype).reflex(), 0, "{archetype:?}");
        }
        assert!(calculate_enemy_attributes(10, EnemyArchetype::Skirmisher).reflex() > 0);
    }

    #[test]
    fn test_all_points_allocated() {
        // Total absolute axis + spectrum values should equal level for every build
        for (level, archetype) in [1, 5, 10, 15, 20].into_iter().flat_map(|l| [
            EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Kiter, EnemyArchetype::Defender, EnemyArchetype::Skirmisher, EnemyArchetype::Ambusher,
        ].map(|a| (l, a))) {
            let attrs = calculate_enemy_attributes(level, archetype);
            let total = attrs.might_agility_axis().unsigned_abs()
                + attrs.vitality_discipline_axis().unsigned_abs()
                + attrs.instinct_resolve_axis().unsigned_abs()
                + attrs.might_agility_spectrum().unsigned_abs()
                + attrs.vitality_discipline_spectrum().unsigned_abs()
                + attrs.instinct_resolve_spectrum().unsigned_abs();
            assert_eq!(total, level, "{archetype:?} at level {level}: all points should be allocated");
        }
    }
}
