# Player Feedback: ADR-008 Combat HUD Implementation

## Document Purpose

This document provides **PLAYER role feedback** on ADR-008: Combat HUD Implementation. The architect drafted ADR-008 to implement MVP requirements from the combat-hud spec, received player feedback, and revised the ADR to address all critical concerns.

**Reviewer:** PLAYER role
**Initial Review Date:** 2025-10-31
**Final Review Date:** 2025-10-31
**Reviewed Document:** ADR-008 (Proposed - Revised)
**Source Spec:** `docs/spec/combat-hud.md`

---

## Executive Summary

**VERDICT: ✅ APPROVED FOR IMPLEMENTATION**

The revised ADR-008 now delivers **100% of Phase 1 Critical features** from the combat-hud spec. All previously missing features have been added:

1. ✅ **Action Bar** (Q/W/E/R abilities) - Added as Phase 2
2. ✅ **Target Detail Frame** (enemy resources/queue) - Added as Phase 5
3. ✅ **Facing Indicator** (arrow + position offset) - Added to Phase 7

The architect has successfully integrated all critical feedback and created a **functionally complete combat HUD** for MVP gameplay testing.

### Approval Summary

**Timeline:** 17-23 days (was 12-16 days, +5-7 days for critical features) ✅ Reasonable
**Scope:** All 5 Phase 1 Critical + 1 High Priority feature ✅ Complete
**Design:** Sound architectural decisions, clear visual language ✅ Excellent
**Player Experience:** Playable, tactical, informed decision-making ✅ Achieved

**Recommendation:** Proceed with implementation.

---

## Revision Summary

### What Changed Between Initial and Revised ADR

**Initial ADR-008 (Lines 1-1118 original):**
- Status: "Proposed"
- Features: 5 of 8 required
- Missing: Action Bar, Target Detail Frame, Facing Indicator
- Timeline: 12-16 days
- Player Verdict: "NOT READY FOR IMPLEMENTATION"

**Revised ADR-008 (Current):**
- Status: "Proposed (Revised)"
- Features: 8 of 8 required ✅
- Added: Action Bar (Decision 6, Phase 2), Target Detail Frame (Decision 7, Phase 5), Facing Indicator (Decision 8, Phase 7)
- Timeline: 17-23 days (+5-7 days)
- Player Verdict: **"APPROVED FOR IMPLEMENTATION"** ✅

---

## Critical Features: Now Addressed

### 1. Action Bar ✅ ADDED (Phase 2)

**Previous Status:** Missing (listed as "future enhancement")
**Current Status:** ✅ Fully designed in ADR-008 lines 646-788

**What Was Added:**

**Location:** Bottom-center, directly below resource bars
**Design:**
- 4 rectangular ability slots (Q, W, E, R)
- Each slot shows:
  - Keybind label (Q/W/E/R, always visible)
  - Ability icon (⚔️, 🔥, 🌀)
  - Resource cost badge (30💧 stamina, 40💠 mana)
  - Cooldown overlay (radial sweep + countdown text)
  - State-indicating border (green=ready, gray=cooldown, red=insufficient resources, orange=out of range, yellow=no target)
- Empty slots show lock icon 🔒

**Player Impact:**
> "Perfect! Now I can see what abilities I have, if I can afford them, and if they're ready. The color-coded borders instantly communicate state - green means go, red means can't afford. This is exactly what I needed."

**Integration:**
- Queries `Stamina`, `Mana` for resource checks
- Queries `Gcd` component for global cooldown overlay
- Future: Queries `EquippedAbilities` component
- MVP: Hardcoded abilities (Q=BasicAttack, E=Dodge, W/R=Empty)

**Timeline:** Phase 2, 2-3 days ✅

---

### 2. Target Detail Frame ✅ ADDED (Phase 5)

**Previous Status:** Not mentioned anywhere in original ADR
**Current Status:** ✅ Fully designed in ADR-008 lines 791-954

**What Was Added:**

