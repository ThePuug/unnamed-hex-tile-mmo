# ADR-016: Three-Pole Attribute Scaling System

## Status
Proposed

## Context

### The Scaling Problem

Current combat balance suffers from inconsistent scaling logic scattered across ability implementations:
- Auto-attack uses `base * (1 + might/33)` in damage pipeline
- Abilities hardcode base damage values
- Recovery times are fixed constants
- Counter reflection is hardcoded at 50%
- No clear framework for when stats should scale linearly vs. relatively

This creates several issues:
1. **Balancing difficulty**: No unified model to reason about power curves
2. **Level progression feels weak**: Leveling up doesn't feel impactful when fighting same-level enemies
3. **Investment feels inconsistent**: 50% commitment to a stat feels different at level 10 vs level 50
4. **Contested abilities ignore context**: Counter reflection doesn't account for opponent's level/skill

### The Three-Pole Framework

After analysis of combat balance requirements, we've identified three fundamental scaling dimensions that different ability components need:

**1. MAGNITUDE (Absolute Power)**
- Raw capability that scales with absolute stat values
- "More is better" regardless of context
- Examples: Damage output, HP pools, resource capacity

**2. COMMITMENT (Specialization Efficiency)**
- Effectiveness based on percentage of total investment
- Rewards specialization equally at all levels
- Examples: Cooldown reduction, attack speed, movement speed

**3. RATIO (Contested Matchup)**
- Effectiveness determined by comparison between actors
- Your investment vs. their investment/level
- Examples: Counter reflection, mitigation penetration, CC duration

**Key Insight:** Each ability *component* (not ability as a whole) uses 1-2 of these poles. For example:
- **Lunge** has:
  - Damage component (MAGNITUDE pole)
  - Recovery component (COMMITMENT pole)
- **Counter** has:
  - Reflection % component (COMMITMENT + RATIO poles)

## Decision

### Core Architecture

Create a unified scaling system with three calculation functions and a component-based composition model.

#### 1. Scaling Function Signatures

```rust
/// Magnitude scaling: absolute stat values produce absolute outputs
/// Used for: damage, HP, resource pools
pub fn calculate_magnitude_value(
    base: f32,
    level: u32,
    stat: i8,
    reach: u32,
    scalars: MagnitudeScalars,
) -> f32 {
    base
        + (level as f32 * scalars.level)
        + (stat.abs() as f32 * scalars.stat)
        + (reach as f32 * scalars.reach)
}

/// Commitment scaling: investment ratio produces efficiency
/// Used for: cooldowns, attack speed, movement speed
pub fn calculate_commitment_modifier(
    stat: i8,
    total_level: u32,
    curve: CommitmentCurve,
) -> f32 {
    if stat <= 0 {
        return curve.base;
    }

    // Investment ratio: (points spent) / (points available)
    // 1 point = 2 axis movement, so stat/2 = points spent
    let investment_ratio = ((stat as f32 / 2.0) / total_level as f32)
        .min(curve.max_ratio);

    // Apply curve function (linear, sqrt, etc.)
    let modified_ratio = match curve.function {
        CurveFunction::Linear => investment_ratio,
        CurveFunction::Sqrt => investment_ratio.sqrt(),
        CurveFunction::Square => investment_ratio.powi(2),
    };

    // Apply to base value
    match curve.mode {
        CurveMode::Additive => curve.base + (modified_ratio * curve.scale),
        CurveMode::Multiplicative => curve.base * (1.0 + modified_ratio * curve.scale),
        CurveMode::Reduction => curve.base * (1.0 - modified_ratio * curve.scale),
    }
}

/// Ratio scaling: contested matchup between actors
/// Used for: counter reflection, mitigation penetration, dodge/parry
pub fn calculate_ratio_effectiveness(
    your_stat: i8,
    your_level: u32,
    their_stat: i8,
    their_level: u32,
    config: RatioConfig,
) -> f32 {
    // Your skill/power
    let your_investment = ((your_stat.abs() as f32 / 2.0) / your_level as f32)
        .min(config.max_ratio);
    let your_power = your_investment.sqrt(); // Diminishing returns

    // Their resistance/defense
    let their_investment = ((their_stat.abs() as f32 / 2.0) / their_level as f32)
        .min(config.max_ratio);
    let their_power = their_investment.sqrt();

    // Level factor (experience advantage)
    let level_factor = if config.level_matters {
        (their_level as f32 / your_level as f32).sqrt()
    } else {
        1.0
    };

    let their_resistance = their_power * level_factor;

    // Ratio determines effectiveness
    let ratio = your_power / their_resistance.max(config.min_resistance);

    // Apply to base value
    config.base * (config.base_multiplier + ratio * config.ratio_scale)
}
```

