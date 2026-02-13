# ADR-031: Relative Meta-Attribute Opposition System

## Status

Accepted

## Context

The attribute system decomposes each of the six attributes (Might, Grace, Vitality, Focus, Instinct, Presence) into three scaling modes:

- **Absolute** — progression metric with super-linear level scaling
- **Relative** — build benefit based on raw stat difference (no level scaling)
- **Commitment** — build identity via discrete tiers (% of total budget)

Each scaling mode has its own opposition map, distinct from the others:

- **Absolute opposition**: Might↔Grace, Vitality↔Focus, Instinct↔Presence
- **Relative opposition** (this ADR): Might↔Focus, Grace↔Instinct, Vitality↔Presence
- **Commitment opposition**: Might↔Presence, Grace↔Vitality, Instinct↔Focus

Rotating oppositions across layers means no single "counter-build" exists for any attribute. An entity must invest across multiple axes to oppose another on all three layers simultaneously, which is impossible under a fixed budget. This creates a richer build space than mirroring the same opposition at every level.

### Current Implementation

The existing relative meta-attributes were:

- `precision()` [Grace] ↔ `toughness()` [Vitality] - affects crit chance and mitigation
- `impact()` [Might] ↔ `composure()` [Focus] - mapped but not fully implemented
- `dominance()` [Presence] ↔ `cunning()` [Instinct] - mapped but not fully implemented

This was a transitional state from Phase 4 of SOW-020, which established the contest framework but did not complete the mechanical implementation.

### Problems

1. **Precision as relative stat**: Grace opposing Vitality created an awkward pairing where accuracy contested durability, which felt disconnected mechanically
2. **Critical hits**: Tied to Precision, but crit chance doesn't fit the "build benefit" philosophy of relative stats (it's more of an absolute progression stat)
3. **Incomplete mechanics**: Impact/Composure and Dominance/Cunning had placeholder implementations without full system integration
4. **Opposition mismatch**: The relative oppositions (Grace↔Vitality, Might↔Focus, Presence↔Instinct) didn't align with RFC-021's design intent for emergent opposition through independent stats

## Decision

We adopt the relative meta-attribute system from RFC-021 with the following implementation specifics:

### Three Relative Pairs

#### 1. Impact (Might) vs Composure (Focus) — Recovery Timeline

| Meta-Attribute | Stat | Mechanism |
|----------------|------|-----------|
| Impact | Recovery pushback | Each hit extends the target's recovery timer by 25% of the recovery's max duration, modified by contest |
| Composure | Recovery reduction | Passively reduces all recovery timers by a percentage based on Composure value |

**Contest formula**: Standard `contest_modifier(attacker_impact, defender_composure)` applies to pushback percentage.

**Implementation notes**:
- Pushback only applies when target has active `GlobalRecovery`
- Pushback percentage: `base_pushback = 0.25 * recovery.duration * contest_modifier(impact, composure)`
- Composure reduction: Passive tick rate modifier on `GlobalRecovery::tick()`

#### 2. Finesse (Grace) vs Cunning (Instinct) — Lockout vs Window

| Meta-Attribute | Stat | Mechanism |
|----------------|------|-----------|
| Finesse | Synergy recovery reduction | Reduces the recovery time between chained skills via contest |
| Cunning | Reaction window | Extends the time available to react to each incoming threat in the queue |

**Contest formula**: Standard `contest_modifier(attacker_finesse, defender_cunning)` modifies synergy unlock_reduction.

**Implementation notes**:
- Finesse modifies the `unlock_reduction` value in synergy rules: `effective_reduction = base_reduction * contest_modifier(finesse, cunning)`
- Cunning extends `QueuedThreat::timer_duration` when threat is inserted
- The lockout equation: `chain_gap + reaction_window > lockout` determines reactability

#### 3. Dominance (Presence) vs Toughness (Vitality) — Sustain Ratio

| Meta-Attribute | Stat | Mechanism |
|----------------|------|-----------|
| Dominance | Healing reduction | Reduces the effectiveness of incoming healing on all entities within 5 hex radius. Operates as an aura; when multiple Dominance sources overlap, only the strongest effect applies (worst-effect-wins, no stacking) |
| Toughness | Mitigation | Flat damage reduction per hit received (existing mechanic) |