**Location:** Top-right corner of screen
**Design:**
- Compact frame (280px × variable height)
- Shows for current target:
  - Entity name + distance in hexes (top header)
  - HP bar with exact numbers (80/100, orange fill)
  - Resource pools if entity has them:
    - Stamina bar (💧 yellow)
    - Mana bar (💠 blue)
  - Threat queue if entity has one:
    - Mini circular indicators (60% of player queue size)
    - Timer rings, attack icons, countdown text
    - Label: "QUEUE:" for clarity
- Auto-shows when target exists, auto-hides when no target
- Scales content by entity type:
  - Basic enemies: Name + HP only
  - Elite/players: Everything (HP + resources + queue)

**Player Impact:**
> "This is game-changing. Now I can answer critical questions: 'Can they dodge?' (see their stamina). 'Is their queue full?' (see threat indicators). 'How close to death?' (exact HP numbers). The auto-show behavior is perfect - no extra keypresses, just target and see."

**Tactical Decisions Enabled:**
- **Resource pressure:** "They have 20/80 stamina - they can't afford to dodge, I should attack now"
- **Queue manipulation:** "Their queue is 2/3 full with 0.6s on the oldest threat - if I attack now, their queue overflows"
- **Kill timing:** "They're at 58/100 HP - one more Fire Bolt (40 damage) will probably finish them"

**Integration:**
- Queries `Target` component (from ADR-004)
- Queries target entity's `Health`, `Stamina`, `Mana`, `ReactionQueue`
- Calculates distance using `Loc` component (hex distance)

**Timeline:** Phase 5, 2-3 days ✅

---

### 3. Facing Indicator ✅ ADDED (Phase 7)

**Previous Status:** Not mentioned in original ADR
**Current Status:** ✅ Fully designed in ADR-008 lines 957-1046

**What Was Added:**

**Design:** Dual reinforcement (arrow + position offset)

**Component 1: Character Position Offset**
- Character positioned on hex to indicate facing
- Offset 15% of hex radius toward facing direction
- Already in combat-system spec, now explicitly documented

