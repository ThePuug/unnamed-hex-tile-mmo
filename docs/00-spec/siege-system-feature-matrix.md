# Siege System - Feature Matrix

**Specification:** [siege-system.md](siege-system.md)
**Last Updated:** 2025-11-01
**Overall Status:** 0/15 features complete (0% - not yet implemented)

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Core Mechanics

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Encroachment (enemy strength) | ❌ Not Started | - | Lines 7-11 | Distance into hostile territory |
| Anger (wave size) | ❌ Not Started | - | Lines 13-17 | Hub development activity |
| Two-force opposition system | ❌ Not Started | - | Lines 5-6 | Encroachment vs Anger |

**Category Status:** 0/3 complete (0%)

---

### Hub Evolution Lifecycle

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Frontier Camp sieges | ❌ Not Started | - | Lines 23-27 | High encroachment, low anger |
| Growing Town sieges | ❌ Not Started | - | Lines 29-32 | Medium both |
| Established City sieges | ❌ Not Started | - | Lines 34-38 | Low encroachment, high anger |

**Category Status:** 0/3 complete (0%)

---

### Influence System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Influence radius formula | ❌ Not Started | - | Lines 43-51 | population × 3.0 tiles |
| Influence falloff curve | ❌ Not Started | - | Lines 53-58 | Cubic distance factor |
| Maximum influence scaling | ❌ Not Started | - | Lines 60-69 | √(pop/1000) capped at 1.0 |

**Category Status:** 0/3 complete (0%)

---

### Encroachment & Anger Calculation

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Total influence summation | ❌ Not Started | - | Lines 74-84 | Sum all hub influences |
| Encroachment from influence | ❌ Not Started | - | Lines 77-78 | MAX × (1.0 - influence) |
| Anger accumulation | ❌ Not Started | - | Lines 88-100 | Cross-hub anger propagation |

**Category Status:** 0/3 complete (0%)

---

### Strategic Patterns

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Urbanization incentives | ❌ Not Started | - | Lines 104-117 | Merge vs separate hubs |
| Deadly proximity zones | ❌ Not Started | - | Lines 119-125 | Small hub near large city |
| Safe placement patterns | ❌ Not Started | - | Lines 127-131 | Frontier/regional/capital spacing |

**Category Status:** 0/3 complete (0%)

---

## Implementation Deviations

None - system is entirely unimplemented.

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### Critical for Hub System
- **Encroachment System:** Distance-based enemy strength scaling (Lines 7-11)
- **Anger System:** Development-based wave size scaling (Lines 13-17)
- **Influence Calculations:** Hub influence radius, falloff, and summation (Lines 41-84)

### Medium Priority
- **Hub Evolution Patterns:** Frontier → Town → City siege progression (Lines 21-38)
- **Cross-Hub Anger:** Anger accumulation from nearby hubs (Lines 86-100)
- **Strategic Placement:** Urbanization vs frontier patterns (Lines 102-131)

### Low Priority (Post-Siege MVP)
- **Hub Merging Vote System:** Player voting for merges (Lines 145-148)
- **Merge Transition Period:** Reduced defenses during merge (Line 149)

---

## Progress Summary

**Core Mechanics:** 0/3 complete (0%)
- Encroachment and anger systems not implemented

**Evolution Lifecycle:** 0/3 complete (0%)
- No siege difficulty progression exists

**Influence System:** 0/3 complete (0%)
- Hub influence not calculated

**Strategic Systems:** 0/6 complete (0%)
- No placement incentives or calculations

**Total Siege System:** 0/15 features complete (0%)

---

## Dependencies

Siege system depends on:
- **Hub System** ([hub-system.md](hub-system.md)) - Population, structures, placement
- **Combat System** ([combat-system.md](combat-system.md)) - Enemy AI, waves, telegraphs
- **Enemy Types** - Multiple enemy tiers for encroachment scaling

---

## Next Priorities

Siege system is a major post-MVP feature. Suggested implementation order:

1. **Encroachment Formula** - Calculate encroachment at any hex position
2. **Anger Formula** - Calculate anger based on hub structures
3. **Influence System** - Hub influence radius and falloff
4. **Basic Siege Waves** - Spawn enemies based on encroachment + anger
5. **Enemy Strength Scaling** - Encroachment affects individual enemy stats
6. **Wave Size Scaling** - Anger affects number of enemies in wave
7. **Hub Lifecycle** - Frontier → Town → City siege evolution

---

**Document Version:** 1.0
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
