# Combat System - Feature Matrix

**Specification:** [combat-system.md](combat-system.md)
**Last Updated:** 2025-11-07
**Overall Status:** 61/114 features complete (54%)

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
| Movement speed (Grace scaling) | ✅ Complete | [ADR-010](../adr/010-combat-variety-phase-1.md), [ADR-010 Acceptance](../adr/010-acceptance.md) | attribute-system.md Lines 338-347 | Formula: max(75, 100 + grace/2), Grace -100=75%, 0=100%, 100=150% |
| Facing cone (60°) | ⏸️ Deferred | - | Lines 35, 45 | Optional visual overlay, not MVP |

**Category Status:** 5/6 complete (83%)

---

### Targeting System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Directional targeting | ✅ Complete | [ADR-004](../adr/004-ability-system-and-targeting.md) | Lines 48-62 | Face + proximity based |
| Hostile indicator (red) | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Lines 66-69 | World-space hex indicator |
| Ally indicator (green) | ✅ Complete | [ADR-008](../adr/008-combat-hud.md) | Lines 66-69 | Ready for PvP/allies |
| Range tier system | ✅ Complete | [ADR-004](../adr/004-ability-system-and-targeting.md), [ADR-010](../adr/010-combat-variety-phase-1.md) | Lines 71-76 | Close/Mid/Far tiers defined and functional |
| Automatic targeting | ✅ Complete | [ADR-004](../adr/004-ability-system-and-targeting.md) | Lines 80-83 | Nearest in facing direction |
| Tier lock (1/2/3 keys) | ✅ Complete | [ADR-010](../adr/010-combat-variety-phase-1.md), [ADR-010 Acceptance](../adr/010-acceptance.md) | Lines 85-106 | 1/2/3 keys lock to Close/Mid/Far, resets on ability use. **Unified design: affects BOTH hostile and ally targets** (tutorial required before support abilities). Includes visual ring indicator showing targeting area. |
| TAB cycling | ❌ Not Started | - | Lines 108-115 | Manual target selection |
| ESC clear targeting | ❌ Not Started | - | Line 113 | Return to auto-target |
| Tier badge visual | ⏸️ Deferred | [ADR-010 Acceptance](../adr/010-acceptance.md) Lines 323-328 | Lines 127 | Requires Bevy 0.16 3D text setup, core functionality complete |
| Facing cone overlay | ⏸️ Deferred | [ADR-010 Acceptance](../adr/010-acceptance.md) Lines 330-334 | Line 131 | Optional visual aid, bundled with tier badge UI |

**Category Status:** 6/10 complete (60%)

---

### Attack Execution Patterns

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Instant attacks | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) | Lines 141-145 | Auto-Attack, Lunge, Overpower all instant |
| Ranged attacks (instant hit) | ✅ Complete | [ADR-011](../adr/011-movement-intent-system.md) | Lines 147-158 | Instant damage to reaction queue, no travel time, not dodgeable by movement (Volley ability) |
| Attack telegraphs | ✅ Complete | [ADR-011](../adr/011-movement-intent-system.md) | Lines 177-208 | Yellow ball → hit line visual feedback system for ranged attacks |
| Ground effects/telegraphs | ❌ Not Started | - | Lines 160-169 | AOE warnings, delayed damage (planned) |
| Unavoidable attacks | ⏸️ Deferred | - | Lines 170-175 | Ultimate-tier abilities |

**Category Status:** 3/5 complete (60%)

---

### Network & Prediction

