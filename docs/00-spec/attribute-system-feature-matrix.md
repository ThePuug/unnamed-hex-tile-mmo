# Attribute System - Feature Matrix

**Specification:** [attribute-system.md](attribute-system.md)
**Last Updated:** 2026-02-09
**Overall Status:** 2/27 features complete (7% - foundational only)

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Core Attribute Pairs

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Might ↔ Grace pair | 🚧 Partial | Various ADRs | Lines 17-20 | Attributes exist, sliding scale not implemented |
| Vitality ↔ Focus pair | 🚧 Partial | Various ADRs | Lines 22-25 | Attributes exist, sliding scale not implemented |
| Instinct ↔ Presence pair | 🚧 Partial | Various ADRs | Lines 27-30 | Attributes exist, sliding scale not implemented |

**Category Status:** 0/3 complete (partial foundation exists)

---

### Derived Stats

**Note:** All derived stats are currently linear. [RFC-017](../01-rfc/017-combat-balance-overhaul.md) / [ADR-020](../02-adr/020-super-linear-level-multiplier.md) proposes super-linear polynomial multiplier applied after existing linear formulas.

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Might → Physical Damage | ✅ Complete | [ADR-005](../adr/005-damage-pipeline.md) | Line 40 | Implemented in damage calc. **Planned:** Super-linear multiplier (ADR-020) |
| Might → Stagger Multiplier | ❌ Not Started | - | Line 40 | Stagger system deferred |
| Might → Stamina Pool | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Line 40 | Resource scaling implemented |
| Grace → Movement Speed | ❌ Not Started | - | Lines 41, 338-347 | Formula defined, not implemented |
| Grace → Hit Chance | ❌ Not Started | - | Line 41 | Attack accuracy system |
| Grace → Dodge Recovery | ❌ Not Started | - | Line 41 | Dodge ability not in MVP |
| Vitality → Health Pool | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Line 42 | Resource scaling implemented. **Planned:** Super-linear multiplier (ADR-020) |
| Vitality → Stagger Resist | ⏸️ Deferred | - | Line 42 | Stagger system deferred |
| Vitality → DoT Resistance | ❌ Not Started | - | Line 42 | Status effects not implemented |
| Focus → Mana Pool | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Line 43 | Resource scaling implemented. **Planned:** Queue capacity via commitment ratio (ADR-021) |
| Focus → Magic Damage | 🚧 Partial | [ADR-005](../adr/005-damage-pipeline.md) | Line 43 | Formula exists, no magic abilities |
| Focus → Resist Recovery | ❌ Not Started | - | Line 43 | Recovery mechanics |
| Instinct → Crit Chance | ⏸️ Deferred | - | Line 44 | Crit system deferred |
| Instinct → Physical Penetration | ❌ Not Started | - | Line 44 | Armor penetration |
| Instinct → Parry Recovery | ⏸️ Deferred | - | Line 44 | Parry ability post-MVP |
| Presence → Threat Generation | ❌ Not Started | - | Line 45 | Aggro/threat system |
| Presence → AoE Multiplier | ❌ Not Started | - | Line 45 | AoE abilities post-MVP |
| Presence → CC Duration | ❌ Not Started | - | Line 45 | Crowd control system |

**Category Status:** 4/18 complete (22%)

---

### Axis & Spectrum Mechanics

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Axis (permanent center) | ❌ Not Started | - | Lines 66-69 | Core progression mechanic |
| Spectrum (adjustment range) | ❌ Not Started | - | Lines 71-74 | Tactical flexibility |
| Shift (current adjustment) | ❌ Not Started | - | Lines 76-78 | Pre-encounter positioning |
| Position calculation formulas | ❌ Not Started | - | Lines 80-100 | Left/right reach math |
| Scrollbar UI visualization | ❌ Not Started | - | Lines 108-134 | Visual metaphor |

**Category Status:** 0/5 complete (0%)

---

### Progression Systems

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Starting position (all 0) | ❌ Not Started | - | Lines 140-148 | New player state |
| Level 1-50 investment | ❌ Not Started | - | Lines 152-194 | +2% Axis OR +1% Spectrum per level |
| Prestige redistribution | ❌ Not Started | - | Lines 198-242 | Level 51+ respec actions |
| Prestige banking | ❌ Not Started | - | Line 219 | Limited banking (TBD amount) |

**Category Status:** 0/4 complete (0%)

---

### Advanced Mechanics

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Overclock (>100%) | ⏸️ Deferred | - | Lines 246-258 | Risk/reward for extreme values |
| Reach skills (max stat) | ❌ Not Started | - | Lines 318-329 | Ultimate abilities scale from reach |

**Category Status:** 0/2 complete (0%)

---

### Triumvirate Integration

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Approach attribute leanings | ❌ Not Started | - | Lines 268-278 | Primary/secondary/tertiary |
| Resilience attribute leanings | ❌ Not Started | - | Lines 282-290 | Primary/secondary/tertiary |
| Signature skill scaling | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | Lines 303-315 | MVP abilities scale, but no axis/spectrum |

**Category Status:** 0/3 complete (0%)

---

## Implementation Deviations

Currently no deviations - system is mostly unimplemented. MVP combat uses **simplified fixed attributes** rather than the full Axis/Spectrum/Shift system.

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### Critical for Full Combat System
- **Axis/Spectrum/Shift Core Mechanics:** Entire sliding scale system (Lines 62-134)
- **Level 1-50 Investment System:** Attribute point progression (Lines 152-194)
- **Scrollbar UI:** Visual representation of attributes (Lines 108-134)

### Medium Priority
- **Derived Stat Implementations:** Movement speed, hit chance, threat, etc. (Lines 338-349)
- **Triumvirate Attribute Integration:** Approach/Resilience leanings (Lines 262-329)
- **Prestige Redistribution:** Respec system for level 51+ (Lines 198-242)

### Low Priority (Post-Launch)
- **Overclock Mechanics:** Risk/reward for >100% attributes (Lines 246-258)
- **Reach Skills:** Ultimate abilities using max potential (Lines 318-329)
- **Prestige Banking:** Limited respec point storage (Line 219)

---

## Progress Summary

**Foundation (Basic Attributes):** 4/18 derived stats implemented (22%)
- Resource pools (stamina, health, mana): ✅ Complete
- Damage scaling (physical, magic): ✅ Complete
- All other derived stats: ❌ Not Started

**Core System (Axis/Spectrum/Shift):** 0/12 features implemented (0%)
- Entire sliding scale system not yet built

**Progression (Leveling/Prestige):** 0/4 features implemented (0%)
- No attribute progression system exists

**Total Attribute System:** 4/37 features complete (11%)

---

## Next Priorities

Based on combat system dependencies and player value:

1. **Combat Balance Overhaul (RFC-017)** - Super-linear scaling, commitment-ratio queue, reaction window gap, dismiss mechanic — required for meaningful multi-enemy balance
2. **Implement Axis/Spectrum/Shift Core** - Foundation for entire system
3. **Scrollbar UI Prototype** - Make system understandable to players
4. **Level 1-50 Investment** - Allow progression and build diversity
5. **Movement Speed (Grace)** - Immediate tactical impact
6. **Threat Generation (Presence)** - Required for PvE tanking
7. **Critical Hit System (Instinct)** - Adds build variety and excitement

---

**Document Version:** 1.0
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