#### 2. Configuration Types

```rust
/// Scalars for magnitude-based calculations
#[derive(Debug, Clone, Copy)]
pub struct MagnitudeScalars {
    pub level: f32,  // Per-level scaling
    pub stat: f32,   // Per-stat-point scaling
    pub reach: f32,  // Per-reach-point scaling
}

/// Curve configuration for commitment-based scaling
#[derive(Debug, Clone, Copy)]
pub struct CommitmentCurve {
    pub base: f32,           // Starting value (at 0% investment)
    pub scale: f32,          // Scaling factor
    pub max_ratio: f32,      // Cap on investment ratio (typically 2.0 for spectrum)
    pub function: CurveFunction,  // Shape of curve
    pub mode: CurveMode,     // How to apply the result
}

#[derive(Debug, Clone, Copy)]
pub enum CurveFunction {
    Linear,   // 1:1 relationship
    Sqrt,     // Diminishing returns
    Square,   // Accelerating returns (rare)
}

#[derive(Debug, Clone, Copy)]
pub enum CurveMode {
    Additive,        // base + (ratio * scale)
    Multiplicative,  // base * (1 + ratio * scale)
    Reduction,       // base * (1 - ratio * scale) - for cooldowns
}

/// Configuration for ratio-based contested calculations
#[derive(Debug, Clone, Copy)]
pub struct RatioConfig {
    pub base: f32,             // Base effectiveness (at 1:1 ratio)
    pub base_multiplier: f32,  // Multiplier applied to base
    pub ratio_scale: f32,      // How much ratio affects result
    pub max_ratio: f32,        // Cap on investment ratio
    pub min_resistance: f32,   // Minimum resistance value (prevent div/0)
    pub level_matters: bool,   // Whether level difference affects ratio
}
```

#### 3. Ability Component System

Each ability declares its components and their scaling poles:

```rust
/// Defines how an ability component scales
#[derive(Debug, Clone)]
pub enum ComponentScaling {
    Magnitude {
        base: f32,
        scalars: MagnitudeScalars,
    },
    Commitment {
        curve: CommitmentCurve,
    },
    MagnitudeAndRatio {
        magnitude: MagnitudeScalars,
        ratio: RatioConfig,
    },
    CommitmentAndRatio {
        commitment: CommitmentCurve,
        ratio: RatioConfig,
    },
}

/// Ability component definition
#[derive(Debug, Clone)]
pub struct AbilityComponent {
    pub name: &'static str,
    pub scaling: ComponentScaling,
    pub primary_stat: AttributeStat,  // Which stat scales this
    pub secondary_stat: Option<AttributeStat>, // For ratio calculations
}

/// Complete ability definition
pub struct AbilityDefinition {
    pub ability_type: AbilityType,
    pub components: &'static [AbilityComponent],
}
```

#### 4. Example Ability Definitions