**Core Philosophy:** "Conscious but Decisive" requires responsive combat even at network scale. Client-side prediction eliminates perceived latency for tactical decision-making.

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Local player prediction | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md), [GUIDANCE.md](../../GUIDANCE.md) | Lines 7-17 (philosophy) | Input queue, offset.step prediction, server confirmation |
| Movement intent broadcasting | ✅ Complete | [ADR-011](../adr/011-movement-intent-system.md) Phase 1 | Lines 147-158 (ranged) | Server broadcasts destination when movement starts ([actor.rs:533+](../../src/server/systems/actor.rs)) |
| Relevance filtering | ✅ Complete | [ADR-011](../adr/011-movement-intent-system.md) Phase 2 | - | 30 hex radius spatial filtering via NNTree, per-client messaging ([renet.rs:398-422](../../src/server/systems/renet.rs)) |
| Remote entity prediction | ✅ Complete | [ADR-011](../adr/011-movement-intent-system.md) Phase 1, [ADR-011 Acceptance](../adr/011-acceptance.md) | Lines 7-17 (responsive) | Client predicts NPC/player movement using intent ([actor.rs:185-248](../../src/client/systems/actor.rs)) |
| Intent validation | ✅ Complete | [ADR-011](../adr/011-movement-intent-system.md) Phase 4, [ADR-011 Acceptance](../adr/011-acceptance.md) | - | Self-correcting interpolation via Unreliable channel, heading tracking prevents redundant broadcasts |

**Category Status:** 5/5 complete (100% - ADR-011 Phases 1, 2, 4 complete; Phase 3 obsolete; Phase 5 deferred)

**Impact:** Solves "ghost targeting" and "teleporting NPC" problems. Reduces perceived lag from 175ms to 50ms. Enables smooth remote entity movement.

**Note:** ADR-011 Phase 3 (Projectile Integration) obsolete due to combat system pivot to instant hit mechanics (see "Attack Execution Patterns" and "Implementation Deviations" sections).

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

**Category Status:** 1/8 complete (13% - MVP scope intentional)

---

### Ability Recovery System

**Core Mechanic (Lines 352-381):** Universal lockout system where using an ability locks ALL abilities for that ability's recovery duration. Replaces GCD with per-ability lockout periods creating tactical commitment.

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Universal lockout component (GlobalRecovery) | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 352-370 | Single component tracks lockout: remaining, duration, triggered_by |
| Per-ability lockout durations | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 364-370 | MVP values: Lunge 1s, Overpower 2s, Knockback 0.5s, Deflect 1s |
| Global recovery system (tick down) | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 352-381 | Ticks down lockout timer, removes component when expired |
| All abilities locked during recovery | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 352-363 | Universal lockout prevents any ability use |
| Integration with all MVP abilities | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | - | Lunge, Overpower, Knockback, Deflect all create GlobalRecovery |
| Recovery timer UI (circular fill) | ❌ Not Started | [ADR-012 Acceptance](../adr/012-acceptance.md) | Lines 371-377 | Deferred: matches reaction window UI pattern (non-blocking) |
| Client-side prediction | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | - | Both server and client apply recovery locally |

**Category Status:** 6/7 complete (86%)

---

### Tactical Synergies

**Core Mechanic (Lines 383-456):** Certain ability sequences unlock follow-up abilities early during recovery lockout. Using one ability creates "window of opportunity" where tactical follow-ups glow and become available before full lockout expires.

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Synergy detection system | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 388-395 | MVP hardcoded rules: gapcloser→heavy, heavy→push |
| Early unlock during recovery | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 392-395 | SynergyUnlock component marks abilities available before full recovery |
| Visual glow on synergy activation | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md), [action_bar.rs](../../src/client/systems/action_bar.rs) | Lines 418-423 | Bright gold overlay (intentionally VERY bright for testing) |
| Immediate glow appearance | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 418-423 | Glow appears when triggering ability used (not when unlock arrives) |
| Additive glow layer | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 418-423 | Overlay preserves base colors (grey/green/yellow), doesn't replace |
| Synergy window duration | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 422-423 | Glow persists until full recovery, supports future queued input |
| Audio feedback on synergy | ❌ Not Started | [ADR-012 Acceptance](../adr/012-acceptance.md) | Lines 425-428 | Deferred: sound on synergy trigger (non-blocking) |
| Synergy chaining | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 443-447 | MVP chain: Lunge → Overpower (0.5s) → Knockback (instant at 1s) |
| Build-specific synergy patterns | ⏸️ Deferred | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 453-454 | Phase 4: data-driven synergies, needs gear system |
| Discovery through play | ✅ Complete | [ADR-012](../adr/012-ability-recovery-and-synergies.md) | Lines 430-441 | Visual feedback guides learning, glowing abilities self-teach |

