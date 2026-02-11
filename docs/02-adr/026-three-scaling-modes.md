# ADR-026: Three Scaling Modes Architecture

## Status

Proposed - 2026-02-10

## Context

**Related RFC:** [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)

The previous attribute system uses a single linear scaling mode: each attribute point adds a fixed amount to derived stats. This conflates three distinct design concerns:

1. **Progression** — How strong is this entity at their level? (Should scale with level)
2. **Build matchup** — How does this entity's build compare to an opponent's? (Should depend on stat difference, not level)
3. **Build identity** — What kind of fighter is this entity? (Should be level-invariant)

A level-10 Focus specialist and a level-10 Might specialist should have the same absolute power level (progression) but different relative matchup profiles (build benefit) and different combat identities (who they are). Single-mode scaling cannot separate these concerns.

**Supersedes:** The linear-only stat derivation in [attribute-system.md](../00-spec/attribute-system.md) and the single-dimension scaling in [ADR-005](005-derived-combat-stats.md).

**References:**
- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md) — Full system design
- [ADR-020: Super-Linear Level Multiplier](020-super-linear-level-multiplier.md) — Polynomial multiplier (reused for absolute mode)
- [ADR-005: Derived Combat Stats](005-derived-combat-stats.md) — Existing linear derivation (extended, not replaced)

## Decision

Each of the six attributes (Might, Grace, Vitality, Focus, Instinct, Presence) has **three named sub-attributes**, one per scaling mode. Each mode scales independently and serves a different design purpose.

### Core Mechanism

**Absolute (Progression) — Scales with level:**

```
absolute_value = derived_attribute × level_multiplier(total_budget, k, p)
```

Reuses the polynomial multiplier from ADR-020. Super-linear scaling means level dominates — a level-10 entity vastly outperforms a level-5 in absolute stats. HP exponent > Damage exponent ensures more exchanges at higher levels.

| Attribute | Absolute Name | Concrete Stat |
|-----------|--------------|---------------|
| Might | Force | Damage |
| Grace | Technique | — (open) |
| Vitality | Constitution | HP |
| Focus | Discipline | — (open) |
| Instinct | Intuition | — (open) |
| Presence | Gravitas | — (open) |

**Relative (Build Benefit) — No level scaling:**

```
contest_outcome = f(attacker_stat - defender_stat)
```

Raw stat values are compared between attacker and defender. No level multiplier applied. A lower-level player who heavily invested in one attribute can win relative contests against a higher-level player who neglected the opposing stat.

Three opposing pairs:

| Attacker | Defender | Contest |
|----------|----------|---------|
| Precision (Grace) | Toughness (Vitality) | Crit vs mitigation |
| Impact (Might) | Composure (Focus) | Force vs discipline |
| Dominance (Presence) | Cunning (Instinct) | Tempo control |

**Commitment (Build Identity) — Level-invariant:**

```
commitment_tier = tier_from_percentage(derived_attribute / total_budget)
```

Percentage of total budget invested determines a discrete tier (see ADR-027). Commitment does not scale with level — it is a permanent statement about build identity. Tiers unlock concrete combat stats.

| Attribute | Commitment Name | Concrete Stat |
|-----------|----------------|---------------|
| Might | Ferocity | — (open) |
| Grace | Poise | Evasion |
| Vitality | Grit | — (open) |
| Focus | Concentration | Queue capacity |
| Instinct | Flow | — (open) |
| Presence | Intensity | Cadence |

## Rationale

**Why three modes, not one:**
- Single mode forces "being strong" and "being a specialist" to scale together — you can't be a low-level specialist or a high-level generalist with distinct identity
- Three modes create independent tuning knobs: adjust progression curve without affecting matchups, adjust matchup formulas without affecting identity
- Equipment can modify each mode independently (future RFC): +Force, +Precision, +Poise are three different item properties

**Why these specific three modes:**
- Absolute addresses the progression fantasy ("I'm getting stronger")
- Relative addresses the matchup fantasy ("my build counters yours")
- Commitment addresses the identity fantasy ("I am a Might fighter")
- Each mode answers a different player question and uses a different math model

**Why reuse ADR-020 for absolute rather than create new scaling:**
- Polynomial level multiplier is already implemented and tested
- Same formula, broader application (all absolute stats, not just HP/damage)
- Backward compatible — level-0 multiplier = 1.0

**Why relative has no level scaling:**
- If relative scaled with level, it would be redundant with absolute
- Build investment should matter regardless of level — a Might-heavy level-5 should feel their impact advantage against a Focus-heavy level-8
- Level advantage is already captured by absolute mode

## Consequences

**Positive:**
- Three independent tuning surfaces for game balance
- Clear conceptual model for players and designers
- Equipment system (future) gets rich itemization space
- Each mode can be iterated independently without affecting others

**Negative:**
- Three sub-attributes per attribute = 18 named stats total (cognitive load)
- Some sub-attributes have no concrete stat yet (open design space)
- More complex data model than single-mode system

**Mitigations:**
- Players only need to understand their build's 2-3 relevant stats, not all 18
- Open stats are explicitly documented — fill in as gameplay testing reveals needs
- Data model complexity is internal; player-facing UI shows concrete effects

## Implementation Notes

**Data Model:**
The existing Axis/Spectrum/Shift bipolar model produces six derived attribute values (`might()`, `grace()`, `vitality()`, `focus()`, `instinct()`, `presence()`). These derived values serve as input to all three scaling modes:
- Absolute: computed at spawn/level-up (cached)
- Relative: computed at combat event time (per contest)
- Commitment: computed at spawn/level-up/respec (cached tier)

**Files Affected:**
- `src/common/components/` — ActorAttributes (add CommitmentTier, total_budget, scaling mode methods; preserve existing struct)
- `src/common/systems/combat/resources.rs` — Absolute derivation (extend existing)
- Combat event handlers — Relative contest resolution
- Queue/cadence/evasion systems — Commitment tier lookups

## References

- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [ADR-020: Super-Linear Level Multiplier](020-super-linear-level-multiplier.md)
- [ADR-027: Commitment Tiers](027-commitment-tiers.md)
- [ADR-029: Relative Stat Contests](029-relative-stat-contests.md)
- [ADR-005: Derived Combat Stats](005-derived-combat-stats.md)

## Date

2026-02-10
