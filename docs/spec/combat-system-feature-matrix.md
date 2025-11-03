# Combat System - Feature Matrix

**Specification:** [combat-system.md](combat-system.md)
**Last Updated:** 2025-11-03
**Overall Status:** 18/44 features complete (41% - MVP scope)

---

## Status Legend

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

---

## Feature Breakdown

### Movement and Heading

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Arrow key movement | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 22-31 | 4-key hex movement implemented |
| Heading tracking | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 33-41 | Persists after movement stops |
| Character rotation | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 43-45 | Visual facing indicator |
| Position on hex | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 44 | Micro-positioning based on facing |
| Facing cone (60°) | ⏸️ Deferred | - | Lines 35, 45 | Optional visual overlay, not MVP |

**Category Status:** 4/5 complete (80%)

---

### Targeting System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Directional targeting | ✅ Complete | [ADR-004](../adr/004-ability-system-and-targeting.md) | Lines 48-62 | Face + proximity based |
| Hostile indicator (red) | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Lines 66-69 | World-space hex indicator |
| Ally indicator (green) | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Lines 66-69 | Ready for PvP/allies |
| Range tier system | 🚧 Partial | [ADR-004](../adr/004-ability-system-and-targeting.md) | Lines 71-76 | Close/Mid/Far defined |
| Automatic targeting | ✅ Complete | [ADR-004](../adr/004-ability-system-and-targeting.md) | Lines 80-83 | Nearest in facing direction |
| Tier lock (1/2/3 keys) | 🚧 Partial | [ADR-004](../adr/004-ability-system-and-targeting.md) | Lines 85-106 | Defined in spec, implementation TBD |
| TAB cycling | ❌ Not Started | - | Lines 108-115 | Manual target selection |
| ESC clear targeting | ❌ Not Started | - | Line 113 | Return to auto-target |
| Tier badge visual | ❌ Not Started | - | Lines 127 | UI feedback for tier lock |
| Facing cone overlay | ⏸️ Deferred | - | Line 131 | Optional visual aid |

**Category Status:** 4/10 complete (40%)

---

### Attack Execution Patterns

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Instant attacks | 🚧 Partial | Implemented | Lines 145-149 | BasicAttack only (Lunge/Overpower not yet) |
| Projectile attacks | ❌ Not Started | - | Lines 151-161 | Travel time, dodgeable |
| Ground effects/telegraphs | ❌ Not Started | - | Lines 163-173 | AOE warnings, delayed damage |
| Unavoidable attacks | ⏸️ Deferred | - | Lines 175-179 | Ultimate-tier abilities |

**Category Status:** 0/4 complete (0%)

---

### Reaction Queue System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Threat queue component | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Lines 188-194 | Per-entity queue |
| Independent timers | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Line 190 | Circular progress |
| Queue capacity (Focus) | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Lines 222-230 | Scales with Focus attribute |
| Timer duration (Instinct) | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Lines 212-220 | Reaction window scaling |
| Queue overflow resolution | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Lines 239-243 | Oldest threat resolves |
| Queue display UI | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Lines 196-209 | Circular icons with timers |
| Reaction ability clear | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Lines 244-248 | Deflect clears queue |

**Category Status:** 7/7 complete (100%)

---

### Reaction Abilities

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Dodge (queue clear) | ⏸️ Deferred | Implemented (replaced) | Lines 260-264 | Replaced by Deflect |
| Deflect (Hardened) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [deflect.rs](../../src/server/systems/combat/abilities/deflect.rs) | Lines 289-294 | 50 stamina, full queue clear |
| Ward (Shielded) | ⏸️ Deferred | - | Lines 266-270 | Post-MVP |
| Fortify (Hardened) | ⏸️ Deferred | - | Lines 272-276 | Post-MVP |
| Counter (Patient) | ⏸️ Deferred | - | Lines 282-287 | Post-MVP |
| Parry (Primal) | ⏸️ Deferred | - | Lines 296-302 | Post-MVP |
| Endure (Vital) | ⏸️ Deferred | - | Lines 308-313 | Post-MVP |
| Dispel (Mental) | ⏸️ Deferred | - | Lines 315-320 | Post-MVP |
| Global Cooldown (0.5s) | ✅ Complete | [ADR-003](../adr/003-reaction-queue-system.md) | Lines 323-328 | Prevents spam |

**Category Status:** 2/9 complete (22% - MVP scope intentional)

---

