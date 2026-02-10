# ADR-027: Commitment Tiers (Discrete 30/45/60)

## Status

Proposed - 2026-02-10

## Context

**Related RFC:** [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)

The commitment scaling mode (ADR-026) needs a concrete mechanism for determining build identity from attribute investment. Two approaches are possible: continuous scaling (linear benefit per percentage point) or discrete tiers (breakpoints that unlock identity features).

The existing commitment-ratio queue capacity system (ADR-021) uses thresholds at 33/50/66% to determine queue slots. This RFC generalizes that pattern across all six attributes, with adjusted thresholds at 30/45/60%.

**References:**
- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [ADR-026: Three Scaling Modes](026-three-scaling-modes.md) — Commitment as one of three modes
- [ADR-021: Commitment-Ratio Queue Capacity](021-commitment-ratio-queue-capacity.md) — Predecessor (Focus-specific thresholds)

## Decision

Commitment values scale in **discrete tiers** based on the **percentage of total attribute budget** invested in that attribute. Three tiers above baseline, with thresholds at 30%, 45%, and 60%.

### Core Mechanism

**Tier Calculation:**

```rust
fn commitment_tier(derived_attribute: f32, total_budget: f32) -> CommitmentTier {
    if total_budget == 0.0 {
        return CommitmentTier::T0;
    }
    let percentage = derived_attribute / total_budget;
    match percentage {
        p if p >= 0.60 => CommitmentTier::T3,
        p if p >= 0.45 => CommitmentTier::T2,
        p if p >= 0.30 => CommitmentTier::T1,
        _ => CommitmentTier::T0,
    }
}
```

**Tier Definitions:**

| Tier | Threshold | Meaning |
|------|-----------|---------|
| T0 | < 30% | No commitment identity — baseline only |
| T1 | ≥ 30% | Identity unlocked — noticeable specialization |
| T2 | ≥ 45% | Identity deepened — significant commitment |
| T3 | ≥ 60% | Identity defining — dominant aspect of build |

**Budget Constraint Analysis (100% across 6 attributes):**

| Build Pattern | Investment | Remaining | Viable |
|---------------|-----------|-----------|--------|
| Specialist | T3 (60%) + T1 (30%) | 10% | ✅ |
| Dual identity | T2 (45%) + T2 (45%) | 10% | ✅ |
| Generalist | T1 (30%) + T1 (30%) + T1 (30%) | 10% | ✅ |
| T3 + T2 | 60% + 45% | -5% | ❌ Impossible |
| Dual T3 | 60% + 60% | -20% | ❌ Impossible |
| Quad T1 | 30% × 4 | -20% | ❌ Impossible |

The 30/45/60 thresholds allow 10% wiggle room in all viable builds. This buffer means players don't need perfectly optimal splits to hit breakpoints.

**Concrete Commitment Stats (MVP):**

| Attribute | Commitment | Stat | T0 | T1 | T2 | T3 |
|-----------|-----------|------|----|----|----|----|
| Grace | Poise | Evasion | None | Low | Moderate | High |
| Focus | Concentration | Queue capacity | 1 slot | 2 slots | 3 slots | 4 slots |
| Presence | Intensity | Cadence | Slow | Moderate | Fast | Rapid |

Specific numeric values for Poise and Intensity are tuning knobs to be determined through playtesting. Concentration slot counts follow the existing ADR-021 pattern.

**Open Commitment Stats (no concrete mechanic yet):**

| Attribute | Commitment | Status |
|-----------|-----------|--------|
| Might | Ferocity | Open — assigned when gameplay testing reveals need |
| Vitality | Grit | Open — assigned when gameplay testing reveals need |
| Instinct | Flow | Open — assigned when gameplay testing reveals need |

## Rationale

**Why discrete tiers, not continuous:**
- Queue capacity is inherently discrete (can't have 2.7 slots) — Concentration already needs tiers
- Clear player-facing goals ("I need 45% Focus for 3 queue slots")
- Simpler to balance (three tier values per stat vs infinite continuous curve)
- Creates meaningful identity boundaries (you either "are" a Might fighter or you're not)
- UI representation is cleaner (tier badges vs percentage bars)

**Why 30/45/60 thresholds, not 33/50/66:**
- 10% buffer in all viable builds (T3+T1 = 90%, not 99%)
- More forgiving investment math (players don't need exact splits)
- Three viable archetypes (specialist, dual, generalist) all have breathing room
- Lower T1 threshold (30% vs 33%) makes generalist triple-T1 build achievable

**Why generalize ADR-021 rather than keep Focus-specific:**
- Same mathematical pattern applies to all six attributes
- Unified system is simpler to understand (one tier mechanic, not six different mechanics)
- ADR-021's 33/50/66 thresholds shift to 30/45/60 — slot mapping stays the same (4 tiers → 4 slot counts)
- Equipment that modifies commitment tier uses the same system for all attributes

**Why T0 exists (below 30%):**
- Most attributes will be T0 in any viable build (specialist has 4-5 T0 attributes)
- T0 provides baseline functionality (1 queue slot, basic auto-attack speed, no evasion)
- Clear distinction between "invested" and "not invested" aids build readability

## Consequences

**Positive:**
- Budget math is simple and memorable (30/45/60 are easy numbers)
- Three viable build archetypes emerge naturally from constraints
- Clear breakpoints for player decision-making
- Unified tier system across all six attributes
- Easy to balance (adjust three values per stat instead of continuous curves)

**Negative:**
- Cliff effects at tier boundaries (29.9% vs 30.0% is binary)
- Three open commitment stats have no mechanical effect yet
- Tier thresholds are fixed — if future content needs 4+ tiers, system must change

**Mitigations:**
- Cliff effects are intentional — clear breakpoints aid decision-making
- Open stats are explicitly documented; fill when testing reveals needs
- 30/45/60 thresholds are constants, easily adjustable
- System can accommodate a T4 at higher threshold if needed (though budget math makes it very constraining)

## Implementation Notes

**Enum Definition:**
```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommitmentTier { T0, T1, T2, T3 }
```

**Computation Timing:**
- Calculated at entity spawn, level-up, or attribute respec
- Cached as component data (not recomputed per frame)
- Equipment modifiers may shift effective percentage (future)

**Migration from ADR-021:**
- `calculate_queue_capacity` → uses `commitment_tier(focus, total_budget)` instead of `focus_reach / (total_level × 7)`
- Threshold mapping: T0→1 slot, T1→2, T2→3, T3→4 (same output, different input formula)
- Level-0 special case preserved (T0 → 1 slot)

**Files Affected:**
- `src/common/components/` — CommitmentTier enum, cached tier per attribute
- `src/common/systems/combat/queue.rs` — Queue capacity from Focus commitment tier
- Cadence system — Auto-attack speed from Presence commitment tier
- Evasion system — Dodge chance from Grace commitment tier

## References

- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [ADR-026: Three Scaling Modes](026-three-scaling-modes.md)
- [ADR-021: Commitment-Ratio Queue Capacity](021-commitment-ratio-queue-capacity.md)

## Date

2026-02-10