**Component 2: Directional Arrow (New)**
- Small arrow sprite above character (0.3 world units)
- Points in facing direction (rotates with Heading)
- White, semi-transparent (doesn't obscure)
- Always visible

**Component 3: Optional Facing Cone Overlay**
- Faint 60° cone on ground (blue, very transparent)
- Shows exact targeting area
- Toggleable (TAB to show, release to hide)
- OR: Always on during combat, off outside combat

**Player Impact:**
> "The arrow is clear and always visible. After I stop moving, I immediately know which way I'm facing. The optional cone overlay is great for tactical players who want to see the exact targeting area."

**Why This Matters:**
- Directional targeting depends on 60° facing cone (ADR-004)
- Tier lock (close/medium/far) searches in facing direction
- Without clear facing indication, targeting feels arbitrary

**Integration:**
- Queries `Heading` component (from ADR-004)
- Updates arrow rotation to match heading angle
- Optional: Toggle cone overlay visibility

**Timeline:** Phase 7 (combined with combat state feedback), 2-3 days ✅

---

## Features That Were Already Good

These features were well-designed in the original ADR and remain unchanged:

### ✅ Resource Bars (Phase 1)
- Bottom-center, horizontal layout
- Red (Health), Yellow (Stamina), Blue (Mana)
- Shows current/max values (120 / 150)
- Always visible
- **Player Feedback:** "Perfect, no changes needed."

### ✅ Reaction Queue Display (Phase 3)
- Top-center, circular threat indicators
- Timer rings (depleting clockwise)
- Attack type icons (⚔️, 🔥)
- Urgency animations (pulsing when < 20% remaining)
- Queue overflow warnings (red pulsing on leftmost threat)
- **Player Feedback:** "Instantly readable. The circular design screams 'urgent threat' and the timer rings are intuitive."

### ✅ Target Indicators (Phase 4)
- Red circle on hostile, green circle on ally
- World-space (follows entity movement)
- Tier lock badge (shows range lock: 1, 2, or 3)
- TAB lock marker (white outline when manually selected)
- **Player Feedback:** "Clear and unambiguous. Genre-standard design."

### ✅ World-Space Health Bars (Phase 6)
- Above entities, red fill with gray background
- Smart visibility (show in combat or when damaged)
- Always visible for players
- Hidden for NPCs when out of combat and full HP
- **Player Feedback:** "Good for quick reference. For exact values, the target detail frame is key."

### ✅ Combat State Feedback (Phase 7)
- Subtle vignette (dark red edges, 0.15 opacity)
- Resource bar glow (orange outline when in combat)
- Smooth fade in/out transitions
- **Player Feedback:** "Subtle but effective. Doesn't obscure gameplay."

### ✅ Architectural Decisions
- Visual language (circles=urgent, rectangles=stable)
- Color coding (red=health, yellow=stamina, blue=mana, green=allies)
- UI layering (world-space, screen-space, floating)
- **Player Feedback:** "Excellent foundation. Clear visual hierarchy."

---

## Player Experience: Complete MVP

### Combat Scenario: Fighting a Wild Dog (Revised ADR-008)

**What I Can See:**

**My State (Bottom-Center):**
- ✅ Health: 80/100 (red bar)
- ✅ Stamina: 45/80 (yellow bar)
- ✅ Mana: 60/60 (blue bar)
- ✅ **Action bar:**
  - **Q: Basic Attack** (FREE, ready, green border, ⚔️ icon)
  - **W: Fire Bolt** (30 mana, ready, green border, 🔥 icon)
  - **E: Dodge** (30 stamina, ready, green border, 🌀 icon)
  - **R: [Empty]** (locked, 🔒 icon)

**Threats (Top-Center):**
- ✅ Incoming threat: ⚔️ with 0.8s timer (orange ring depleting)

**Target (World + Top-Right):**
- ✅ Red circle on Wild Dog (targeted)
- ✅ **Target frame (top-right):**
  - **Wild Dog - 3 hexes**
  - **HP: 58/100** (orange bar with exact numbers)
  - **Stamina: 20/80** (yellow bar - low!)
  - **Queue: (⚔️) 0.6s** (they have one threat, distracted)

**Facing (Character):**
- ✅ **Arrow above character** (pointing at Wild Dog)
- ✅ **Character offset** (positioned toward Wild Dog on hex)

---

### Player Internal Monologue (Tactical Decision-Making)

> "Okay, threat incoming in 0.8s. I need to dodge - I have 45 stamina, Dodge costs 30, so I can afford it (green border confirms it's ready).
>
> Looking at the Wild Dog in my target frame: they're at 58/100 HP (almost dead), they only have 20/80 stamina left (probably can't dodge), and they have a threat in their queue with 0.6s remaining (they're distracted).
>
> My plan:
> 1. Press **E** to dodge this incoming threat (0.8s)
> 2. Immediately press **Q** (Basic Attack) while they're busy resolving their own threat (0.6s) - they won't be able to dodge because they only have 20 stamina and dodging costs 30
> 3. If that doesn't kill them (58 HP left), press **W** (Fire Bolt) for 30 mana to finish them off
>
> *Executes plan, Wild Dog dies*
>
> That felt great! I could see exactly what was happening, make informed decisions, and execute a tactical plan based on visible information. This is how combat should feel."

---

### Player Feeling

**Initial ADR (60% complete):** Confused, frustrated, flying blind
**Revised ADR (100% complete):** **Informed, tactical, in control** ✅

---

## Timeline Assessment

**Revised Timeline:** 17-23 days (was 12-16 days, +5-7 days)

**Breakdown:**
- Phase 1: Resource Bars (2 days)
- Phase 2: **Action Bar (2-3 days)** ← Added
- Phase 3: Reaction Queue (3-4 days)
- Phase 4: Target Indicators (2 days)
- Phase 5: **Target Detail Frame (2-3 days)** ← Added
- Phase 6: Health Bars (2-3 days)
- Phase 7: Combat State + **Facing Indicator (2-3 days)** ← Modified
- Phase 8: Polish (2-3 days)

**Player Assessment:** ✅ **Timeline increase is reasonable and necessary**

The +5-7 days accounts for:
- Action bar implementation (complex state management, cooldown visualization)
- Target detail frame (dynamic content scaling by entity type, distance calculation)
- Facing indicator (arrow + optional cone overlay)

**This is NOT scope creep** - these features were always in the combat-hud spec as Phase 1 Critical. The initial ADR underscoped by implementing only combat-system.md requirements instead of the full combat-hud.md spec.

**Comparison:**
- Shipping incomplete MVP (12-16 days) → playtest feedback "combat unplayable" → rework action bar + target frame anyway → **total 20-25 days + frustration**
- Shipping complete MVP (17-23 days) → playtest feedback "polish requests only" → iterate on feel/balance → **total 17-23 days + positive momentum**

**Recommendation:** Accept the 17-23 day timeline. Building it right the first time is faster than building twice.

---

## Remaining Minor Notes

These are **NOT blocking concerns**, just observations for future consideration:

### 1. Action Bar: Hardcoded Abilities (MVP)

**Current Design (ADR-008 line 1272):**
> MVP: Hardcoded abilities (Q=BasicAttack, E=Dodge, W/R=Empty)

**Player Note:** This is fine for MVP. Eventually, we'll need an `EquippedAbilities` component for ability customization, but hardcoding Q/E for initial testing is pragmatic.

**Future:** When ability system expands (more than 2 abilities), add ability swapping/customization.

---

### 2. Target Frame: Entity Type Scaling

**Current Design (ADR-008 lines 892-904):**
```rust
match entity_type {
    EntityType::BasicEnemy => {
        // Show: Name, distance, HP only
    },
    EntityType::EliteEnemy | EntityType::Player => {
        // Show: Everything (HP, resources, queue)
    },
}
```

**Player Note:** This is excellent design. Basic enemies (Wild Dog) don't need resource/queue display yet (they don't use stamina/mana tactically or have reaction queues in MVP).

