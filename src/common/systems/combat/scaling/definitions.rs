//! Ability component definitions
//!
//! Defines how each ability scales using the three-pole system.
//! Each ability is composed of 1+ components (damage, recovery, etc.),
//! and each component uses 1-2 scaling poles (magnitude, commitment, ratio).

use super::types::{CommitmentCurve, CurveFunction, CurveMode, MagnitudeScalars, RatioConfig};
use crate::common::message::AbilityType;

/// Attribute stats that can scale ability components
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeStat {
    /// Physical offense
    Might,
    /// Agility and finesse
    Grace,
    /// Physical defense and HP
    Vitality,
    /// Magical power and defense
    Focus,
    /// Critical hits and burst
    Instinct,
    /// Cooldown reduction and control
    Presence,
}

/// Defines how an ability component scales
#[derive(Debug, Clone)]
pub enum ComponentScaling {
    /// Pure magnitude scaling (absolute power)
    Magnitude {
        base: f32,
        scalars: MagnitudeScalars,
    },
    /// Pure commitment scaling (specialization efficiency)
    Commitment {
        curve: CommitmentCurve,
    },
    /// Magnitude + contested ratio
    MagnitudeAndRatio {
        magnitude: MagnitudeScalars,
        ratio: RatioConfig,
    },
    /// Commitment + contested ratio
    CommitmentAndRatio {
        commitment: CommitmentCurve,
        ratio: RatioConfig,
    },
}

/// A single component of an ability (damage, recovery, etc.)
#[derive(Debug, Clone)]
pub struct AbilityComponent {
    /// Component name (e.g., "damage", "recovery", "reflection")
    pub name: &'static str,
    /// Scaling configuration for this component
    pub scaling: ComponentScaling,
    /// Primary stat that scales this component
    pub primary_stat: AttributeStat,
    /// Secondary stat for ratio calculations (opponent's stat)
    pub secondary_stat: Option<AttributeStat>,
}

/// Complete ability definition with all components
#[derive(Debug, Clone)]
pub struct AbilityDefinition {
    /// Which ability this defines
    pub ability_type: AbilityType,
    /// List of components and their scaling
    pub components: &'static [AbilityComponent],
}

// ===== ABILITY DEFINITIONS =====
// These define how each ability scales in the game

/// Auto-attack: Passive basic attack
/// - Damage: Magnitude only (base + level + Might)
pub const AUTO_ATTACK: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::AutoAttack,
    components: &[
        AbilityComponent {
            name: "damage",
            scaling: ComponentScaling::Magnitude {
                base: 20.0,
                scalars: MagnitudeScalars {
                    level: 0.5,   // +0.5 damage per level
                    stat: 1.0,    // +1 damage per Might
                    reach: 0.0,   // Reach doesn't affect auto-attack
                },
            },
            primary_stat: AttributeStat::Might,
            secondary_stat: None,
        },
    ],
};

/// Lunge: Gap closer with damage
/// - Damage: Magnitude (base + level + Might)
/// - Recovery: Commitment (reduced by Presence investment)
pub const LUNGE: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::Lunge,
    components: &[
        AbilityComponent {
            name: "damage",
            scaling: ComponentScaling::Magnitude {
                base: 40.0,
                scalars: MagnitudeScalars {
                    level: 1.0,   // +1 damage per level
                    stat: 2.0,    // +2 damage per Might
                    reach: 0.0,   // Reach doesn't affect lunge damage
                },
            },
            primary_stat: AttributeStat::Might,
            secondary_stat: None,
        },
        AbilityComponent {
            name: "recovery",
            scaling: ComponentScaling::Commitment {
                curve: CommitmentCurve {
                    base: 1.0,           // 1.0s base recovery
                    scale: 1.0 / 3.0,    // Up to 33% reduction
                    max_ratio: 2.0,      // Cap at 200% investment
                    function: CurveFunction::Sqrt,  // Diminishing returns
                    mode: CurveMode::Reduction,     // Reduce recovery time
                },
            },
            primary_stat: AttributeStat::Presence,
            secondary_stat: None,
        },
    ],
};

/// Overpower: Overwhelming strike (Triumvirate signature)
/// - Damage: Magnitude (Presence primary, Might secondary)
/// - Scales with both dominance (Presence) and physical power (Might)
pub const OVERPOWER: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::Overpower,
    components: &[
        AbilityComponent {
            name: "damage",
            scaling: ComponentScaling::Magnitude {
                base: 20.0,
                scalars: MagnitudeScalars {
                    level: 1.5,   // +1.5 damage per level
                    stat: 4.0,    // +4 damage per Presence (primary)
                    reach: 1.5,   // +1.5 damage per Might (secondary, using reach slot)
                },
            },
            primary_stat: AttributeStat::Presence,
            secondary_stat: Some(AttributeStat::Might),
        },
    ],
};

/// Counter: Defensive reaction with damage reflection
/// - Reflection: Commitment (your Grace skill) + Ratio (vs opponent's Might/level)
pub const COUNTER: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::Counter,
    components: &[
        AbilityComponent {
            name: "reflection",
            scaling: ComponentScaling::CommitmentAndRatio {
                commitment: CommitmentCurve {
                    base: 0.5,           // 50% base reflection
                    scale: 0.7,          // Up to 70% bonus from commitment
                    max_ratio: 2.0,
                    function: CurveFunction::Sqrt,  // Diminishing returns
                    mode: CurveMode::Multiplicative, // Multiply base reflection
                },
                ratio: RatioConfig {
                    base: 0.5,
                    base_multiplier: 1.0,
                    ratio_scale: 0.5,
                    max_ratio: 2.0,
                    min_resistance: 0.5,
                    level_matters: true,  // Higher level enemies harder to counter
                },
            },
            primary_stat: AttributeStat::Grace,
            secondary_stat: Some(AttributeStat::Might),  // Opponent's offense
        },
    ],
};

