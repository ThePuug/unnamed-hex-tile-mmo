//! Where the haven stands, and the enemy archetypes: what each is drawn
//! as, its signature ability, and how an NPC of one spends the points its
//! level gives it.

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

/// The attributes of an NPC of `archetype` at `level`: every point of its
/// level in the one attribute the archetype is built on, none in a spectrum
/// and none shifted.
///
/// # Examples
/// ```
/// # use common_bevy::spatial_difficulty::*;
/// let attrs = calculate_enemy_attributes(10, EnemyArchetype::Juggernaut);
/// assert_eq!(attrs.might_agility_axis(), 0);
/// assert_eq!(attrs.vitality_discipline_axis(), -10);
/// assert_eq!(attrs.instinct_resolve_axis(), 0);
/// ```
pub fn calculate_enemy_attributes(
    level: u8,
    archetype: EnemyArchetype,
) -> ActorAttributes {
    let points = level.min(i8::MAX as u8) as i8;
    // Each pair's axis: negative its left attribute, positive its right
    let (physique, conditioning, temperament) = match archetype {
        EnemyArchetype::Berserker => (-points, 0, 0),  // Might
        EnemyArchetype::Kiter => (points, 0, 0),       // Agility
        EnemyArchetype::Juggernaut => (0, -points, 0), // Vitality
        EnemyArchetype::Ambusher => (0, points, 0),    // Discipline
        EnemyArchetype::Skirmisher => (0, 0, -points), // Instinct
        EnemyArchetype::Defender => (0, 0, points),    // Resolve
    };
    ActorAttributes::new(physique, 0, 0, conditioning, 0, 0, temperament, 0, 0)
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