### MVP Abilities (Phase 1)

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| BasicAttack (Q) | ⏸️ Deferred | Implemented (replaced) | Lines 538-548 | Replaced by Auto-Attack + Lunge |
| Dodge (E) | ⏸️ Deferred | Implemented (replaced) | Lines 260-264 | Replaced by Knockback + Deflect |
| Auto-Attack (passive) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [auto_attack.rs](../../src/server/systems/combat/abilities/auto_attack.rs) | Lines 73-92 | 20 dmg, adjacent, manual trigger |
| Lunge (Q) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [lunge.rs](../../src/server/systems/combat/abilities/lunge.rs) | Lines 93-110 | 40 dmg, 20 stam, 4 hex, gap closer |
| Overpower (W) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [overpower.rs](../../src/server/systems/combat/abilities/overpower.rs) | Lines 111-128 | 80 dmg, 40 stam, 1 hex |
| Knockback (E) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [knockback.rs](../../src/server/systems/combat/abilities/knockback.rs) | Lines 130-149 | 30 stam, reactive counter, push 1 hex |
| Deflect (R) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [deflect.rs](../../src/server/systems/combat/abilities/deflect.rs) | Lines 150-165 | 50 stam, clears all threats |

**Category Status:** 5/7 complete (71% - MVP abilities fully implemented, legacy abilities deferred)

---

### Resources

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Stamina pool | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 334-350 | Scales with Might/Vitality |
| Stamina regeneration | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Line 340 | 10/sec base rate |
| Mana pool | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 352-369 | Scales with Focus/Presence |
| Mana regeneration | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Line 359 | 8/sec base rate |
| Resource bars UI | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 1 | Stamina/Health/Mana display |

**Category Status:** 5/5 complete (100%)

---

### Damage Calculation

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Physical damage formula | ✅ Complete | [ADR-005](../adr/005-damage-pipeline.md) | Lines 377-380 | Might scaling |
| Magic damage formula | 🚧 Partial | [ADR-005](../adr/005-damage-pipeline.md) | Lines 382-385 | Formula exists, no magic abilities yet |
| Critical hits | ⏸️ Deferred | - | Lines 387-393 | Instinct-based crits |
| Armor (physical reduction) | ✅ Complete | [ADR-005](../adr/005-damage-pipeline.md) | Lines 399-407 | Vitality scaling |
| Resistance (magic reduction) | 🚧 Partial | [ADR-005](../adr/005-damage-pipeline.md) | Lines 409-417 | Formula exists, no magic damage yet |
| Stagger resist | ⏸️ Deferred | - | Lines 419-424 | Cast interruption system |

**Category Status:** 3/6 complete (50%)

---

### Combat State

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Enter combat triggers | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 432-436 | Damage/aggro/ability use |
| Combat state effects | 🚧 Partial | [ADR-002](../adr/002-combat-foundation.md) | Lines 440-445 | UI shows, other effects TBD |
| Leave combat conditions | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Lines 449-452 | Distance/time based |
| Combat music | ❌ Not Started | - | Line 443 | Audio system |

**Category Status:** 2/4 complete (50%)

---

### Enemy AI

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Enemy directional targeting | ✅ Complete | [ADR-006](../adr/006-ai-behavior-and-ability-integration.md) | Lines 458-464 | Face + geometric target |
| Wild Dog (melee enemy) | ✅ Complete | [ADR-006](../adr/006-ai-behavior-and-ability-integration.md) | Lines 468-480 | Aggro, pursuit, attack cycle |
| Ranged enemy (Forest Sprite) | ❌ Not Started | - | Lines 482-490 | Kiting behavior |
| Visual telegraphs | ❌ Not Started | - | Lines 494-504 | Enemy attack warnings |

**Category Status:** 2/4 complete (50%)

---

### Combat HUD

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Resource bars | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 1 | Stamina/Health/Mana |
| Action bar (Q/W/E/R) | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 2 | 4 ability slots with states |
| Threat icons display | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 3 | Circular timers, attack icons |
| Target indicators | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 4 | Red hostile, green ally |
| Target detail frame | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 5 | Name, health, triumvirate |
| World health bars | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Phase 6 | Above current targets |
| Combat state visuals | ⏸️ Deferred | - | Phase 7 | Vignette, glows (intentionally skipped) |

**Category Status:** 6/7 complete (86%)

---

### Special Mechanics

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Mutual destruction | ✅ Complete | [ADR-005](../adr/005-damage-pipeline.md) | Lines 508-526 | Both combatants can die |

**Category Status:** 1/1 complete (100%)

---

## Implementation Deviations

Features where implementation intentionally differs from spec:

### 1. Reaction Queue Position
- **Spec Says:** Top-center, 50px from top edge (Line 82 in ADR-008)
- **Actually Implemented:** Center-screen with VERTICAL_OFFSET = -150px (above center)
- **Rationale:** Better visibility during combat, closer to player focus area
- **ADR Reference:** [ADR-008 Acceptance](../adr/008-acceptance.md) Lines 81-88