**Category Status:** 7/10 complete (70%)

**MVP Synergies Implemented (ADR-012):**
- Gap Closer → Heavy Strike: Lunge unlocks Overpower 0.5s early (at 0.5s remaining instead of 1.0s)
- Heavy Strike → Push: Overpower unlocks Knockback 1.0s early (at 1.0s remaining instead of 2.0s)
- Full chain: Lunge → Overpower (0.5s wait) → Knockback (instant at 1s mark)

**Design Benefits (Lines 449-456):** Rewards tactical thinking over memorized rotations. Self-teaching through visual feedback. Build diversity through different synergy patterns. Accessible depth (works without synergies, better with them).

---

### MVP Abilities (Phase 1)

**Spec MVP Loadout (Lines 624-698):** Sword+Shield warrior with Hardened armor. Starting gear demonstrates gear-skill system.

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Auto-Attack (passive) | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [auto_attack.rs](../../src/server/systems/combat/abilities/auto_attack.rs) | Lines 641-651 | 20 dmg, adjacent, auto-triggers |
| Lunge (Q) - Direct approach | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [lunge.rs](../../src/server/systems/combat/abilities/lunge.rs) | Lines 654-664 | 40 dmg, 20 stam, 4 hex, teleports adjacent to target (gap closer from Sword) |
| Counter (W) - Patient approach | ❌ Not Started | - | Lines 666-676 | 35 stam, reflect first queued threat (from Shield) |
| Fortify (E) - Hardened resilience | ❌ Not Started | - | Lines 679-687 | 40 stam, reduce all queued damage 50% (from Hardened armor) |
| Deflect (R) - Hardened resilience | ✅ Complete | [ADR-009](../adr/009-mvp-ability-set.md) + [deflect.rs](../../src/server/systems/combat/abilities/deflect.rs) | Lines 689-698 | 50 stam, clear all queued threats (from Hardened armor) |

**Implementation Deviations:**
- **Overpower (W)** and **Knockback (E)** were implemented instead of Counter/Fortify per ADR-009
- Spec now specifies Counter/Fortify to better demonstrate gear-skill system
- Overpower/Knockback remain functional but don't align with updated spec's build philosophy

**Category Status:** 3/5 complete (60% - Auto-Attack, Lunge, Deflect match spec; Counter and Fortify not implemented)

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

**Category Status:** 2/6 complete (33%)

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
| Ranged enemy (Forest Sprite) | ✅ Complete | [ADR-010](../adr/010-combat-variety-phase-1.md), [ADR-010 Acceptance](../adr/010-acceptance.md) | Lines 482-490 | Kiting behavior, 5-8 hex optimal, projectile attacks, 40% spawn rate |
| Visual telegraphs | ❌ Not Started | - | Lines 494-504 | Enemy attack warnings |

**Category Status:** 3/4 complete (75%)

---

### Player Combat Build System

**Core Philosophy (Lines 513-583):** Gear determines skills, attributes scale them. 3 systems: Weapons (offense), Armor (defense), Attributes (scaling).

