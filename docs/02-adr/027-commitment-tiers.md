# ADR-027: Commitment Tiers (Discrete 20/40/60)

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

Commitment values scale in **discrete tiers** based on the **percentage of maximum possible** for that attribute. Three tiers above baseline, with thresholds at 20%, 40%, and 60%.

### Core Mechanism

**Tier Calculation:**

```rust
fn commitment_tier(derived_attribute: u16, total_level: u32) -> CommitmentTier {
    let max_possible = total_level * 10; // Maximum for any single attribute
    if max_possible == 0 {
        return CommitmentTier::T0;
    }
    let percentage = (derived_attribute as f64 / max_possible as f64) * 100.0;
    match percentage {
        p if p >= 60.0 => CommitmentTier::T3,
        p if p >= 40.0 => CommitmentTier::T2,
        p if p >= 20.0 => CommitmentTier::T1,
        _ => CommitmentTier::T0,
    }
}
```

**Attribute Formulas (axis×16, spectrum×12, shift×12):**
- **Axis side**: axis×16 + spectrum×12 ± shift×12
- **Opposite side**: ±shift×12 only (starts at 0)
- **Balanced (axis=0)**: spectrum×6 each side, no shift

**Tier Definitions:**

| Tier | Threshold | Meaning |
|------|-----------|---------|
| T0 | < 20% | No commitment identity — baseline only |
| T1 | ≥ 20% | Identity unlocked — noticeable specialization |
| T2 | ≥ 40% | Identity deepened — significant commitment |
| T3 | ≥ 60% | Identity defining — dominant aspect of build |

**Build Constraint Analysis (10 points, max=100):**

| Build Pattern | Values | Tiers | Total | Viable |
|---------------|--------|-------|-------|--------|
| Dual T3 (5+5 axis) | 80+80 | 80% each | 160 | ✅ |
| T3+2×T2 (4+3+3 axis) | 64+48+48 | T3+T2+T2 | 160 | ✅ |
| 4×T2 (0/5/0 + 0/5/0) | 30×4 | T1 each | 120 | ❌ (30% < 40%) |
| 5×T1 (0/4/0 + 0/4/0 + 2/0/0) | 24+24+24+24+32 | T1 each | 128 | ✅ |

The 20/40/60 thresholds create viable dual-T3 builds and smooth progression from specialist (160 total) to generalist (120 total).

**Concrete Commitment Stats (MVP):**

| Attribute | Commitment | Stat | T0 | T1 | T2 | T3 |
|-----------|-----------|------|----|----|----|----|
| Grace | Poise | Evasion | 0% | 10% | 20% | 30% |
| Focus | Concentration | Queue capacity | 1 slot | 2 slots | 3 slots | 4 slots |
| Presence | Intensity | Cadence | 3000ms | 2500ms | 2000ms | 1500ms |

Concentration slot counts follow ADR-021 pattern. Poise and Intensity values tuned for combat pacing.

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

**Why 20/40/60 thresholds:**
- Enables dual-T3 builds (80% each with 5+5 axis split)
- Creates smooth 160→120 stat progression (specialist to generalist)
- Lower T1 (20%) allows 5×T1 generalist builds
- Wider tiers (20% gaps) reduce cliff effects
- Matches 3:4 ratio of spectrum:axis multipliers (12:16)

**Why use total_level×10, not total_budget:**
- Prevents spectrum builds from being penalized in tier calculation
- Axis and spectrum builds with same points compare fairly (both to max=100 with 10 points)
- Creates smooth stat curve: pure axis (160) → hybrids (140-128) → pure spectrum (120)
- Opposite side starts at 0 (shift only) - rewards axis commitment while preserving spectrum flexibility

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
- `calculate_queue_capacity` → uses `commitment_tier_for(focus)` instead of `focus_reach / (total_level × 7)`
- Threshold mapping: T0→1 slot, T1→2, T2→3, T3→4 (same output, different input formula)
- Level-0 special case preserved (T0 → 1 slot)
- Tier calculation now uses max_possible (total_level × 10) instead of total_budget to fairly compare axis vs spectrum builds

**Shift Constraints:**
- Shift direction locked by axis: positive axis → negative shift only, negative axis → positive shift only
- Pure spectrum (axis=0) cannot shift - requires axis commitment for tactical redistribution
- Shift magnitude clamped to spectrum value

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
