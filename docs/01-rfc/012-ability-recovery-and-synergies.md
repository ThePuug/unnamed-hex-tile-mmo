# RFC-012: Ability Recovery System and Tactical Synergies

## Status

**Implemented** - 2025-11-07

## Feature Request

### Player Need

From player perspective: **Tactical combat with meaningful ability sequencing** - Combat should reward smart choices, not memorized rotations or button mashing.

**Current Problem:**
Without recovery variety and synergies:
- All abilities share identical 0.5s GCD (no commitment differentiation)
- No reward for tactical sequencing - all ability orders feel the same
- High-impact abilities (Overpower) have same recovery as quick reactions (Knockback)
- Resource costs are the only throttle, leading to binary "spam until empty" gameplay
- Combat pacing feels uniform and monotonous

**We need a system that:**
- Makes heavy abilities feel impactful (longer recovery reflects commitment)
- Rewards tactical adaptation through ability synergies
- Avoids forcing memorized rotations (discover synergies naturally)
- Creates fluid combat flow (not artificial delays between abilities)
- Maintains "conscious but decisive" philosophy (no GCD spam)

### Desired Experience

Players should experience:
- **Variable Pacing:** Heavy strikes feel impactful (2s recovery), quick reactions stay responsive (0.5s)
- **Tactical Depth:** Ability order matters - Lunge → Overpower feels different from Overpower → Knockback
- **Natural Discovery:** Glowing abilities guide players to synergies without tutorials
- **Fluid Combos:** Synergies reduce recovery, enabling smooth ability chains
- **Skill Expression:** Skilled players chain glowing abilities early, creating satisfying flow state
- **Build Diversity:** Different weapons/armor unlock unique synergy patterns (future)

### Specification Requirements

**MVP Recovery System:**

**1. Universal Lockout Pattern:**
- Using an ability locks ALL abilities for that ability's recovery duration
- Recovery durations vary by commitment weight:
  - Lunge (Gap Closer): 1.0s lockout
  - Overpower (Heavy Strike): 2.0s lockout
  - Knockback (Push): 0.5s lockout
  - Deflect (Defensive): 1.0s lockout
- Single `GlobalRecovery` component tracks lockout (not per-ability cooldowns)
- Replaces fixed 0.5s GCD with variable recovery

**2. Tactical Synergies:**
- Certain ability sequences unlock specific abilities early during lockout
- MVP Synergy Chain:
  - Lunge → Overpower unlocks 0.5s early (available at 0.5s instead of 1.0s)
  - Overpower → Knockback unlocks 1.0s early (available at 1.0s instead of 2.0s)
- Synergy glow appears IMMEDIATELY when ability used (not when window opens)
- Glow persists until full recovery completes
- Using glowing ability consumes synergy

**3. Visual Feedback:**
- **Additive Glow:** Gold border + particles layered on base state color
  - Grey + Gold Glow = "Locked but will unlock early"
  - Green + Gold Glow = "Unlocked via synergy, ready to use"
- **Circular Progress:** Recovery timer around ability icons (like reaction queue UI)
- **Audio Cues:** Synergy trigger sound + synergy use sound
- **Pulsing Animation:** Draws attention to glowing abilities

### MVP Scope

**Phase 1 includes:**
- GlobalRecovery component (universal lockout system)
- Per-ability recovery durations (Lunge 1s, Overpower 2s, Knockback 0.5s, Deflect 1s)
- SynergyUnlock component (early unlock marker)
- MVP synergy rules (Lunge → Overpower, Overpower → Knockback)
- Synergy glow UI (additive gold border + particles)
- Audio feedback (trigger + use sounds)
- Recovery timer UI (circular progress on action bar)

**Phase 1 excludes:**
- Data-driven synergies (hardcoded MVP rules only - Phase 2)
- Attribute scaling for recovery (fixed durations for MVP - Phase 2)
- Multiple synergy sources per ability (one synergy per ability - Phase 2)
- NPC synergies (player-only mechanic for MVP - Phase 2+)

