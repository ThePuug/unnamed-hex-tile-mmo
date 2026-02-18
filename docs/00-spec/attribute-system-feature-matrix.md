# Attribute System - Feature Matrix

> **Note:** This feature matrix tracks the **v2.1 attribute system** (RFC-020 + RFC-021: three scaling modes with rotated relative oppositions). The existing A/S/S input model is preserved and extended.

**Specification:** [attribute-system.md](attribute-system.md) (v2.1)
**Last Updated:** 2026-02-13
**Overall Status:** 11/28 features complete (39% — SOW-020 Phase 4 superseded by SOW-021)

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
| Force (Might absolute) → Offensive Damage | ✅ Complete | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | All offensive abilities scale with Force: Lunge (100%), Overpower (150%), AutoAttack (50%) |
| Constitution (Vitality absolute) → HP | ✅ Complete | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | max_health() uses vitality() with shift sensitivity (SOW-020 Phase 2) |
| Technique (Grace absolute) → Defensive Damage | ✅ Complete | — | Counter reflected damage scales with Technique (20% base + 30% threat, cap 200%) |
| Discipline (Focus absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Intuition (Instinct absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Gravitas (Presence absolute) | ❌ Not Started | — | Open — no concrete stat mapped |
| Super-linear level multiplier | ✅ Complete | [ADR-020](../02-adr/020-super-linear-level-multiplier.md) | Polynomial multiplier implemented |

**Category Status:** 4/7 complete (57%) — Force, Technique, Constitution, level multiplier all complete

---

### Relative Stats (Build Benefit)

**Note:** RFC-021 + ADR-031 reworked relative oppositions with rotated pairings. SOW-020 Phase 4 implementation superseded by SOW-021.

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Impact (Might) vs Composure (Focus) — Recovery timeline | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Impact: recovery pushback (25% of max × contest). Composure: recovery reduction (passive tick rate). SOW-021 Phase 1 |
| Finesse (Grace) vs Cunning (Instinct) — Lockout-vs-window | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Finesse: synergy reduction via contest. Cunning: reaction window extension (2ms/point, 600ms cap). SOW-021 Phase 2 |
| Dominance (Presence) vs Toughness (Vitality) — Sustain ratio | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Dominance: healing reduction aura (5 hex, worst-wins). Toughness: damage mitigation. SOW-021 Phase 3 |
| Contest resolution function | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | contest_modifier(): clamped linear [0.5, 1.5], K=200 (reused for all pairs) |
| ~~Precision (Grace) — Critical hits~~ | ⏸️ **Removed** | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Critical hit system removed entirely. Damage now deterministic and contest-driven. |

**Category Status:** 1/4 complete (25%) — Contest function preserved, relative pairs being reworked

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

**Note:** SOW-021 replaces SOW-020 Phase 4 relative stat integrations. Some features deprecated.

| Feature | Status | ADR/Impl | Notes |
|---------|--------|----------|-------|
| Queue capacity via Concentration tier | ✅ Complete | [ADR-021](../02-adr/021-commitment-ratio-queue-capacity.md) | Migrated to commitment tier (SOW-020 Phase 3) |
| Reaction window via Cunning | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | timer_duration + (cunning × 2ms), capped at 600ms (SOW-021 Phase 2) |
| Recovery pushback via Impact | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | 25% of recovery.duration × contest_modifier (SOW-021 Phase 1) |
| Recovery reduction via Composure | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Passive tick rate modifier in global_recovery_system (SOW-021 Phase 1) |
| Synergy reduction via Finesse vs Cunning | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | unlock_reduction × contest_modifier (SOW-021 Phase 2) |
| Healing reduction via Dominance | ❌ Not Started | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Aura: 5 hex radius, worst-effect-wins, contests Toughness (SOW-021 Phase 3) |
| Mitigation via Toughness | ✅ Complete | [ADR-029](../02-adr/029-relative-stat-contests.md) | Flat damage reduction per hit (already implemented) |
| Cadence via Intensity tier | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | Replaces fixed 1500ms cooldown (SOW-020 Phase 3) |
| Evasion via Poise tier | ✅ Complete | [ADR-027](../02-adr/027-commitment-tiers.md) | Grace tier dodge at threat insertion (SOW-020 Phase 3) |
| ~~Crit via Precision vs Toughness~~ | ⏸️ **Removed** | [ADR-031](../02-adr/031-relative-meta-attributes-rework.md) | Critical hit system removed (ADR-031) |
| Movement speed via Grace | ✅ Complete | Various | Grace-based movement speed formula exists |

**Category Status:** 4/10 complete (40%) — Commitment integrations preserved, relative integrations being reworked

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
- **Decoupling Migration:** Remove archetype-attribute coupling (SOW-020 Phase 5)

### Low Priority (Post-Launch)
- **Open Stats:** Discipline, Intuition, Gravitas, Ferocity, Grit, Flow
- **Equipment Attribute Modifiers:** Separate RFC
- **Commitment Tier Tuning:** Specific values for Poise/Intensity breakpoints
- **Healing System Expansion:** Minimal healing in SOW-021 Phase 3, full system separate RFC

---

## Progress Summary

**Scaling Mode Foundation:** 2/3 features (67%)
- CommitmentTier + total_budget complete (SOW-020 Phase 1)
- Decoupling remains (Phase 5)

**Absolute Stats:** 2/7 features (29%)
- Level multiplier implemented (ADR-020)
- HP rewired to vitality() with shift sensitivity (SOW-020 Phase 2)
- Damage partially rewired

**Relative Stats:** 1/4 features (25%) — **Rework in progress (RFC-021, SOW-021)**
- Contest resolution function preserved: clamped linear [0.5, 1.5], K=200
- Impact vs Composure (recovery timeline): SOW-021 Phase 1 (not started)
- Finesse vs Cunning (lockout-vs-window): SOW-021 Phase 2 (not started)
- Dominance vs Toughness (sustain ratio): SOW-021 Phase 3 (not started)
- ~~Precision (crit chance)~~: Removed in ADR-031

**Commitment Stats:** 3/6 features (50%)
- Queue capacity via Focus tier (SOW-020 Phase 3)
- Cadence via Presence tier (SOW-020 Phase 3)
- Evasion via Grace tier (SOW-020 Phase 3)

**Combat Integration:** 4/10 features (40%)
- Movement speed, queue capacity, cadence, evasion, mitigation complete (commitment + existing)
- Relative stat integrations being reworked via SOW-021 (Impact/Composure, Finesse/Cunning, Dominance/Toughness)

**Total Attribute System (v2.1):** 11/28 features complete (39% — SOW-020 Phase 4 superseded, SOW-021 in planning)

---

## Next Priorities

**SOW-021: Relative Meta-Attributes Implementation** (supersedes SOW-020 Phase 4):

1. ~~**Phase 1: Scaling Mode Foundation**~~ ✅ (SOW-020)
2. ~~**Phase 2: Absolute Stats**~~ ✅ (SOW-020 — HP rewired; damage partial)
3. ~~**Phase 3: Commitment Stats**~~ ✅ (SOW-020 — queue capacity, cadence, evasion)
4. ~~**Phase 4: Relative Contests**~~ ⏸️ **Superseded by RFC-021** (old implementation deprecated)
5. **SOW-021 Phase 1:** Impact/Composure (recovery timeline) + crit removal — **Next priority**
6. **SOW-021 Phase 2:** Finesse/Cunning (lockout-vs-window)
7. **SOW-021 Phase 3:** Dominance/Toughness (sustain ratio) + minimal healing
8. **SOW-020 Phase 5:** Decoupling — Remove archetype coupling, migrate NPCs, cleanup

---

**Document Version:** 2.1
**Maintained By:** Development team
**Review Cadence:** Update after each SOW-021 phase completion
