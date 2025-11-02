# Triumvirate - Feature Matrix

**Specification:** [triumvirate.md](triumvirate.md)
**Last Updated:** 2025-11-01
**Overall Status:** 2/46 features complete (4% - MVP Approach only)

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Origin System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Evolved origin | 🚧 Partial | Player only | Lines 9 | Players start as Evolved, no mechanics |
| Synthetic origin | ❌ Not Started | - | Line 10 | Future origin option |
| Essential origin | ❌ Not Started | - | Line 11 | Future origin option |
| Corrupted origin | ❌ Not Started | - | Line 12 | Future origin option |
| Mythic origin | ❌ Not Started | - | Line 13 | Future origin option |
| Forgotten origin | ❌ Not Started | - | Line 14 | Future origin option |
| Indiscernible origin | ❌ Not Started | - | Line 15 | Unique origin (unknowable) |
| Origin-based skill flavoring | ❌ Not Started | - | Lines 56-68 | Visual/thematic variations |

**Category Status:** 0/8 complete (0% - Evolved exists as placeholder only)

---

### Approach System (Offensive Identity)

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Direct approach | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | Lines 21-22 | MVP has Lunge only |
| Direct: Charge signature | ❌ Not Started | - | Line 22 | Post-MVP ability |
| Direct: Lunge signature | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) | Line 22 | MVP Q ability |
| Distant approach | ❌ Not Started | - | Lines 23-24 | Range-based combat |
| Distant: Volley signature | ❌ Not Started | - | Line 24 | Ranged attack |
| Distant: Mark signature | ❌ Not Started | - | Line 24 | Target marking |
| Ambushing approach | ❌ Not Started | - | Lines 25-26 | Surprise attacks |
| Ambushing: Ambush signature | ❌ Not Started | - | Line 26 | Stealth strike |
| Ambushing: Trap signature | ❌ Not Started | - | Line 26 | Ground trap |
| Patient approach | ❌ Not Started | - | Lines 27-28 | Defensive/counter |
| Patient: Taunt signature | ❌ Not Started | - | Line 28 | Threat generation |
| Patient: Counter signature | ❌ Not Started | - | Line 28 | Reflect damage |
| Binding approach | ❌ Not Started | - | Lines 29-30 | Control/immobilize |
| Binding: Root signature | ❌ Not Started | - | Line 30 | Immobilize target |
| Binding: Snare signature | ❌ Not Started | - | Line 30 | Slow/trap |
| Evasive approach | ❌ Not Started | - | Lines 31-32 | Mobility/disruption |
| Evasive: Flank signature | ❌ Not Started | - | Line 32 | Positioning attack |
| Evasive: Disorient signature | ❌ Not Started | - | Line 32 | Confusion/disrupt |
| Overwhelming approach | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | Lines 33-34 | MVP has Overpower only |
| Overwhelming: Overpower signature | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) | Line 34 | MVP W ability |
| Overwhelming: Petrify signature | ❌ Not Started | - | Line 34 | Stun/disable |

**Category Status:** 2/21 complete (10% - MVP scope only)

---

### Resilience System (Defensive Identity)

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Vital resilience | ❌ Not Started | - | Lines 40-41 | Physical endurance |
| Vital: Regenerate signature | ❌ Not Started | - | Line 41 | Health regen |
| Vital: Endure signature | ❌ Not Started | - | Line 41 | Stagger resist |
| Mental resilience | ❌ Not Started | - | Lines 42-43 | Mental fortitude |
| Mental: Focus signature | ❌ Not Started | - | Line 43 | Concentration |
| Mental: Dispel signature | ❌ Not Started | - | Line 43 | Remove debuffs |
| Hardened resilience | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | Lines 44-45 | MVP has Deflect (modified) |
| Hardened: Fortify signature | ❌ Not Started | - | Line 45 | Armor buff |
| Hardened: Deflect signature | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | Line 45 | MVP R ability (clears all, not first) |
| Shielded resilience | ❌ Not Started | - | Lines 46-47 | Magic wards |
| Shielded: Ward signature | ❌ Not Started | - | Line 47 | Magic shield |
| Shielded: Repel signature | ❌ Not Started | - | Line 47 | Knockback/push |
| Blessed resilience | ❌ Not Started | - | Lines 48-49 | Divine/conviction |
| Blessed: Heal signature | ❌ Not Started | - | Line 49 | Restore health |
| Blessed: Cleanse signature | ❌ Not Started | - | Line 49 | Remove status |
| Primal resilience | ❌ Not Started | - | Lines 50-51 | Elemental/raw |
| Primal: Enrage signature | ❌ Not Started | - | Line 51 | Damage buff |
| Primal: Attune signature | ❌ Not Started | - | Line 51 | Elemental resist |
| Eternal resilience | ❌ Not Started | - | Lines 52-53 | Time/immortality |
| Eternal: Revive signature | ❌ Not Started | - | Line 53 | Self-resurrect |
| Eternal: Defy signature | ❌ Not Started | - | Line 53 | Death prevent |