### Priority Justification

**HIGH PRIORITY** - Improves combat feel, adds tactical depth, enables build diversity.

**Why high priority:**
- Recovery variety: Makes abilities feel distinct (Overpower commits you, Knockback stays reactive)
- Synergies: Creates emergent gameplay (discover optimal sequences through experimentation)
- Self-teaching: Glow guides players naturally (no tutorial required)
- Build diversity foundation: Future weapons/armor can define unique synergy patterns

**Benefits:**
- Tactical combat without memorized rotations
- Skill expression through synergy chaining
- Satisfying flow state (glowing abilities → smooth combos)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Universal Lockout + Early Unlock System**

#### Core Mechanism

**Universal Lockout:**
- `GlobalRecovery` component replaces per-ability cooldowns
- Using an ability inserts `GlobalRecovery { remaining, duration, triggered_by }`
- ALL abilities check `GlobalRecovery` to determine if usable
- Lockout expires → component removed → all abilities available

**Synergy Early Unlock:**
- Synergy detection runs immediately after ability use (same frame)
- `SynergyUnlock` component marks which abilities can be used early
- Contains: `ability_key` (which ability), `unlock_at` (when during lockout), `triggered_by` (which ability triggered)
- Ability execution checks: if `SynergyUnlock` exists AND `recovery.remaining <= synergy.unlock_at`, allow early use
- Using synergized ability removes its `SynergyUnlock` component

**Additive Glow UI:**
- Glow rendered as separate layer on top of base ability state color
- Gold border + particle effects + brightness boost
- Does NOT replace green/yellow/grey state indicators
- Glow persists entire lockout duration (spec requirement)

#### Performance Projections

**GlobalRecovery:**
- Single component per player (replaces N per-ability cooldowns)
- Updated every frame (decrement timer)
- Overhead: negligible (single component vs. 4-12 before)

**SynergyUnlock:**
- Multiple components per player (one per synergized ability)
- MVP: Maximum 2 synergies active (Lunge→Overpower, Overpower→Knockback)
- Detection overhead: Runs once per ability use (not every frame)
- Cleanup overhead: Runs when `GlobalRecovery` expires (rare event)

**UI Rendering:**
- Glow effects: Border + particles per glowing ability
- Maximum 2 glowing abilities simultaneously (MVP synergies)
- Particle spawning: 2-3 particles per frame per glowing ability
- Overhead: < 1ms per frame (standard Bevy UI rendering)

**Development Time:**
- Phase 1 (MVP): 5-8 days (recovery system + synergies + UI)

#### Technical Risks

**1. UI Clarity (Additive Glow)**
- *Risk:* Gold glow + base state color might be confusing (3 visual layers)
- *Mitigation:* Playtest extensively, adjust glow opacity if needed
- *Frequency:* Visual design iteration, not technical blocker

**2. Lockout Duration Balancing**
- *Risk:* 2s lockout (Overpower) might feel too punishing, 0.5s (Knockback) too short
- *Mitigation:* Start with longer MVP durations for clarity, tune down to spec ranges (0.2-1.2s) after validation
- *Impact:* Balancing issue, not technical blocker

**3. Synergy Discovery**
- *Risk:* Players might not notice glow during combat chaos
- *Mitigation:* Audio cues + pulsing animation + immediate glow (not delayed)
- *Frequency:* UX iteration, addressable through feedback

**4. Animation Timing**
- *Risk:* Lockout must sync with ability animations for feel (e.g., Overpower animation 1.5s, lockout 2s)
- *Mitigation:* Lockout starts on ability use (not animation end), overlap is acceptable
- *Impact:* Design decision, not technical blocker

### System Integration