```rust
// Lunge: damage (magnitude) + recovery (commitment)
pub const LUNGE: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::Lunge,
    components: &[
        AbilityComponent {
            name: "damage",
            scaling: ComponentScaling::Magnitude {
                base: 40.0,
                scalars: MagnitudeScalars {
                    level: 1.0,    // +1 damage per level
                    stat: 2.0,     // +2 damage per Might
                    reach: 0.0,    // Reach doesn't affect damage
                },
            },
            primary_stat: AttributeStat::Might,
            secondary_stat: None,
        },
        AbilityComponent {
            name: "recovery",
            scaling: ComponentScaling::Commitment {
                curve: CommitmentCurve {
                    base: 1.0,
                    scale: 1.0 / 3.0,  // sqrt(ratio) / 3
                    max_ratio: 2.0,
                    function: CurveFunction::Sqrt,
                    mode: CurveMode::Reduction,
                },
            },
            primary_stat: AttributeStat::Presence,
            secondary_stat: None,
        },
    ],
};

// Auto-attack: damage only (magnitude)
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
                    reach: 0.0,
                },
            },
            primary_stat: AttributeStat::Might,
            secondary_stat: None,
        },
    ],
};

// Counter: reflection % (commitment + ratio)
pub const COUNTER: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::Counter,
    components: &[
        AbilityComponent {
            name: "reflection",
            scaling: ComponentScaling::CommitmentAndRatio {
                commitment: CommitmentCurve {
                    base: 0.5,
                    scale: 0.7,
                    max_ratio: 2.0,
                    function: CurveFunction::Sqrt,
                    mode: CurveMode::Multiplicative,
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
            secondary_stat: Some(AttributeStat::Might), // Opponent's offense
        },
    ],
};

// Physical Mitigation: base (magnitude) + contested (ratio)
pub const PHYSICAL_MITIGATION: AbilityDefinition = AbilityDefinition {
    ability_type: AbilityType::Passive, // Not an ability, but uses same system
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
            secondary_stat: Some(AttributeStat::Might), // Attacker's offense
        },
    ],
};
```

#### 5. Module Organization

```
src/common/systems/combat/
├── scaling/
│   ├── mod.rs              // Public API
│   ├── magnitude.rs        // Magnitude calculations
│   ├── commitment.rs       // Commitment calculations
│   ├── ratio.rs            // Ratio calculations
│   ├── definitions.rs      // Ability component definitions
│   └── types.rs            // Configuration types
├── damage.rs               // Uses scaling system for damage
└── abilities/
    ├── auto_attack.rs      // Uses AUTO_ATTACK definition
    ├── lunge.rs            // Uses LUNGE definition
    ├── counter.rs          // Uses COUNTER definition
    └── ...
```

#### 6. Usage in Ability Handlers