### 2. Deflect Ability Scope
- **Spec Says:** Clears first queued threat only (Hardened signature, Lines 289-294)
- **Actually Implemented:** Clears all queued threats (50 stamina cost)
- **Rationale:** Simplified MVP defensive option, expensive cost forces tactical usage
- **ADR Reference:** [ADR-009](../adr/009-mvp-ability-set.md)

### 3. World Health Bars Implementation
- **Spec Says:** Health bars on all entities in combat (spawn/despawn per entity)
- **Actually Implemented:** 2 persistent bars (hostile, ally) repositioned to current targets
- **Rationale:** More performant, clearer visual feedback, less clutter
- **ADR Reference:** [ADR-008 Acceptance](../adr/008-acceptance.md) Lines 130-150

### 4. Action Bar Range Feedback
- **Spec Says:** Target out of ability range: Indicator dims or shows range error on cast attempt (Line 130)
- **Actually Implemented:** Action bar shows visual range validation feedback (red border when target out of range), plus basic attack restricted to adjacent tiles only
- **Rationale:** Proactive UX feedback prevents failed ability attempts, clearer tactical feedback
- **Implementation Commit:** `c9db09a` (2025-11-01)

### 5. Knockback as Reactive Counter
- **Spec Says (ADR-009):** Knockback creates space without clearing threats (positioning tool)
- **Actually Implemented:** Knockback targets source of most recent threat in reaction queue, removes threat, pushes attacker away
- **Rationale:** Creates deeper skill expression and queue interaction. Requires usage within threat window (1-1.75s based on Instinct). More tactical than simple positioning.
- **ADR Reference:** [ADR-009 Acceptance](../adr/009-acceptance.md)

### 6. Overpower Cooldown
- **Spec Says (ADR-009, Line 123):** 2 second cooldown prevents spam
- **Actually Implemented:** 1s Attack GCD only (standard for all offensive abilities)
- **Rationale:** GCD prevents spam effectively for MVP. Separate cooldown system adds complexity without significant tactical benefit.
- **ADR Reference:** [ADR-009 Acceptance](../adr/009-acceptance.md)

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### High Priority
- **TAB Cycling:** Manual target selection within tier (Lines 108-115)
- **Tier Lock Number Keys:** 1/2/3 for range tier selection (Lines 85-106)
- **Projectile Attacks:** Travel time, dodgeable mechanics (Lines 151-161)

### Medium Priority
- **Ranged Enemy Type:** Forest Sprite with kiting behavior (Lines 482-490)
- **Critical Hit System:** Instinct-based crits (Lines 387-393)
- **Visual Telegraphs:** Enemy attack warnings (Lines 494-504)

### Low Priority (Post-MVP)
- **Full Reaction Ability Set:** 7 additional reaction abilities (Lines 260-320)
- **Ground Effects:** AOE telegraphs with delayed damage (Lines 163-173)
- **Unavoidable Attacks:** Ultimate-tier mechanics (Lines 175-179)
- **Combat Music:** Audio system integration (Line 443)
- **Stagger System:** Cast interruption mechanics (Lines 419-424)

---

## Progress Summary

**MVP Scope (Phase 1):** 18/31 features complete (58%)
- Core systems: Movement (4/4), Queue (7/7), Resources (5/5), HUD (6/6) ✅ Complete
- MVP abilities: 5/7 complete (Auto-Attack, Lunge, Overpower, Knockback, Deflect) ✅
- Partial systems: Targeting (4/9), Damage (3/4), Combat State (2/3), Enemy AI (2/3)
- Missing: Attack execution patterns (0/3), TAB cycling, tier lock keys

**Post-MVP (Phases 2-4):** 0/13 features started (0%)
- Deferred: 7 reaction abilities, crits, stagger, facing cone visuals, combat state visuals, ranged enemies, telegraphs, unavoidable attacks

**Total Combat System:** 18/44 features complete (41%)

---

## Next Priorities

Based on actual implementation status and user value:

1. ✅ ~~**Accept or Reject ADR-009**~~ - ACCEPTED (2025-11-03)
2. ✅ ~~**Implement Accepted MVP Abilities**~~ - All 5 abilities complete (Auto-Attack, Lunge, Overpower, Knockback, Deflect)
3. **Playtest MVP Combat Loop** - Validate resource economy, ability feel, Deflect cost balance
4. **Tier Lock Number Keys (1/2/3)** - Critical for range targeting UX
5. **TAB Cycling** - Required for equidistant target selection
6. **Second Enemy Type** - Validates targeting system, increases variety
7. **Projectile Attack Pattern** - Foundation for ranged combat

---

**Document Version:** 1.0
**Maintained By:** Development team
**Review Cadence:** Update after each ADR acceptance or spec change
