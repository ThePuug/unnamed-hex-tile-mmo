//! The enemy archetypes: what each is drawn as, its signature ability,
//! and how an NPC of one spends the points its level gives it.

use common::den::Habitat;

use crate::{
    components::{entity_type::actor::{Approach, Resilience}, ActorAttributes},
    message::AbilityType,
};

/// The enemy archetypes, each built on one attribute.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnemyArchetype {
    #[default]
    Berserker,
    Juggernaut,
    Flanker,
    Defender,
    Skirmisher,
    Ambusher,
}

/// What an archetype is ([`EnemyArchetype::profile`]).
pub struct Profile {
    /// What an NPC of it is called
    pub name: &'static str,
    /// Its skill, the one of the early kit its attribute's commitment shows in
    pub ability: AbilityType,
    pub approach: Approach,
    pub resilience: Resilience,
    /// The attribute it is built on, as the sign each pair's axis takes
    /// (Might-Agility, Physique-Discipline, Instinct-Resolve): negative the
    /// pair's left attribute, positive its right
    pub build: [i8; 3],
    /// The ground it dens on: a den site of this habitat is one of its
    pub habitat: Habitat,
}

impl EnemyArchetype {
    pub const ALL: [Self; 6] = [Self::Berserker, Self::Juggernaut, Self::Flanker, Self::Defender, Self::Skirmisher, Self::Ambusher];

    /// Everything that sets this archetype apart, one row each
    pub const fn profile(self) -> Profile {
        use AbilityType::*;
        use Approach::*;
        use Resilience::*;
        match self {
            Self::Berserker  => Profile { name: "Wild Dog",      ability: Frenzy,        approach: Direct,    resilience: Primal,   build: [-1, 0, 0], habitat: Habitat::Open },
            Self::Juggernaut => Profile { name: "Juggernaut",    ability: Overpower,     approach: Binding,   resilience: Vital,    build: [0, -1, 0], habitat: Habitat::Rock },
            Self::Flanker      => Profile { name: "Forest Sprite", ability: PerfectStride, approach: Oblique,   resilience: Mental,   build: [1, 0, 0], habitat: Habitat::Woods },
            Self::Defender   => Profile { name: "Defender",      ability: Counter,       approach: Vigilant,  resilience: Hardened, build: [0, 0, 1], habitat: Habitat::Range },
            Self::Skirmisher => Profile { name: "Skirmisher",    ability: Leap,          approach: Fluid,     resilience: Shielded, build: [0, 1, 0], habitat: Habitat::Scrub },
            Self::Ambusher   => Profile { name: "Ambusher",      ability: Punish,        approach: Opportunistic, resilience: Blessed,  build: [0, 0, -1], habitat: Habitat::River },
        }
    }

    /// The archetype that dens on `habitat`
    pub fn denning_on(habitat: Habitat) -> Self {
        Self::ALL.into_iter().find(|archetype| archetype.profile().habitat == habitat).unwrap_or_default()
    }

    /// The stem of its den's model, `models/den-<stem>-active.glb` and its
    /// cleared twin
    pub fn den_model(self) -> &'static str {
        match self {
            Self::Berserker => "berserker",
            Self::Juggernaut => "juggernaut",
            Self::Flanker => "flanker",
            Self::Defender => "defender",
            Self::Skirmisher => "skirmisher",
            Self::Ambusher => "ambusher",
        }
    }
}

impl EnemyArchetype {
    /// The skills an NPC of it holds: its own, and a Feint and a Parry, as
    /// every fighter does
    pub fn bar(self) -> Vec<AbilityType> {
        let own = self.profile().ability;
        std::iter::once(own).chain([AbilityType::Feint, AbilityType::Parry].into_iter().filter(|&shared| shared != own)).collect()
    }
}