**Future:** When elite enemies or PvP is added, the target frame automatically scales up to show full details.

---

### 3. Facing Cone Overlay: Toggle Behavior

**Current Design (ADR-008 lines 989-995):**
> Optional: Toggle cone overlay visibility
> Toggle key: TAB (hold to show, release to hide)
> OR: Always on during combat, off outside combat

**Player Note:** I prefer **"hold TAB to show"** for MVP. This gives tactical players the option without cluttering the screen for everyone else.

**Future Playtesting:** If players consistently request "always on," add a settings toggle.

---

### 4. GCD Overlay vs Individual Cooldowns

**Current Design (ADR-008 lines 710-721):**
> Cooldown overlay (radial sweep + countdown text)

**Player Note:** The design mentions both GCD (0.5s global cooldown from ADR-002) and individual ability cooldowns. The ADR should clarify:
- **GCD active:** Do ALL ability slots dim simultaneously?
- **Individual cooldown:** Do only specific slots dim?

**Recommendation:** GCD dims all slots with a global overlay, individual cooldowns dim only that slot. This communicates "can't use anything" vs "that specific ability is on cooldown."

**Not Blocking:** Developer can implement either way and adjust based on feel.

---

### 5. UI Polish Phase Priorities

**Current Phase 8 (ADR-008 lines 1465-1501):**
- Timer ring smoothness (60fps)
- Health bar interpolation (0.2s lerp)
- Object pooling (50 health bars)
- Text update optimization (dirty checking)
- Camera culling (off-screen entities)

**Player Note:** All of these are excellent optimizations, but prioritize **visual smoothness over performance** for MVP:

**Priority 1:** Timer ring smoothness (combat feel depends on readable timers)
**Priority 2:** Health bar interpolation (snapping feels janky)
**Priority 3:** Object pooling (only if profiling shows issue)
**Priority 4:** Text optimization (only if profiling shows issue)
**Priority 5:** Camera culling (nice to have, not critical for 10-20 NPCs)

