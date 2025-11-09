# Haven System - Feature Matrix

**Specification:** [haven-system.md](haven-system.md)
**Last Updated:** 2025-11-08
**Overall Status:** 3/18 features complete (17% - MVP subset via ADR-014)
**ADR-014 Implementation:** Simplified spatial difficulty system as MVP foundation

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Starter Havens

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Bootstrap problem solution | 🚧 Partial | ADR-014 | Lines 7-8 | Single haven at origin (not 3 havens) |
| Mountain Stronghold | ❌ Not Started | - | Line 12 | Deferred to post-MVP |
| Prairie Fortress | ❌ Not Started | - | Line 13 | Deferred to post-MVP |
| Forest Village | ❌ Not Started | - | Line 14 | Deferred to post-MVP |
| 1000 city-level influence | ❌ Not Started | - | Lines 19-24 | Deferred (using distance instead) |
| 50 town-level anger | ❌ Not Started | - | Line 23 | Deferred to post-MVP |
| 30 tile urban core | ❌ Not Started | - | Line 24 | Deferred to post-MVP |
| Indestructible property | ❌ Not Started | - | Line 27 | No havens to destroy yet |
| Permanent respawn point | ✅ Complete | ADR-014 | Line 28 | Players spawn at HAVEN_LOCATION (0,0) |
| Basic services | ❌ Not Started | - | Line 29 | Deferred to post-MVP |

**Category Status:** 1/10 complete (10%) - ADR-014 MVP foundation

---

### Haven Placement

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| 7000+ tile spacing | ❌ Not Started | - | Lines 35-40 | Avoid influence overlap |
| Mountain: (-5000, -5000) | ❌ Not Started | - | Line 38 | Specific coordinates |
| Prairie: (5000, -5000) | ❌ Not Started | - | Line 39 | Specific coordinates |
| Forest: (0, 5000) | ❌ Not Started | - | Line 40 | Specific coordinates |

**Category Status:** 0/4 complete (0%)

---

### Haven Siege Behavior

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Tutorial sieges | ❌ Not Started | - | Lines 44-55 | Small waves, weak enemies |
| Always survivable | ❌ Not Started | - | Line 53 | Even unmanned |
| Cannot be destroyed | ❌ Not Started | - | Line 54 | Regardless of outcome |

**Category Status:** 0/3 complete (0%)

---

### Territory Zones

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Zone definitions | 🚧 Partial | ADR-014 | Lines 96-104 | Distance-based difficulty (0-10 levels per 100 tiles) + directional zones |

**Category Status:** 0.5/1 complete (50%) - Simplified MVP implementation

---

## Implementation Deviations

**ADR-014 MVP Subset (Intentional Simplification for Combat Prototyping):**

### Implemented
- ✅ **Single Haven** instead of three (Mountain/Prairie/Forest) - Origin (0,0) as spawn point
- ✅ **Distance-based difficulty** instead of influence radius - 100 tiles per level (0-10)
- ✅ **Directional zones** instead of biome placement - N/E/S/W archetype zones
- ✅ **Permanent respawn** - Players spawn at haven location
- ✅ **UI feedback** - Distance indicator shows haven distance, zone, enemy level

### Deferred to Full Haven System
- ❌ Three distinct havens (Mountain/Prairie/Forest at 7000+ tile spacing)
- ❌ Influence radius system (1000 pop, 3000 tile radius)
- ❌ Encroachment formula (anger-based siege pressure)
- ❌ Haven services (vendors, crafting)
- ❌ Indestructible property mechanics
- ❌ Tutorial sieges

**Rationale:** Combat systems (ADR-002 through ADR-012) needed spatial variety for playtesting. ADR-014 provides foundation while deferring hub/siege/influence dependencies (~40 hours saved).

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### Critical for Launch
- **Three Starter Havens:** Mountain, Prairie, Forest with specific properties (Lines 11-31)
- **Haven Placement:** Fixed coordinates 7000+ tiles apart (Lines 35-40)
- **Indestructible Property:** Havens cannot be destroyed (Line 27)
- **Permanent Respawn:** New/defeated players spawn at havens (Line 28)

### Medium Priority
- **Haven Siege Behavior:** Tutorial-level sieges that never destroy haven (Lines 44-55)
- **Territory Zones:** Distance-based difficulty progression (Lines 96-104)
- **Basic Services:** Starter vendors, crafting stations (Line 29)

### Low Priority (Post-Launch)
- **Progression Timeline:** Expected player expansion patterns (Lines 106-138)
- **Endgame Function:** Long-term role as fallback zones (Lines 141-149)

---

## Progress Summary

**Starter Havens:** 1/10 complete (10%)
- ✅ Permanent respawn point (single haven at origin)
- 🚧 Bootstrap solution (simplified - one haven vs. three)
- ❌ Mountain/Prairie/Forest havens deferred
- ❌ Influence, anger, services deferred

**Haven Mechanics:** 0.5/4 complete (13%)
- 🚧 Territory zones (distance-based MVP instead of influence)
- ❌ Haven placement (three distinct locations) deferred
- ❌ Siege behavior deferred

**Total Haven System:** 3/18 features complete (17%)
- **ADR-014 MVP:** Single haven + distance-based difficulty + directional zones
- **Full Haven System:** Deferred pending hub/siege/influence systems

---

## Dependencies

Haven system depends on:
- **Hub System** ([hub-system.md](hub-system.md)) - Influence, population, urban core
- **Siege System** ([siege-system.md](siege-system.md)) - Encroachment, anger, waves
- **Spawn System** - Player respawn mechanics
- **World Generation** - Biome placement for Mountain/Prairie/Forest locations

---

## Next Priorities

**MVP Foundation Complete (ADR-014):**
- ✅ Single haven respawn point
- ✅ Distance-based difficulty
- ✅ Directional archetype zones
- ✅ UI feedback (distance indicator, level display)

**Full Haven System (Post-MVP):**

Haven system full implementation requires hub/siege/influence systems. Suggested order:

1. **Hub System Foundation** - Influence radius, population mechanics
2. **Siege System Foundation** - Encroachment formula, anger calculation
3. **Three Fixed Haven Locations** - Mountain (-5000,-5000), Prairie (5000,-5000), Forest (0,5000)
4. **Haven Influence** - 1000 pop equivalent, 3000 tile radius, 1.0 max influence
5. **Multi-Haven Respawn** - Players spawn at nearest haven on death/join
6. **Basic Services** - Starter vendor NPCs at each haven
7. **Indestructible Property** - Havens immune to siege destruction
8. **Tutorial Sieges** - Small weak waves for new player experience

**Current Priority:** Continue combat/attribute system prototyping with ADR-014 foundation

---

**Document Version:** 2.0 (Updated for ADR-014 acceptance)
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
**Last Major Update:** ADR-014 spatial difficulty MVP subset (2025-11-08)