**Category Status:** 0/21 complete (0% - 1 partial MVP deviation)

---

### Triumvirate Combinations

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Origin flavor system | ❌ Not Started | - | Lines 56-68 | Thematic variations |
| Approach/Resilience pairing | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | - | MVP uses mixed abilities, not true pairs |
| 4 signature skills per class | 🚧 Partial | [ADR-009](../adr/009-mvp-ability-set.md) | Line 56 | MVP has 4 abilities (not true signatures) |

**Category Status:** 0/3 complete (0%)

---

## Implementation Deviations

### 1. MVP Ability Set Not Triumvirate-Based
- **Spec Says:** Players choose Approach + Resilience, get 2 signature skills from each (Lines 56)
- **Actually Implemented:** MVP has 4 fixed abilities (Lunge, Overpower, Knockback, Deflect) that don't follow Triumvirate system
- **Rationale:** Simplified MVP to validate combat mechanics before adding build diversity
- **ADR Reference:** [ADR-009](../adr/009-mvp-ability-set.md)

### 2. Deflect Scope Change
- **Spec Says:** Deflect clears first queued threat (Hardened signature, Line 45)
- **Actually Implemented:** Deflect clears all queued threats (50 stamina cost)
- **Rationale:** MVP defensive option, expensive cost forces tactical usage
- **ADR Reference:** [ADR-009](../adr/009-mvp-ability-set.md)

### 3. Origin System Not Implemented
- **Spec Says:** Origin provides thematic flavor for skill visuals (Lines 56-68)
- **Actually Implemented:** Players are "Evolved" by default, no mechanical impact
- **Rationale:** Visual flavor deferred until core Triumvirate system implemented
- **Status:** Future expansion

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### Critical for Build Diversity
- **Full Approach Set:** 7 approaches with 14 signature abilities (only 2/14 exist)
- **Full Resilience Set:** 7 resiliences with 14 signature abilities (only 0.5/14 exist - Deflect modified)
- **Triumvirate Selection System:** Choose 1 Approach + 1 Resilience at character creation

### Medium Priority
- **Approach Opposites:** Direct ↔ Distant, Ambushing ↔ Patient, Binding ↔ Evasive (Lines 21-34)
- **Resilience Opposites:** Vital ↔ Mental, Hardened ↔ Shielded, Blessed ↔ Primal (Lines 40-53)
- **Attribute Integration:** Link to attribute-system.md leanings (see attribute spec)

### Low Priority (Post-Launch)
- **Additional Origins:** Synthetic, Essential, Corrupted, Mythic, Forgotten, Indiscernible (Lines 9-15)
- **Origin Skill Flavoring:** Visual/audio variations per origin (Lines 56-68)

---

## Progress Summary

**Origin System:** 0/8 complete (0%)
- Evolved exists as placeholder only

**Approach System:** 2/21 complete (10%)
- Direct: Lunge ✅
- Overwhelming: Overpower ✅
- All other approaches: ❌ Not Started

**Resilience System:** 0/21 complete (0%)
- Deflect exists but heavily modified from spec

**Triumvirate Framework:** 0/3 complete (0%)
- No true Approach/Resilience pairing system

**Total Triumvirate System:** 2/53 features complete (4%)

---

## Next Priorities

Based on build diversity value and spec completeness:

1. **Triumvirate Selection System** - Allow players to choose Approach + Resilience
2. **Complete Direct Approach** - Add Charge to join Lunge
3. **Implement One Full Resilience** - Add Vital (Regenerate + Endure) or Mental (Focus + Dispel)
4. **Add Distant Approach** - Ranged playstyle (Volley + Mark)
5. **Patient Approach** - Counter-based defense (Taunt + Counter)
6. **Expand Enemy Triumvirates** - Wild Dog should have Triumvirate classification

---

**Document Version:** 1.0
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