#### Weapons System (Approach Skills)

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Main Hand: Sword (Direct+Evasive) | 🚧 Partial | Implemented (no gear system) | Lines 529 | Skills exist, gear gating not implemented |
| Main Hand: Mace (Direct+Binding) | ❌ Not Started | - | Lines 528 | Post-MVP |
| Main Hand: Whip (Distant+Binding) | ❌ Not Started | - | Lines 530 | Post-MVP |
| Main Hand: Revolver (Distant+Evasive) | ❌ Not Started | - | Lines 531 | Post-MVP |
| Off Hand: Shield (Patient) | 🚧 Partial | Implemented (no gear system) | Lines 534 | Skills exist, gear gating not implemented |
| Off Hand: Dagger (Ambushing) | ❌ Not Started | - | Lines 535 | Post-MVP |
| Weapon skill gating | ❌ Not Started | - | Lines 539 | 6 approach skills per loadout |
| Weapon swapping | ❌ Not Started | - | Lines 542 | Change offensive toolkit |

**Category Status:** 0/8 complete (0% - gear system not implemented, skills exist but not gated)

#### Armor System (Resilience Skills)

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Helm: Mental (Focus, Dispel) | 🚧 Partial | Implemented (no gear system) | Lines 553 | Skills planned, gear gating not implemented |
| Helm: Primal (Enrage, Attune) | ❌ Not Started | - | Lines 554 | Post-MVP |
| Chest: Hardened (Fortify, Deflect) | 🚧 Partial | Implemented (no gear system) | Lines 558 | Deflect exists, Fortify planned, no gear gating |
| Chest: Shielded (Ward, Repel) | ❌ Not Started | - | Lines 557 | Post-MVP |
| Accessory: Vital (Regenerate, Endure) | ❌ Not Started | - | Lines 562 | Post-MVP |
| Accessory: Blessed (Heal, Cleanse) | ❌ Not Started | - | Lines 561 | Post-MVP |
| Armor skill gating | ❌ Not Started | - | Lines 566 | 6 resilience skills per loadout |
| Armor swapping | ❌ Not Started | - | Lines 568 | Counter different threats |

**Category Status:** 0/8 complete (0% - gear system not implemented, some skills exist but not gated)

#### Attributes System

| Feature | Status | ADR/Impl | Spec Reference | Notes |
|---------|--------|----------|----------------|-------|
| Might (damage, stamina pool) | ✅ Complete | [ADR-002](../adr/002-combat-foundation.md) | Line 576 | Scales physical damage and stamina |
| Grace (movement, hit, dodge) | 🚧 Partial | [ADR-010](../adr/010-combat-variety-phase-1.md) | Line 577 | Movement speed implemented, hit/dodge TBD |
| Vitality (HP, armor, stagger) | 🚧 Partial | [ADR-002](../adr/002-combat-foundation.md) | Line 578 | HP and armor complete, stagger TBD |
| Focus (magic, mana, queue) | 🚧 Partial | [ADR-003](../adr/003-reaction-queue-system.md) | Line 579 | Queue capacity implemented, magic scaling exists but unused |
| Instinct (crit, reaction window) | 🚧 Partial | [ADR-003](../adr/003-reaction-queue-system.md) | Line 580 | Reaction window implemented, crits TBD |
| Presence (threat, AoE, CC) | ❌ Not Started | - | Line 581 | Post-MVP |
| Attribute respeccing | ❌ Not Started | - | Line 583 | Post-MVP |

**Category Status:** 1/7 complete (14% - MVP attributes functional, full system incomplete)

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

### 7. Tier Badge Visual UI
- **Spec Says (ADR-010, Lines 76, 127):** Tier badge on target indicator (small "1", "2", or "3" icon)
- **Actually Implemented:** Core tier lock functionality complete, visual UI deferred
- **Rationale:** Bevy 0.16 3D text component API complexity. Visual polish not required for MVP.
- **ADR Reference:** [ADR-010 Acceptance](../adr/010-acceptance.md)

### 8. Tier Badge Visual UI
- **Spec Says (ADR-010, Lines 76, 127):** Tier badge on target indicator (small "1", "2", or "3" icon)
- **Actually Implemented:** Core tier lock filtering works, visual UI deferred
- **Rationale:** Bevy 0.16 3D text component API complexity. Visual polish not required for MVP.
- **ADR Reference:** [ADR-010 Acceptance](../adr/010-acceptance.md) Lines 323-328

