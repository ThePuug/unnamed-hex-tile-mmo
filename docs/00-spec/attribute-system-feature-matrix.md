# Attribute System - Feature Matrix

> **Note:** This feature matrix tracks the **v2.0 attribute system** (RFC-020: three scaling modes layered on existing bipolar Axis/Spectrum/Shift model, commitment tiers). The existing A/S/S input model is preserved and extended.

**Specification:** [attribute-system.md](attribute-system.md) (v2.0)
**Last Updated:** 2026-02-10
**Overall Status:** 12/28 features complete (43% — Phases 1–4 complete)

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
| CommitmentTier enum (T0/T1/T2/T3) | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | 20/40/60% thresholds, max=level×10 (SOW-020 Phase 1) |
| Attribute formulas (axis×16, spectrum×12) | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | Smooth 160→120 progression, shift constraints (SOW-020 Phase 1) |
| Attribute-Triumvirate decoupling | ❌ Not Started | [ADR-028](../02-adr/028-attribute-triumvirate-decoupling.md) | Remove archetype-attribute mapping (SOW-020 Phase 5) |

**Category Status:** 2/3 complete (67%)

---

### Absolute Stats (Progression)

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Force (Might absolute) → Damage | 🚧 Partial | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | Damage exists, uses might() scaling |
| Constitution (Vitality absolute) → HP | ✅ Complete | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | max_health() uses vitality() with shift sensitivity (SOW-020 Phase 2) |
| Technique (Grace absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Discipline (Focus absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Intuition (Instinct absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Gravitas (Presence absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Super-linear level multiplier | ✅ Complete | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | Polynomial multiplier implemented |

**Category Status:** 2/7 complete (29%) — HP rewired via Phase 2, damage partially rewired

---

### Relative Stats (Build Benefit)

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Precision (Grace) vs Toughness (Vitality) | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | Crit chance × precision_mod, mitigation × 1/precision_mod (SOW-020 Phase 4) |
| Impact (Might) vs Composure (Focus) | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | Impact passively increases pushback; Composure reduces it (SOW-020 Phase 4) |
| Dominance (Presence) vs Cunning (Instinct) | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | Reaction window × 1/tempo_mod, recovery pushback × tempo_mod (SOW-020 Phase 4) |
| Contest resolution function | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | contest_modifier(): clamped linear [0.5, 1.5], K=200 (SOW-020 Phase 4) |

**Category Status:** 4/4 complete (100%)

---

### Commitment Stats (Build Identity)

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Poise (Grace commitment) → Evasion | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | evasion_chance() — T0→0%, T1→10%, T2→20%, T3→30% (SOW-020 Phase 3) |
| Concentration (Focus commitment) → Queue capacity | ✅ Complete | [ADR-021](../02-adr/021-commitment-ratio-queue-capacity.md), [ADR-027](../02-adr/027-commitment-tiers.md) | calculate_queue_capacity() uses commitment tier (SOW-020 Phase 3) |
| Intensity (Presence commitment) → Cadence | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | cadence_interval() — T0→2s, T1→1.5s, T2→1s, T3→750ms (SOW-020 Phase 3) |
| Ferocity (Might commitment) | ❌ Not Started | — | Open — no concrete stat mapped |
| Grit (Vitality commitment) | ❌ Not Started | — | Open — no concrete stat mapped |
| Flow (Instinct commitment) | ❌ Not Started | — | Open — no concrete stat mapped |

**Category Status:** 3/6 complete (50%)

---

### Combat System Integration

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Queue capacity via Concentration tier | ✅ Complete | [ADR-021](../02-adr/021-commitment-ratio-queue-capacity.md) | Migrated to commitment tier (SOW-020 Phase 3) |
| Reaction window via Cunning vs Dominance | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | timer_duration × 1/tempo_mod (SOW-020 Phase 4) |
| Recovery pushback via Dominance | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | BASE_PUSHBACK (250ms) × tempo_mod per threat (SOW-020 Phase 4) |
| Recovery reduction via Composure | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | Pushback × 1/composure_mod (SOW-020 Phase 4) |
| Crit via Precision vs Toughness | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | crit_chance × precision_mod (SOW-020 Phase 4) |
| Cadence via Intensity tier | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | Replaces fixed 1500ms cooldown (SOW-020 Phase 3) |
| Evasion via Poise tier | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | Grace tier dodge at threat insertion (SOW-020 Phase 3) |
| Dismiss + Precision/Toughness | 🚧 Partial | [ADR-022](../02-adr/022-dismiss-mechanic.md), [ADR-029](../02-adr/029-relative-stat-contests.md) | precision_mod stored in QueuedThreat, available at resolution; dismiss mechanic not yet implemented |
| Movement speed via Grace | ✅ Complete | Various | Grace-based movement speed formula exists |

**Category Status:** 8/9 complete (89%)

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
- ~~**Relative Stat Contest Framework:** Three opposing pairs (SOW-020 Phase 4)~~ ✅ Complete

### Medium Priority
- **Absolute Stat Rewiring:** Force (damage) still partial — needs full rewiring (SOW-020 Phase 2 remainder)
- **Decoupling Migration:** Remove archetype-attribute coupling (SOW-020 Phase 5)

### Low Priority (Post-Launch)
- **Open Stats:** Technique, Discipline, Intuition, Gravitas, Impact, Ferocity, Grit, Flow
- **Equipment Attribute Modifiers:** Separate RFC
- **Commitment Tier Tuning:** Specific values for Poise/Intensity breakpoints

---

## Progress Summary

**Scaling Mode Foundation:** 2/3 features (67%)
- CommitmentTier + total_budget complete (SOW-020 Phase 1)
- Decoupling remains (Phase 5)

**Absolute Stats:** 2/7 features (29%)
- Level multiplier implemented (ADR-020)
- HP rewired to vitality() with shift sensitivity (SOW-020 Phase 2)
- Damage partially rewired

**Relative Stats:** 4/4 features (100%)
- Contest resolution function: clamped linear [0.5, 1.5], K=200 (SOW-020 Phase 4)
- Precision vs Toughness: crit chance and mitigation scaling (SOW-020 Phase 4)
- Dominance vs Cunning: reaction window and recovery pushback (SOW-020 Phase 4)
- Impact vs Composure: recovery pushback reduction (SOW-020 Phase 4)

**Commitment Stats:** 3/6 features (50%)
- Queue capacity via Focus tier (SOW-020 Phase 3)
- Cadence via Presence tier (SOW-020 Phase 3)
- Evasion via Grace tier (SOW-020 Phase 3)

**Combat Integration:** 8/9 features (89%)
- Movement speed, queue capacity, cadence, evasion, reaction window, recovery pushback, recovery reduction, crit all wired
- Dismiss + Precision/Toughness partial (precision_mod stored, dismiss mechanic not yet implemented)

**Total Attribute System (v2.0):** 12/28 features complete (43% — through Phase 4, open stats deferred)

---

## Next Priorities

Based on SOW-020 phase ordering (Phases 1–4 complete):

1. ~~**Phase 1: Scaling Mode Foundation**~~ ✅
2. ~~**Phase 2: Absolute Stats**~~ ✅ (HP rewired; damage partial)
3. ~~**Phase 3: Commitment Stats**~~ ✅ (queue capacity, cadence, evasion)
4. ~~**Phase 4: Relative Contests**~~ ✅ (all 3 pairs + contest function)
5. **Phase 5: Decoupling** — Remove archetype coupling, migrate NPCs, cleanup

---

**Document Version:** 2.0
**Maintained By:** Development team
**Review Cadence:** Update after each SOW-020 phase completion
