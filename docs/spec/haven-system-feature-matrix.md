# Haven System - Feature Matrix

**Specification:** [haven-system.md](haven-system.md)
**Last Updated:** 2025-11-01
**Overall Status:** 0/18 features complete (0% - not yet implemented)

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
| Bootstrap problem solution | ❌ Not Started | - | Lines 7-8 | Safe zones at launch |
| Mountain Stronghold | ❌ Not Started | - | Line 12 | Cold/mountain biome haven |
| Prairie Fortress | ❌ Not Started | - | Line 13 | Grassland/plains biome haven |
| Forest Village | ❌ Not Started | - | Line 14 | Forest/woodland biome haven |
| 1000 city-level influence | ❌ Not Started | - | Lines 19-24 | Full encroachment suppression |
| 50 town-level anger | ❌ Not Started | - | Line 23 | Minimal siege pressure |
| 30 tile urban core | ❌ Not Started | - | Line 24 | Close building allowed |
| Indestructible property | ❌ Not Started | - | Line 27 | Cannot be destroyed |
| Permanent respawn point | ❌ Not Started | - | Line 28 | New/defeated player spawn |
| Basic services | ❌ Not Started | - | Line 29 | Starter vendors, crafting |

**Category Status:** 0/10 complete (0%)

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
| Zone definitions | ❌ Not Started | - | Lines 96-104 | 0-300 core to 3000+ wilderness |

**Category Status:** 0/1 complete (0%)

---

## Implementation Deviations

None - system is entirely unimplemented.

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

**Starter Havens:** 0/10 complete (0%)
- No havens exist in game

**Haven Mechanics:** 0/4 complete (0%)
- Placement, sieges, zones not implemented

**Total Haven System:** 0/18 features complete (0%)

---

## Dependencies

Haven system depends on:
- **Hub System** ([hub-system.md](hub-system.md)) - Influence, population, urban core
- **Siege System** ([siege-system.md](siege-system.md)) - Encroachment, anger, waves
- **Spawn System** - Player respawn mechanics
- **World Generation** - Biome placement for Mountain/Prairie/Forest locations

---

## Next Priorities

Haven system is critical for launch (bootstrap problem). Suggested implementation order:

1. **Three Fixed Haven Locations** - Mountain, Prairie, Forest at specified coords
2. **Haven Influence** - 1000 pop equivalent, 3000 tile radius, 1.0 max influence
3. **Haven Respawn** - Players spawn at nearest haven on death/join
4. **Basic Services** - Starter vendor NPCs at each haven
5. **Indestructible Property** - Havens immune to siege destruction
6. **Tutorial Sieges** - Small weak waves for new player experience
7. **Territory Zone Visualization** - Distance-based difficulty indicators

---

**Document Version:** 1.0
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