```rust
// In auto_attack.rs
use crate::common::systems::combat::scaling::{
    calculate_magnitude_value, AUTO_ATTACK,
};

pub fn handle_auto_attack(/* ... */) {
    // Get damage component
    let damage_component = AUTO_ATTACK.components
        .iter()
        .find(|c| c.name == "damage")
        .unwrap();

    // Calculate damage using magnitude scaling
    let ComponentScaling::Magnitude { base, scalars } = damage_component.scaling else {
        panic!("Auto-attack damage must use magnitude scaling");
    };

    let damage = calculate_magnitude_value(
        base,
        caster_level,
        caster_attrs.might(),
        0, // Auto-attack doesn't use reach
        scalars,
    );

    // Emit damage event with calculated value
    commands.trigger_targets(
        Try {
            event: GameEvent::DealDamage {
                source: *ent,
                target: target_ent,
                base_damage: damage,
                damage_type: DamageType::Physical,
                ability: Some(AbilityType::AutoAttack),
            },
        },
        target_ent,
    );
}

// In lunge.rs
pub fn handle_lunge(/* ... */) {
    // Calculate damage
    let damage_component = LUNGE.components
        .iter()
        .find(|c| c.name == "damage")
        .unwrap();

    let damage = match damage_component.scaling {
        ComponentScaling::Magnitude { base, scalars } => {
            calculate_magnitude_value(
                base,
                caster_level,
                caster_attrs.might(),
                0,
                scalars,
            )
        }
        _ => panic!("Lunge damage must use magnitude scaling"),
    };

    // Calculate recovery time
    let recovery_component = LUNGE.components
        .iter()
        .find(|c| c.name == "recovery")
        .unwrap();

    let recovery = match recovery_component.scaling {
        ComponentScaling::Commitment { curve } => {
            calculate_commitment_modifier(
                caster_attrs.presence(),
                caster_level,
                curve,
            )
        }
        _ => panic!("Lunge recovery must use commitment scaling"),
    };

    // Apply recovery lockout
    let recovery_component = GlobalRecovery::new(
        Duration::from_secs_f32(recovery),
        AbilityType::Lunge,
    );
    commands.entity(*ent).insert(recovery_component);
}

// In counter.rs
pub fn handle_counter(/* ... */) {
    // Calculate reflection percentage (commitment + ratio)
    let reflection_component = COUNTER.components
        .iter()
        .find(|c| c.name == "reflection")
        .unwrap();

    let reflection_percent = match reflection_component.scaling {
        ComponentScaling::CommitmentAndRatio { commitment, ratio } => {
            // Your Grace skill
            let skill_modifier = calculate_commitment_modifier(
                caster_attrs.grace(),
                caster_level,
                commitment,
            );

            // Contested vs opponent's level/might
            let ratio_modifier = calculate_ratio_effectiveness(
                caster_attrs.grace(),
                caster_level,
                threat_source_attrs.might(), // Their offense
                threat_source_level,
                ratio,
            );

            // Combine both factors
            (skill_modifier * ratio_modifier).clamp(0.25, 0.90)
        }
        _ => panic!("Counter reflection must use commitment+ratio scaling"),
    };

    let reflected_damage = threat.damage * reflection_percent;
    // ... reflect damage back
}
```

### Migration Strategy

#### Phase 1: Implement Core System
- [ ] Create `src/common/systems/combat/scaling/` module
- [ ] Implement three scaling functions
- [ ] Define configuration types
- [ ] Add ability component definitions for existing abilities
- [ ] Write comprehensive unit tests

#### Phase 2: Migrate Abilities
- [ ] Auto-attack (magnitude only, simple)
- [ ] Lunge (magnitude + commitment)
- [ ] Overpower (magnitude only)
- [ ] Counter (commitment + ratio)
- [ ] Update damage pipeline to remove old `calculate_outgoing_damage()`

#### Phase 3: Migrate Derived Stats
- [ ] Max HP (magnitude with reach)
- [ ] Physical mitigation (magnitude + ratio)
- [ ] Movement speed (magnitude + commitment)
- [ ] Resource pools (magnitude)

#### Phase 4: Add `total_level` to ActorAttributes
- [ ] Add field or calculation method
- [ ] Update all attribute construction sites
- [ ] Use in all scaling calculations

## Consequences

### Positive

**Clarity and Maintainability:**
- Scaling logic centralized and documented
- Adding new abilities is declarative (just define components)
- Each component's intent is explicit (magnitude/commitment/ratio)
- Easy to reason about balance across all abilities

**Consistency:**
- All abilities follow same scaling framework
- Predictable power curves across levels
- Investment feels equally meaningful at all levels (commitment pole)
- Contested abilities properly account for matchups (ratio pole)

**Balance Flexibility:**
- Tune constants without changing code
- Easy to compare balance across abilities (look at definitions)
- Can extract definitions to config files later if needed
- Clear separation between design (definitions) and implementation (calculations)

**Testability:**
- Pure functions easy to unit test
- Can validate entire ability definition set
- Can simulate combat scenarios at any level
- Balance spreadsheets can use same formulas

### Negative

**Complexity:**
- More abstraction layers than current hardcoded approach
- Developers must learn three-pole model
- Configuration types add boilerplate
- Requires documentation and examples

**Performance:**
- Extra function calls and matches (likely negligible)
- More heap allocations for configurations (could optimize later)
- May need caching for frequently calculated values