**Affected Systems:**
- Ability execution (trigger recovery lockout on use)
- Combat HUD (action bar icons show recovery progress + glow)
- Audio system (synergy trigger + use sounds)
- GCD system (REMOVED - replaced by GlobalRecovery)

**Compatibility:**
- ✅ Extends ADR-008 Combat HUD (action bar icons)
- ✅ Uses ADR-009 MVP abilities (Lunge, Overpower, Knockback, Deflect)
- ✅ Removes fixed GCD (ADR-011 pattern no longer needed)
- ✅ Reuses reaction queue timer UI pattern (circular progress)

### Alternatives Considered

#### Alternative 1: Per-Ability Cooldowns (No Universal Lockout)

Each ability has independent cooldown, no lockout.

**Rejected because:**
- Encourages button mashing (spam any available ability)
- No pacing rhythm (abilities fire whenever off cooldown)
- Violates "conscious but decisive" design pillar (too many simultaneous choices)

#### Alternative 2: Fixed GCD + Cooldown Reduction

Keep 0.5s GCD, synergies reduce ability cooldowns.

**Rejected because:**
- GCD still creates uniform pacing (all abilities feel same)
- Cooldown reduction doesn't reward sequencing (just makes abilities available faster)
- Doesn't create fluid combos (still locked for 0.5s between abilities)

#### Alternative 3: Rotation System (Memorized Combos)

Define fixed ability sequences that grant bonuses (like combo system in fighting games).

**Rejected because:**
- Forces memorized rotations (violates tactical adaptation goal)
- Punishes experimentation (only "correct" sequences work)
- Not accessible (requires learning specific inputs)

#### Alternative 4: Glow Delayed Until Window Opens

Synergy glow appears when unlock time reached (not immediately).

**Rejected because:**
- Delayed feedback feels less responsive
- Players might not notice glow (appears mid-lockout, easy to miss)
- Spec requires immediate glow (Line 422: "glow appears when ability is used")

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Universal lockout + early unlock pattern creates commitment-based pacing without forcing rotations. Immediate glow guides players to optimal sequences naturally.

**Synergy design philosophy:** Synergies are tactical rewards, not requirements. Combat works without synergies (base lockouts acceptable), but synergies create flow state for skilled players.

**Extensibility:**
- Future: Data-driven synergy rules (weapons/armor define unique patterns)
- Future: Attribute scaling (Instinct reduces lockout duration?)
- Future: Multi-source synergies (multiple abilities trigger same target)

### PLAYER Validation

**From combat-system.md spec:**

**Success Criteria:**
- ✅ Variable recovery (spec Lines 352-381)
- ✅ Tactical synergies (spec Lines 383-456)
- ✅ Immediate glow feedback (spec Line 422)
- ✅ Additive visual layer (spec Lines 418-420)

**Tactical Depth:**
- Lunge → Overpower: Close gap, capitalize with heavy strike
- Overpower → Knockback: Heavy hit destabilizes, push to create space
- Emergent gameplay: Discover optimal sequences through experimentation

**Skill Expression:**
- Beginner: Uses abilities when available (base lockouts)
- Intermediate: Notices glow, uses synergized abilities
- Expert: Chains synergies for maximum uptime, creates fluid flow state

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Universal lockout pattern solid, synergy detection clean, additive glow preserves base UI
- PLAYER: ✅ Creates tactical depth, skill ceiling, natural discovery through glow feedback

**Scope Constraint:** Fits in one SOW (5-8 days for 3 phases)

**Dependencies:**
- ADR-008: Combat HUD (action bar integration)
- ADR-009: MVP Ability Set (abilities receiving recovery timers)
- ADR-003: Reaction Queue System (circular timer UI pattern)

**Next Steps:**
1. ARCHITECT creates ADR-017 documenting universal lockout architecture
2. ARCHITECT creates SOW-012 with 3-phase implementation plan
3. DEVELOPER begins Phase 1 (recovery system foundation)

**Date:** 2025-11-07