**Contest formula**: Standard `contest_modifier(presence_dominance, target_toughness)` determines healing reduction.

**Implementation notes**:
- Dominance aura: 5 hex radius from entity, checked at heal time
- Healing reduction formula: `effective_healing = base_healing * (1.0 - healing_reduction_factor)` where factor comes from contest
- Worst-effect-wins: Query all Dominance auras in range, apply only the strongest (highest Dominance value)
- Toughness mitigation: Already implemented in `apply_passive_modifiers()`, no changes needed

### Critical Hit Removal

**Decision**: Remove the critical hit system entirely.

**Rationale**:
- Crit chance was tied to Precision (Grace), which is being removed
- Crit mechanics don't fit the "build benefit" philosophy of relative stats
- The RFC explicitly notes "may be cut" for crit chance
- Simpler damage calculation reduces randomness and improves combat readability

**Impact**:
- Remove `roll_critical()` function from `damage.rs`
- Remove crit-related UI elements
- Damage becomes more predictable and contest-driven

### Meta-Attribute Mapping Changes

**Before**:
```rust
pub fn precision(&self) -> u16 { self.grace() }
pub fn toughness(&self) -> u16 { self.vitality() }
pub fn impact(&self) -> u16 { self.might() }
pub fn composure(&self) -> u16 { self.focus() }
pub fn dominance(&self) -> u16 { self.presence() }
pub fn cunning(&self) -> u16 { self.instinct() }
```

**After**:
```rust
// precision() - REMOVED
pub fn finesse(&self) -> u16 { self.grace() }       // NEW
pub fn toughness(&self) -> u16 { self.vitality() }  // unchanged mapping
pub fn impact(&self) -> u16 { self.might() }        // unchanged mapping
pub fn composure(&self) -> u16 { self.focus() }     // unchanged mapping
pub fn dominance(&self) -> u16 { self.presence() }  // unchanged mapping
pub fn cunning(&self) -> u16 { self.instinct() }    // unchanged mapping
```

## Consequences

### Positive

1. **Emergent opposition**: Each pair creates opposition through independent stats affecting a shared mechanical layer, not through direct cancellation
2. **Build diversity**: Rotating oppositions across layers prevents single counter-builds
3. **Clear roles**: Each relative meta-attribute has a distinct mechanical purpose aligned with its base attribute
4. **Strategic depth**: Impact/Composure (tempo control), Finesse/Cunning (burst vs reaction), Dominance/Toughness (sustain pressure)

### Negative

1. **Breaking changes**: Removal of Precision and crit system affects existing combat balance
2. **New systems required**: Minimal healing system must be implemented for Dominance to function
3. **Complexity**: Three distinct mechanical layers (recovery, synergy/reaction, sustain) must all work together
4. **Testing burden**: Each pair requires independent testing plus integration tests for all three

### Migration Impact

**Code changes**:
- Remove: `precision()`, `roll_critical()`, crit-related UI
- Add: `finesse()`, healing system foundation, Dominance aura system
- Modify: `GlobalRecovery` (pushback/reduction), synergies (Finesse scaling), damage pipeline (no crits)

**Data migration**: None required (attribute investment structure unchanged)

## Implementation

This will be implemented in three phases via SOW-021:

1. **Phase 1**: Impact/Composure (recovery timeline) + crit removal
2. **Phase 2**: Finesse/Cunning (lockout vs window)
3. **Phase 3**: Dominance/Toughness (sustain ratio) + minimal healing

See SOW-021 for detailed implementation breakdown.

## References

- RFC-021: Relative Meta-Attribute Opposition System
- SOW-020: Attribute System Rework (Phase 4 established contest framework)
- ADR-029: Relative Stat Contests (contest_modifier formula)
- ADR-030: Reaction Queue Window Mechanic (Cunning foundation)
- ADR-012: Universal Lockout (recovery system)
- ADR-003: Reaction Queue (threat queueing)
