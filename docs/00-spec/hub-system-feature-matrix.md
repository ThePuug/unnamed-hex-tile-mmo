# Hub System - Feature Matrix

**Specification:** [hub-system.md](hub-system.md)
**Last Updated:** 2025-11-01
**Overall Status:** 0/38 features complete (0% - not yet implemented)

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Dynamic Settlement Growth

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Population-based hub tiers | ❌ Not Started | - | Line 7 | Frontier/Town/City progression |
| NPC dynamic migration | ❌ Not Started | - | Line 5 | NPCs appear based on hub size |
| Safe zone expansion | ❌ Not Started | - | Line 5 | Player presence pushes back hostiles |
| Frontier Camps (5-10 players) | ❌ Not Started | - | Line 15 | Smallest hub tier |
| Towns (20-50 players) | ❌ Not Started | - | Line 17 | Mid-tier hubs |
| Cities (100+ players) | ❌ Not Started | - | Line 19 | Large hubs |

**Category Status:** 0/6 complete (0%)

---

### Player Investment

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Homesteads (personal) | ❌ Not Started | - | Line 11 | Player housing |
| Markets (player trade) | ❌ Not Started | - | Line 11 | P2P economy |
| Factories (complex crafting) | ❌ Not Started | - | Line 11 | High-tier production |
| Personal storage | ❌ Not Started | - | Line 15 | Inventory management |

**Category Status:** 0/4 complete (0%)

---

### Influence System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Influence radius formula | ❌ Not Started | - | Lines 27-36 | population × 3.0 tiles |
| Influence falloff curve | ❌ Not Started | - | Lines 39-44 | Cubic distance factor |
| Maximum influence scaling | ❌ Not Started | - | Lines 47-55 | √(pop/1000) capped at 1.0 |
| Encroachment calculation | ❌ Not Started | - | Lines 59-69 | Sums hub influences |
| Multi-hub cooperation | ❌ Not Started | - | Line 67 | Additive influence |

**Category Status:** 0/5 complete (0%)

---

### Urban Core & Protection Zones

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Urban core radius | ❌ Not Started | - | Lines 75-86 | 10% of influence radius |
| Area budget calculation | ❌ Not Started | - | Lines 92-94 | π × (pop × 0.3)² |
| Intelligent boundary optimization | ❌ Not Started | - | Lines 88-102 | Non-circular, construct-aware |
| Urban Core protection (0-10%) | ❌ Not Started | - | Lines 106-110 | 100% wall integrity |
| Standard Urban protection (10-100%) | ❌ Not Started | - | Lines 112-116 | 100% wall integrity |
| Peripheral Zone (beyond influence) | ❌ Not Started | - | Lines 118-123 | Walls degrade to 30% over 90 days |

**Category Status:** 0/6 complete (0%)

---

### Center of Mass

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Dynamic center calculation | ❌ Not Started | - | Lines 127-142 | Σ(pos × mass) / Σ(mass) |
| Construct mass weighting | ❌ Not Started | - | Line 134 | Tiles occupied by construct |
| Protection zone radiation | ❌ Not Started | - | Line 141 | Zones from center |

**Category Status:** 0/3 complete (0%)

---

### Anger Generation

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Homestead anger | ❌ Not Started | - | Line 172 | Moderate anger |
| Factory anger | ❌ Not Started | - | Line 174 | Significantly more than homesteads |
| Economic activity anger | ❌ Not Started | - | Line 175 | Scaling anger from activity |
| Anger accumulation formula | ❌ Not Started | - | Lines 179-185 | Cross-hub anger propagation |

**Category Status:** 0/4 complete (0%)

---

### Hub Merging

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Automatic merge trigger | ❌ Not Started | - | Lines 224-235 | Urban cores overlap |
| Merge mechanics | ❌ Not Started | - | Lines 237-256 | Larger absorbs smaller |
| Neighborhood preservation | ❌ Not Started | - | Line 242 | Smaller becomes neighborhood |
| Unified center of mass | ❌ Not Started | - | Lines 245-250 | Single combined entity |
| Boundary optimization | ❌ Not Started | - | Lines 252-256 | Dumbbell/figure-8 shapes |
| Permanent merge state | ❌ Not Started | - | Lines 266-269 | Never splits |

**Category Status:** 0/6 complete (0%)

---

### Shrinking Hub Mechanics

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Population decrease effects | ❌ Not Started | - | Lines 275-285 | Radius/core/center changes |
| Smart contraction phases | ❌ Not Started | - | Lines 288-303 | 3-phase contraction |
| Natural attrition | ❌ Not Started | - | Lines 299-303 | Siege pressure feedback |
| Corridor maintenance logic | ❌ Not Started | - | Lines 315-329 | Merged hub corridors |

**Category Status:** 0/4 complete (0%)

---

## Implementation Deviations

None - system is entirely unimplemented.

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### Critical for MMO Gameplay
- **Entire Hub System:** Population-based hubs, influence, anger, sieges (All features)
- **Player Investment Structures:** Homesteads, markets, factories (Lines 9-12)
- **Hub Tier Progression:** Frontier → Town → City lifecycle (Lines 7, 147-164)

### Medium Priority
- **Hub Merging System:** Automatic merging when urban cores overlap (Lines 221-269)
- **Protection Zones:** Urban core vs peripheral risk levels (Lines 104-123)
- **Anger Propagation:** Cross-hub anger accumulation (Lines 169-185)

### Low Priority (Post-Hub MVP)
- **Shrinking/Contraction:** Smart boundary optimization as hubs lose population (Lines 273-307)
- **Merged Hub Corridors:** Intelligent boundary shaping for merged entities (Lines 313-358)

---

## Progress Summary

**Foundation:** 0/6 complete (0%)
- Dynamic settlement growth not implemented

**Core Systems:** 0/12 complete (0%)
- Influence, anger, protection zones not implemented

**Advanced Systems:** 0/10 complete (0%)
- Merging, shrinking, optimization not implemented

**Total Hub System:** 0/38 features complete (0%)

---

## Next Priorities

Hub system is a major post-MVP feature. Suggested implementation order:

1. **Basic Hub Entity** - Static hub locations with population tracking
2. **Influence Radius** - Calculate and visualize hub influence zones
3. **Encroachment Integration** - Link to siege system (encroachment reduction)
4. **Homesteads** - Player-placed structures that contribute to hub population
5. **Anger Generation** - Structures generate anger for siege scaling
6. **Hub Merging** - Automatic merge when urban cores overlap
7. **Full Protection Zones** - Urban core vs peripheral mechanics

---

**Document Version:** 1.0
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