**Rationale:** MVP playtest happens with ~10 NPCs max. Premature optimization can delay testing without measurable benefit. Optimize when profiling identifies bottlenecks.

---

## Spec Compliance Checklist

### Phase 1: Critical (Blocks MVP) - From combat-hud.md

- [x] **1. Target indicator (hostile)** - ADR-008 Phase 4 ✅
- [x] **2. Reaction queue visual redesign** - ADR-008 Phase 3 ✅
- [x] **3. Action bar addition** - ADR-008 Phase 2 ✅ **ADDED**
- [x] **4. Enemy health bars (world space)** - ADR-008 Phase 6 ✅
- [x] **5. Target detail frame** - ADR-008 Phase 5 ✅ **ADDED**

**Status: 5 of 5 complete (100%)** ✅

---

### Phase 2: High Priority - From combat-hud.md

- [x] **6. Facing indicator** - ADR-008 Phase 7 ✅ **ADDED**
- [ ] **7. Range feedback** - NOT IN ADR-008 (acceptable for MVP)
- [ ] **8. Tier lock indicators** - Partial (tier badges on target indicator)
- [x] **9. Combat state indicator** - ADR-008 Phase 7 ✅

**Status: 2 of 4 complete (50%)** - Acceptable for MVP

**Note on #7 (Range Feedback):**
The spec suggests "out-of-range abilities dim/gray out" (combat-hud.md lines 417-437). The revised ADR includes this as part of action bar state indicators:
> OutOfRange => Color::rgb(0.9, 0.5, 0.0), // Orange - target too far

So this is actually **partially implemented** via action bar border colors. Not as detailed as the spec's "Option A + B + C," but sufficient for MVP.

---

### Phase 3: Polish - From combat-hud.md

- [ ] **10. Ally target indicator (green)** - Designed but MVP may not need allies yet
- [x] **11. Damage numbers** - ADR-005 (already implemented) ✅
- [x] **12. Manual lock feedback (TAB)** - ADR-008 Phase 4 (TAB markers on indicator) ✅
- [ ] **13. Status effects** - NOT IN ADR-008 (acceptable, system not implemented yet)

**Status: Future/Acceptable for MVP**

---

## Final Approval

### What the Revised ADR Delivers

**Functionally Complete Combat HUD:**
- ✅ See my resources (Health/Stamina/Mana)
- ✅ See my abilities (Q/W/E/R with costs, cooldowns, states)
- ✅ See incoming threats (reaction queue with timers)
- ✅ See what I'm targeting (red/green circles)
- ✅ See enemy state (target frame with HP/resources/queue)
- ✅ See enemy approximate HP (world-space health bars)
- ✅ See my facing direction (arrow + position offset)
- ✅ Know when I'm in combat (vignette + resource glow)

**Player Can Answer All Critical Questions:**
- "What abilities do I have?" → Action bar
- "Can I afford this ability?" → Action bar cost badges
- "Is this ability ready?" → Action bar cooldown overlays
- "What threats am I facing?" → Reaction queue
- "Who am I targeting?" → Red/green circles
- "How much HP does enemy have?" → Target frame (exact) + health bar (approximate)
- "Can enemy dodge?" → Target frame (see their stamina)
- "Is enemy's queue full?" → Target frame (see their threats)
- "Which way am I facing?" → Arrow + character offset
- "Am I in combat?" → Vignette + resource bar glow

**Tactical Gameplay Enabled:**
- Queue manipulation (overflow enemy queue)
- Resource pressure (attack when enemy low on stamina)
- Timing windows (attack when enemy distracted by threats)
- Informed decision-making (see exact values, not guessing)

---

### Player Verdict

**APPROVED FOR IMPLEMENTATION** ✅

