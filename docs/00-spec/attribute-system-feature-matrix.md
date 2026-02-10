# Attribute System - Feature Matrix

> **Note:** This feature matrix tracks the **v2.0 attribute system** (RFC-020: three scaling modes layered on existing bipolar Axis/Spectrum/Shift model, commitment tiers). The existing A/S/S input model is preserved and extended.

**Specification:** [attribute-system.md](attribute-system.md) (v2.0)
**Last Updated:** 2026-02-10
**Overall Status:** 2/28 features complete (7% — foundational only, pre-rework)

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Scaling Mode Foundation

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| CommitmentTier enum (T0/T1/T2/T3) | ❌ Not Started | [ADR-027](../02-adr/027-commitment-tiers.md) | 30/45/60% thresholds (SOW-020 Phase 1) |
| total_budget() calculation | ❌ Not Started | [ADR-026](../02-adr/026-three-scaling-modes.md) | Sum of all six derived values (SOW-020 Phase 1) |
| Attribute-Triumvirate decoupling | ❌ Not Started | [ADR-028](../02-adr/028-attribute-triumvirate-decoupling.md) | Remove archetype-attribute mapping (SOW-020 Phase 5) |

**Category Status:** 0/3 complete (0%)

---

### Absolute Stats (Progression)

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Force (Might absolute) → Damage | 🚧 Partial | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | Damage exists but uses old attribute model |
| Constitution (Vitality absolute) → HP | 🚧 Partial | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | HP exists but uses old attribute model |
| Technique (Grace absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Discipline (Focus absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Intuition (Instinct absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Gravitas (Presence absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Super-linear level multiplier | ✅ Complete | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | Polynomial multiplier implemented |

**Category Status:** 1/7 complete (14%) — multiplier exists, needs rewiring to new model

---

### Relative Stats (Build Benefit)

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Precision (Grace) vs Toughness (Vitality) | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | Crit chance vs mitigation |
| Impact (Might) vs Composure (Focus) | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | Impact is open; Composure = recovery reduction |
| Dominance (Presence) vs Cunning (Instinct) | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | Recovery pushback vs reaction window |
| Contest resolution function | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | f(attacker - defender) → modifier |

**Category Status:** 0/4 complete (0%)

---

### Commitment Stats (Build Identity)

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Poise (Grace commitment) → Evasion | ❌ Not Started | [ADR-027](../02-adr/027-commitment-tiers.md) | Tier-based evasion chance |
| Concentration (Focus commitment) → Queue capacity | 🚧 Partial | [ADR-021](../02-adr/021-commitment-ratio-queue-capacity.md), [ADR-027](../02-adr/027-commitment-tiers.md) | ADR-021 commitment ratio exists; needs migration to tier system |
| Intensity (Presence commitment) → Cadence | ❌ Not Started | [ADR-027](../02-adr/027-commitment-tiers.md) | Tier-based auto-attack speed |
| Ferocity (Might commitment) | ❌ Not Started | — | Open — no concrete stat mapped |
| Grit (Vitality commitment) | ❌ Not Started | — | Open — no concrete stat mapped |
| Flow (Instinct commitment) | ❌ Not Started | — | Open — no concrete stat mapped |

**Category Status:** 0/6 complete (0%) — ADR-021 partial exists

---

### Combat System Integration

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Queue capacity via Concentration tier | 🚧 Partial | [ADR-021](../02-adr/021-commitment-ratio-queue-capacity.md) | Commitment ratio exists, needs tier migration |
| Reaction window via Cunning vs Dominance | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | Currently level-gap only (ADR-020) |
| Recovery pushback via Dominance | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | New mechanic |
| Recovery reduction via Composure | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | New mechanic |
| Crit via Precision vs Toughness | ❌ Not Started | [ADR-029](../02-adr/029-relative-stat-contests.md) | New mechanic |
| Cadence via Intensity tier | ❌ Not Started | [ADR-027](../02-adr/027-commitment-tiers.md) | Currently fixed auto-attack interval |
| Evasion via Poise tier | ❌ Not Started | [ADR-027](../02-adr/027-commitment-tiers.md) | New mechanic |
| Dismiss + Precision/Toughness | ❌ Not Started | [ADR-022](../02-adr/022-dismiss-mechanic.md), [ADR-029](../02-adr/029-relative-stat-contests.md) | Crit on dismissed threats |
| Movement speed via Grace | ✅ Complete | Various | Grace-based movement speed formula exists |

**Category Status:** 1/9 complete (11%)

---

## Implementation Deviations

### v1.0 → v2.0 Extension

The v2.0 system **extends** the v1.0 bipolar model by layering three scaling modes on top. The following v1.0 features are **preserved**:

- Bipolar attribute pairs (Might ↔ Grace, Vitality ↔ Focus, Instinct ↔ Presence) — retained as input model
- Axis/Spectrum/Shift mechanics — retained; derived values feed scaling modes
- Character panel with shift drag — retained unchanged

The following v1.0 features are **superseded** or deferred:

- Approach/Resilience attribute leanings — superseded by decoupling (ADR-028)
- Prestige redistribution system — deferred to future RFC
- Overclock mechanic — deferred to future RFC
- 18-stat derived stat table — replaced by three scaling modes with named sub-attributes

---

## Spec Gaps

### Critical for Full Combat System
- **Scaling Mode Foundation:** CommitmentTier enum + total_budget() on existing bipolar model (SOW-020 Phase 1)
- **Commitment Tier Calculation:** 30/45/60% budget thresholds (SOW-020 Phase 1)
- **Relative Stat Contest Framework:** Three opposing pairs (SOW-020 Phase 4)
- **Commitment-Driven Stats:** Queue capacity, cadence, evasion from tiers (SOW-020 Phase 3)

### Medium Priority
- **Absolute Stat Rewiring:** Force/Constitution through new model (SOW-020 Phase 2)
- **Decoupling Migration:** Remove archetype-attribute coupling (SOW-020 Phase 5)

### Low Priority (Post-Launch)
- **Open Stats:** Technique, Discipline, Intuition, Gravitas, Impact, Ferocity, Grit, Flow
- **Equipment Attribute Modifiers:** Separate RFC
- **Commitment Tier Tuning:** Specific values for Poise/Intensity breakpoints

---

## Progress Summary

**Scaling Mode Foundation:** 0/3 features (0%)
- CommitmentTier + total_budget pending (SOW-020 Phase 1)

**Absolute Stats:** 1/7 features (14%)
- Level multiplier implemented (ADR-020)
- Damage and HP exist but need rewiring

**Relative Stats:** 0/4 features (0%)
- Contest framework not yet built

**Commitment Stats:** 0/6 features (0%)
- ADR-021 queue capacity exists as predecessor

**Combat Integration:** 1/9 features (11%)
- Movement speed exists

**Total Attribute System (v2.0):** 2/28 features complete (7%)

---

## Next Priorities

Based on SOW-020 phase ordering:

1. **Phase 1: Scaling Mode Foundation** — CommitmentTier enum, total_budget() on existing bipolar model
2. **Phase 2: Absolute Stats** — Rewire HP/damage/movement through level multiplier
3. **Phase 3: Commitment Stats** — Queue capacity, cadence, evasion from tiers
4. **Phase 4: Relative Contests** — Precision/Toughness, Dominance/Cunning, Impact/Composure
5. **Phase 5: Decoupling** — Remove archetype coupling, migrate NPCs, cleanup

---

**Document Version:** 2.0
**Maintained By:** Development team
**Review Cadence:** Update after each SOW-020 phase completion