**Migration Cost:**
- All existing abilities need updating
- Damage pipeline needs refactoring
- Requires careful testing to preserve existing balance
- May discover edge cases during migration

### Neutral

**Type Safety:**
- More compile-time guarantees about scaling correctness
- Easier to catch mismatched stat/component pairings
- But adds verbosity to ability definitions

**Future Extensions:**
- Can add new poles if needed (e.g., "cooperative" for ally buffs)
- Can add new curve functions easily
- May need to refactor if poles aren't sufficient (low risk)

## Testing Requirements

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_scales_linearly() {
        let scalars = MagnitudeScalars {
            level: 0.5,
            stat: 1.0,
            reach: 0.0,
        };

        // Level 10, 10 Might
        let damage = calculate_magnitude_value(20.0, 10, 10, 0, scalars);
        assert_eq!(damage, 35.0); // 20 + 5 + 10
    }

    #[test]
    fn commitment_same_at_all_levels() {
        let curve = CommitmentCurve {
            base: 1.0,
            scale: 1.0 / 3.0,
            max_ratio: 2.0,
            function: CurveFunction::Sqrt,
            mode: CurveMode::Reduction,
        };

        // 50% commitment at level 10 and level 50
        let recovery_l10 = calculate_commitment_modifier(10, 10, curve);
        let recovery_l50 = calculate_commitment_modifier(50, 50, curve);

        assert!((recovery_l10 - recovery_l50).abs() < 0.001);
    }

    #[test]
    fn ratio_accounts_for_level_difference() {
        let config = RatioConfig {
            base: 0.5,
            base_multiplier: 1.0,
            ratio_scale: 0.5,
            max_ratio: 2.0,
            min_resistance: 0.5,
            level_matters: true,
        };

        // Same investment, different levels
        let equal_level = calculate_ratio_effectiveness(10, 10, 10, 10, config);
        let lower_level = calculate_ratio_effectiveness(10, 10, 10, 50, config);
        let higher_level = calculate_ratio_effectiveness(10, 10, 10, 5, config);

        assert!(lower_level < equal_level); // Harder vs higher level
        assert!(higher_level > equal_level); // Easier vs lower level
    }
}
```

### Integration Tests
- Simulate full combat scenarios at various levels
- Verify 3-level difference = ~20% swing in hits-to-kill
- Test all ability definitions produce expected values
- Validate balance targets (from PLAYER spec)

### Balance Validation
- Create spreadsheet with formulas matching code
- Test all level ranges (1-50)
- Verify investment ratios feel correct
- Confirm contested abilities behave properly

## Open Questions

1. **Caching:** Should we cache calculated values (e.g., per-frame)? Likely premature optimization.
2. **Configuration Files:** Should ability definitions live in JSON/RON files? Start with code, migrate if needed.
3. **Additional Poles:** Will we need more poles beyond magnitude/commitment/ratio? Unlikely, but framework allows it.
4. **Derived Stat Integration:** Should all derived stats (movement speed, regen) use this system? Yes, eventually.
5. **Client Prediction:** Do clients need full scaling calculations for prediction? Yes, system must be in `common/`.

## References

- Combat System Spec: `docs/spec/combat-system.md`
- Attribute System Spec: `docs/spec/attribute-system.md`
- Triumvirate Spec: `docs/spec/triumvirate.md`
- ADR-002: Combat Foundation
- ADR-009: MVP Ability Set

## Implementation Notes

**For DEVELOPER role:**

When implementing this system:
1. Start with core scaling functions + tests (TDD)
2. Add one simple ability (auto-attack) to validate pattern
3. Add complex ability (lunge, counter) to validate multi-component
4. Migrate remaining abilities incrementally
5. Remove old scaling code only after all migrations complete

Constants in ability definitions are starting points - expect tuning after playtesting.

Ability definitions should live in `scaling/definitions.rs` initially, can be split per-ability later if file gets large.

All three scaling functions must live in `common/` for client-side prediction to work correctly.