### 9. Visual Ring Indicator (Developer Addition)
- **Spec Says (ADR-010):** No mention of visual ring indicator
- **Actually Implemented:** Visual ring around player showing targeting area, resizes based on tier lock
- **Rationale:** Critical UX feedback for spatial tier lock system. Eliminates "is it working?" confusion. Excellent use of developer latitude.
- **Player Assessment:** "Transforms tier lock from abstract to spatial mechanic" ([Player Feedback](../adr/010-player-feedback.md) Lines 75-82, 147-156)
- **ADR Reference:** [ADR-010 Acceptance](../adr/010-acceptance.md) Lines 76-94

### 10. Empty Tier Range Visualization
- **Spec Says (ADR-010, Line 74):** Empty tier shows range cone highlighting
- **Actually Implemented:** Tier lock filtering works, visual feedback deferred
- **Rationale:** Visual overlay not critical for functionality. Tier lock still filters targets correctly.
- **ADR Reference:** [ADR-010 Acceptance](../adr/010-acceptance.md) Lines 330-334

### 11. MVP Ability Set vs Updated Spec
- **Spec Says (combat-system.md, Lines 666-687):** Counter (W) and Fortify (E) as MVP abilities
- **Actually Implemented:** Overpower (W) and Knockback (E) per ADR-009
- **Rationale:** ADR-009 predates updated spec's build system philosophy. Overpower/Knockback functional but don't demonstrate gear-skill relationships as clearly as Counter/Fortify would.
- **ADR Reference:** [ADR-009](../adr/009-mvp-ability-set.md)
- **Status:** Implementation complete, spec evolved. Future alignment needed if demonstrating build system becomes priority.

### 12. Projectile System Removal → Instant Hit Combat
- **Spec Originally Said (ADR-010):** Entity-based projectiles with 4 hexes/sec travel time, dodgeable by moving off target hex
- **Actually Implemented (ADR-011):** Instant hit ranged attacks with attack telegraph visual feedback
- **Rationale:** Physics-based projectile dodging created bullet hell gameplay at scale (multiple ranged enemies firing simultaneously), violating core design pillar "Conscious but Decisive - No twitch mechanics required"
- **Implementation Details:**
  - Removed: Projectile component, projectile systems (750+ lines deleted)
  - Added: Instant damage to reaction queue on cast
  - Added: Attack telegraph system (yellow ball over attacker → hit line on damage apply)
  - Updated: Volley ability uses instant hit mechanics
- **Design Impact:** Skill expression shifted from twitch dodging to reaction queue management (existing, tested system). Positioning still matters (range, kiting, gap closers) without requiring pixel-perfect reflexes.
- **Spec Updated:** combat-system.md Lines 147-208 (Ranged Attacks + Attack Telegraphs sections)
- **ADR Reference:** [ADR-011](../adr/011-movement-intent-system.md) - Combat system refinement during movement intent implementation

### 13. Recovery System: Universal Lockout vs Per-Ability Timers
- **Spec Says (combat-system.md, Lines 352-381):** "Individual ability recovery timers" where "each ability has independent recovery period" and "other abilities remain available"
- **Actually Implemented (ADR-012):** Universal lockout system where using ANY ability locks ALL abilities for that ability's recovery duration
- **Rationale:** Universal lockout creates stronger tactical commitment and weight to ability choices. Each ability's lockout duration reflects its power/impact (Lunge 1s, Overpower 2s, Knockback 0.5s, Deflect 1s). Tactical synergies provide early unlocks during lockout for specific ability sequences, adding skill expression without reducing commitment weight.
- **Implementation Details:**
  - GlobalRecovery component (single per player): `remaining`, `duration`, `triggered_by`
  - SynergyUnlock component (multiple per player): `ability`, `unlock_at`, `triggered_by`
  - MVP synergy chain: Lunge → Overpower (0.5s early) → Knockback (instant at 1s)
  - All 33 tests passing (26 recovery + 7 synergy)