**Reasoning:**
1. All Phase 1 Critical features included (5 of 5)
2. High Priority features included (facing indicator)
3. Architectural decisions are sound
4. Timeline is reasonable (+5-7 days for critical features)
5. Combat will be playable, tactical, and enjoyable

**Next Steps:**
1. ARCHITECT: Mark ADR-008 as "Accepted"
2. DEVELOPER: Begin implementation (Phase 1: Resource Bars)
3. PLAYER: Available for clarification during implementation
4. PLAYER: Will review implemented features during MVP playtest

---

### What Success Looks Like

**MVP Playtest Feedback (Expected):**
- ✅ "I can see everything I need to make decisions"
- ✅ "The action bar makes it clear what abilities I have"
- ✅ "Seeing enemy stamina in the target frame is game-changing"
- ✅ "The reaction queue timer rings are really clear"
- ✅ "I know exactly what's happening in combat"
- 💬 Minor feedback: "Can the action bar be 10px larger?" (polish requests)
- 💬 Minor feedback: "Timer countdown text is hard to read" (toggleable, already planned)

**MVP Playtest Feedback (NOT Expected):**
- ❌ "I don't know what my abilities do" (action bar solves this)
- ❌ "I can't tell if enemy can dodge" (target frame solves this)
- ❌ "I'm pressing keys randomly" (action bar + target frame solve this)
- ❌ "Combat feels like luck" (information visibility enables skill expression)

---

## Comparison: Initial vs Revised

| Feature | Initial ADR | Revised ADR | Player Impact |
|---------|-------------|-------------|---------------|
| **Resource Bars** | ✅ Included | ✅ Included | No change, already good |
| **Action Bar** | ❌ Future enhancement | ✅ Phase 2 | Can see abilities, costs, cooldowns |
| **Reaction Queue** | ✅ Included | ✅ Included | No change, already good |
| **Target Indicators** | ✅ Included | ✅ Included | No change, already good |
| **Target Frame** | ❌ Not mentioned | ✅ Phase 5 | Can see enemy HP, resources, queue |
| **Health Bars** | ✅ Included | ✅ Included | No change, already good |
| **Facing Indicator** | ❌ Not mentioned | ✅ Phase 7 | Know heading direction clearly |
| **Combat State** | ✅ Included | ✅ Included | No change, already good |
| **Timeline** | 12-16 days | 17-23 days | +5-7 days for critical features |
| **Completeness** | 60% (5 of 8) | 100% (8 of 8) | Functionally complete MVP |
| **Player Verdict** | ❌ NOT READY | ✅ APPROVED | Playable, tactical, enjoyable |

---

## Acknowledgment to Architect

The architect demonstrated **excellent responsiveness to player feedback**:

1. **Listened to concerns:** Read detailed feedback, understood criticisms
2. **Accepted scope clarification:** Recognized combat-hud.md as authoritative
3. **Made substantial additions:** Added 3 major features (action bar, target frame, facing)
4. **Updated timeline appropriately:** +5-7 days for additional work
5. **Maintained architectural quality:** New features follow same design principles
6. **Documented decisions thoroughly:** Each addition has full design section

**This is exemplary cross-role collaboration.** PLAYER identified gaps, ARCHITECT addressed them, and the result is a **significantly stronger MVP** that will deliver a better playtest experience.

---

## Conclusion

**ADR-008 (Revised) is APPROVED FOR IMPLEMENTATION.**

The revised ADR delivers a **complete, playable, tactical combat HUD** that fulfills all Phase 1 Critical requirements from the combat-hud spec. Players will have the information they need to make informed decisions, execute tactical plans, and enjoy combat.

**Timeline:** 17-23 days (reasonable for scope)
**Scope:** 8 of 8 features (100% complete)
**Quality:** Excellent architectural decisions, clear visual language
**Player Experience:** Informed, tactical, in control

**Recommendation:** Proceed with implementation. Looking forward to playtesting the MVP! 🎮

---

**Document Status:** Final Approval
**Date:** 2025-10-31
**Next Review:** After MVP implementation, during playtest phase