/// The attributes of an NPC of `archetype` at `level`: every point of its
/// level in the one attribute the archetype is built on, none in a spectrum
/// and none shifted.
pub fn calculate_enemy_attributes(
    level: u8,
    archetype: EnemyArchetype,
) -> ActorAttributes {
    let points = level.min(i8::MAX as u8) as i8;
    let [physique, conditioning, temperament] = archetype.profile().build.map(|sign| sign * points);
    ActorAttributes::new(physique, 0, 0, conditioning, 0, 0, temperament, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_habitat_has_the_one_archetype_that_dens_on_it() {
        for habitat in Habitat::ALL {
            let denning: Vec<_> = EnemyArchetype::ALL.into_iter().filter(|archetype| archetype.profile().habitat == habitat).collect();
            assert_eq!(denning.len(), 1, "{habitat:?}");
            assert_eq!(EnemyArchetype::denning_on(habitat), denning[0]);
        }
    }

    #[test]
    fn no_two_archetypes_share_a_name_a_skill_or_a_build() {
        for (i, a) in EnemyArchetype::ALL.into_iter().enumerate() {
            for b in &EnemyArchetype::ALL[i + 1..] {
                let (a, b) = (a.profile(), b.profile());
                assert_ne!(a.name, b.name);
                assert_ne!(a.ability, b.ability);
                assert_ne!(a.build, b.build);
            }
        }
    }

    // ===== ATTRIBUTE CALCULATION TESTS =====

    #[test]
    fn test_berserker_level_0() {
        let attrs = calculate_enemy_attributes(0, EnemyArchetype::Berserker);
        assert_eq!(attrs.might_agility_axis(), 0);
        assert_eq!(attrs.physique_discipline_axis(), 0);
        assert_eq!(attrs.instinct_resolve_axis(), 0);
    }

    #[test]
    fn test_each_archetype_leads_with_its_attribute() {
        for (archetype, lead) in [
            (EnemyArchetype::Berserker, 0),
            (EnemyArchetype::Juggernaut, 2),
            (EnemyArchetype::Flanker, 1),
            (EnemyArchetype::Defender, 5),
            (EnemyArchetype::Skirmisher, 3),
            (EnemyArchetype::Ambusher, 4),
        ] {
            let attrs = calculate_enemy_attributes(10, archetype);
            let values = [attrs.might(), attrs.agility(), attrs.physique(), attrs.discipline(), attrs.instinct(), attrs.resolve()];
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
            EnemyArchetype::Flanker,
            EnemyArchetype::Defender,
        ] {
            let attrs = calculate_enemy_attributes(10, archetype);
            assert_eq!(attrs.might_agility_shift(), 0);
            assert_eq!(attrs.physique_discipline_shift(), 0);
            assert_eq!(attrs.instinct_resolve_shift(), 0);
        }
    }

    #[test]
    fn test_only_the_ambusher_invests_in_instinct() {
        // Instinct is the Ambusher's alone, so only it has Reflex
        for archetype in [EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Flanker, EnemyArchetype::Defender, EnemyArchetype::Skirmisher] {
            assert_eq!(calculate_enemy_attributes(10, archetype).reflex(), 0, "{archetype:?}");
        }
        assert!(calculate_enemy_attributes(10, EnemyArchetype::Ambusher).reflex() > 0);
    }

    #[test]
    fn test_all_points_allocated() {
        // Total absolute axis + spectrum values should equal level for every build
        for (level, archetype) in [1, 5, 10, 15, 20].into_iter().flat_map(|l| [
            EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Flanker, EnemyArchetype::Defender, EnemyArchetype::Skirmisher, EnemyArchetype::Ambusher,
        ].map(|a| (l, a))) {
            let attrs = calculate_enemy_attributes(level, archetype);
            let total = attrs.might_agility_axis().unsigned_abs()
                + attrs.physique_discipline_axis().unsigned_abs()
                + attrs.instinct_resolve_axis().unsigned_abs()
                + attrs.might_agility_spectrum().unsigned_abs()
                + attrs.physique_discipline_spectrum().unsigned_abs()
                + attrs.instinct_resolve_spectrum().unsigned_abs();
            assert_eq!(total, level, "{archetype:?} at level {level}: all points should be allocated");
        }
    }
}