- **Design Impact:** Creates rhythmic combat pacing with meaningful ability choice. Synergies reward tactical sequences without eliminating commitment. Clearer mental model (one global timer vs tracking multiple timers).
- **ADR Reference:** [ADR-012](../adr/012-ability-recovery-and-synergies.md), [ADR-012 Acceptance](../adr/012-acceptance.md)
- **Status:** Accepted and deployed, spec needs update to reflect implementation

---

## Spec Gaps

Features described in spec but not yet in implementation plan:

### Critical Priority
- **Unified Tier Lock Tutorial:** Explicit teaching that tier lock affects both hostiles and allies ([Player Feedback](../adr/010-player-feedback.md) Lines 106-136, 184-212, 323-402) - **MANDATORY before support abilities launch**

### High Priority
- **TAB Cycling:** Manual target selection within tier (Lines 108-115)
- **Counter Ability:** Patient approach skill from Shield (Lines 666-676)
- **Fortify Ability:** Hardened resilience skill from armor (Lines 679-687)
- **Build System Foundation:** Gear-based skill gating system (Lines 513-617)

### Medium Priority
- **Ability Recovery System:** Individual per-ability timers (Lines 352-381)
- **Tactical Synergies:** Recovery reductions + visual glow on tactical sequences (Lines 383-456)
- **Tier Badge UI:** Visual feedback for tier lock (Lines 76, 127)
- **Empty Tier Visualization:** Facing cone range highlighting (Line 74)
- **Critical Hit System:** Instinct-based crits (Lines 387-393)
- **Visual Telegraphs:** Enemy attack warnings (Lines 494-504)

### Low Priority (Post-MVP)
- **Full Weapon System:** 4 main hands + 2 off-hands with skill gating (Lines 523-545)
- **Full Armor System:** 3 armor slots with binary choices and skill gating (Lines 548-569)
- **Gear Acquisition:** Loot, crafting, vendors (Lines 784-791)
- **Ability Slotting:** Choose 4 from available 12 skills (Lines 797-802)
- **Full Reaction Ability Set:** 7 additional reaction abilities (Lines 260-320)
- **Ground Effects:** AOE telegraphs with delayed damage (Lines 163-173)
- **Unavoidable Attacks:** Ultimate-tier mechanics (Lines 175-179)
- **Combat Music:** Audio system integration (Line 443)
- **Stagger System:** Cast interruption mechanics (Lines 419-424)

---

## Open Design Questions

**Spec Reference:** Lines 1025-1072

**Recovery Timers (Lines 1058-1061):**
- Are base recovery durations correct (0.2-0.3s quick, 0.4-0.6s tactical, 0.8-1.2s high-impact)?
- Synergy strength balance (recovery reductions significant enough)?
- Synergy window clarity (is "on fire" visual obvious during combat)?

**Tactical Synergies (Lines 1062-1072):**
- Which abilities should synergize? (need to define synergy pairs/chains per weapon combo)
- Synergy tagging system (ability tags like "gap_closer", "aoe"? explicit pairs?)
- Synergy discovery pacing (should early game have fewer synergies?)
- Multiple synergy sources (if two abilities both trigger synergy, do both glow?)
- Synergy feedback intensity (how bright/obvious should glow be?)
- Synergy audio (what sound plays? ding? whoosh? ability-specific?)
- Synergy chains depth (how many abilities can chain before resources run out?)
- Weapon-specific synergies (does each weapon combo have unique patterns?)
- Build diversity (do different Triumvirate approaches create different synergy opportunities?)
- Enemy AI synergies (should enemies also have synergies, or player-only mechanic?)

**Design Note:** These questions should be answered during implementation of Ability Recovery System and Tactical Synergies features. May require playtesting and iteration.

---

**Document Version:** 1.2
**Last Updated:** 2025-11-07
**Maintained By:** Development team