/// Physical Mitigation: Passive damage reduction
/// - Base mitigation: Magnitude (flat % from Vitality)
/// - Contested bonus: Ratio (your Vitality vs attacker's Might)
///
/// Note: This isn't technically an "ability" but uses the same scaling system
/// for derived stats.
pub const PHYSICAL_MITIGATION: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::AutoAttack, // Placeholder - not a real ability
    components: &[
        AbilityComponent {
            name: "base_mitigation",
            scaling: ComponentScaling::Magnitude {
                base: 0.0,
                scalars: MagnitudeScalars {
                    level: 0.0,
                    stat: 0.005,  // 0.5% per Vitality point
                    reach: 0.0,
                },
            },
            primary_stat: AttributeStat::Vitality,
            secondary_stat: None,
        },
        AbilityComponent {
            name: "contested_bonus",
            scaling: ComponentScaling::MagnitudeAndRatio {
                magnitude: MagnitudeScalars {
                    level: 0.0,
                    stat: 0.0,
                    reach: 0.0,
                },
                ratio: RatioConfig {
                    base: 0.15,
                    base_multiplier: 1.0,
                    ratio_scale: 1.0,
                    max_ratio: 2.0,
                    min_resistance: 0.5,
                    level_matters: true,  // Experienced attackers penetrate better
                },
            },
            primary_stat: AttributeStat::Vitality,
            secondary_stat: Some(AttributeStat::Might),  // Attacker's offense
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    // ===== DEFINITION VALIDATION TESTS =====
    // Verify ability definitions are well-formed

    #[test]
    fn test_auto_attack_definition() {
        assert_eq!(AUTO_ATTACK.ability_type, AbilityType::AutoAttack);
        assert_eq!(AUTO_ATTACK.components.len(), 1);
        assert_eq!(AUTO_ATTACK.components[0].name, "damage");
        assert_eq!(AUTO_ATTACK.components[0].primary_stat, AttributeStat::Might);
    }

    #[test]
    fn test_lunge_definition() {
        assert_eq!(LUNGE.ability_type, AbilityType::Lunge);
        assert_eq!(LUNGE.components.len(), 2);

        let damage = &LUNGE.components[0];
        assert_eq!(damage.name, "damage");
        assert_eq!(damage.primary_stat, AttributeStat::Might);

        let recovery = &LUNGE.components[1];
        assert_eq!(recovery.name, "recovery");
        assert_eq!(recovery.primary_stat, AttributeStat::Presence);
    }

    #[test]
    fn test_counter_definition() {
        assert_eq!(COUNTER.ability_type, AbilityType::Counter);
        assert_eq!(COUNTER.components.len(), 1);

        let reflection = &COUNTER.components[0];
        assert_eq!(reflection.name, "reflection");
        assert_eq!(reflection.primary_stat, AttributeStat::Grace);
        assert_eq!(reflection.secondary_stat, Some(AttributeStat::Might));
    }

    #[test]
    fn test_physical_mitigation_definition() {
        assert_eq!(PHYSICAL_MITIGATION.components.len(), 2);

        let base_mit = &PHYSICAL_MITIGATION.components[0];
        assert_eq!(base_mit.name, "base_mitigation");
        assert_eq!(base_mit.primary_stat, AttributeStat::Vitality);

        let contested = &PHYSICAL_MITIGATION.components[1];
        assert_eq!(contested.name, "contested_bonus");
        assert_eq!(contested.primary_stat, AttributeStat::Vitality);
        assert_eq!(contested.secondary_stat, Some(AttributeStat::Might));
    }

    #[test]
    fn test_component_scaling_variants() {
        // Auto-attack uses Magnitude only
        match &AUTO_ATTACK.components[0].scaling {
            ComponentScaling::Magnitude { .. } => {},
            _ => panic!("Auto-attack damage should use Magnitude scaling"),
        }

        // Lunge recovery uses Commitment only
        match &LUNGE.components[1].scaling {
            ComponentScaling::Commitment { .. } => {},
            _ => panic!("Lunge recovery should use Commitment scaling"),
        }

        // Counter uses CommitmentAndRatio
        match &COUNTER.components[0].scaling {
            ComponentScaling::CommitmentAndRatio { .. } => {},
            _ => panic!("Counter reflection should use CommitmentAndRatio scaling"),
        }

        // Physical mitigation contested uses MagnitudeAndRatio
        match &PHYSICAL_MITIGATION.components[1].scaling {
            ComponentScaling::MagnitudeAndRatio { .. } => {},
            _ => panic!("Physical mitigation contested should use MagnitudeAndRatio scaling"),
        }
    }

    #[test]
    fn test_all_definitions_have_components() {
        let definitions = [&AUTO_ATTACK, &LUNGE, &OVERPOWER, &COUNTER, &PHYSICAL_MITIGATION];

        for def in definitions {
            assert!(!def.components.is_empty(), "Definition should have at least one component");
        }
    }
}
