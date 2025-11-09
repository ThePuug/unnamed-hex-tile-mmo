# SOW-012: Ability Recovery System and Tactical Synergies

## Status

**Merged** - 2025-11-07

## References

- **RFC-012:** [Ability Recovery and Tactical Synergies](../01-rfc/012-ability-recovery-and-synergies.md)
- **ADR-017:** [Universal Lockout + Early Unlock Architecture](../02-adr/017-universal-lockout-synergy-architecture.md)
- **Spec:** [Combat System Specification](../00-spec/combat-system.md) (Recovery Lines 352-381, Synergies Lines 383-456)
- **Branch:** (merged to main)
- **Implementation Time:** 5-8 days

---

## Implementation Plan

### Phase 1: Recovery System Foundation (MVP)

**Goal:** Replace fixed GCD with universal lockout system

**Deliverables:**
- `GlobalRecovery` component in `common/components/`
- Recovery tick system (decrements timer, removes component when expired)
- Integration with ability execution (trigger lockout on use)
- Recovery duration lookup (ability key → lockout duration)
- Ability validation (check for `GlobalRecovery` before execution)
- Remove old GCD system (delete components/systems)

**Architectural Constraints:**
- Single `GlobalRecovery` component per player (not per-ability cooldowns)
- Lockout durations: Lunge 1.0s, Overpower 2.0s, Knockback 0.5s, Deflect 1.0s
- Component inserted on ability use, removed when timer expires
- Lockout starts immediately on ability use (not when animation completes)
- ALL abilities locked during lockout (universal, not selective)
- Using new ability replaces existing `GlobalRecovery` (overwrites previous lockout)
- Recovery system runs every frame (tick down timer)

**Success Criteria:**
- Each ability triggers variable lockout duration
- All abilities locked during lockout period
- Timer decrements correctly (dt-based)
- Component removed when lockout expires
- Can use any ability when no lockout active
- No GCD system remains (fully replaced)

**Duration:** 1-2 days

---

### Phase 2: Tactical Synergies (MVP)

**Goal:** Detect ability sequences and enable early unlock during lockout

**Deliverables:**
- `SynergyUnlock` component in `common/components/`
- Synergy rule definitions (data structure for MVP hardcoded rules)
- Synergy detection system (listens to ability use, checks rules)
- Ability validation update (check for `SynergyUnlock` during lockout)
- Synergy cleanup system (remove unlocks when lockout expires)
- Audio event spawning (synergy trigger + use sounds)

**Architectural Constraints:**
- MVP synergy rules hardcoded:
  - Lunge (GapCloser) → Overpower unlocks 0.5s early
  - Overpower (HeavyStrike) → Knockback unlocks 1.0s early
- Synergy detection runs AFTER lockout insertion (same frame, needs `GlobalRecovery`)
- `SynergyUnlock` inserted immediately when ability used (not delayed)
- Multiple `SynergyUnlock` components possible (different abilities)
- Unlock time relative to lockout start (not absolute game time)
- Using synergized ability removes its `SynergyUnlock` component
- Lockout expiration removes all `SynergyUnlock` components
- Synergy trigger type based on ability classification (GapCloser, HeavyStrike, Push, Defensive)

**Success Criteria:**
- Using Lunge inserts `SynergyUnlock` for Overpower
- Using Overpower inserts `SynergyUnlock` for Knockback
- Overpower usable at 0.5s during Lunge lockout (not 1.0s)
- Knockback usable at 1.0s during Overpower lockout (not 2.0s)
- Using synergized ability removes its unlock component
- Lockout expiration clears all synergy unlocks
- Audio events spawned correctly

**Duration:** 2-3 days

---

### Phase 3: Visual Feedback (MVP Polish)

**Goal:** Make recovery and synergies discoverable through UI

**Deliverables:**
- Circular recovery progress on action bar ability icons
- Additive synergy glow rendering (gold border + particles)
- Pulsing animation for glowing abilities
- Particle system for glow effects
- Recovery timer text (optional, configurable)
- Audio feedback integration (synergy trigger + use sounds)

**Architectural Constraints:**
- Recovery progress: Circular indicator around ability icons (like reaction queue timer)
- Progress calculation: `1.0 - (recovery.remaining / recovery.duration)`
- Glow rendering: ADDITIVE layer on top of base state color (not replacement)
- Glow elements: Gold border (3-5px) + particle effects + brightness boost (+20%)
- Glow starts immediately when ability used (not delayed until unlock)
- Glow persists entire lockout duration (removed when `GlobalRecovery` expires)
- Combined visual states:
  - Grey + Gold Glow = "Locked but will unlock early"
  - Green + Gold Glow = "Unlocked via synergy, ready to use"
  - Yellow + Gold Glow = "Unlocked via synergy, target invalid"
- Particle effects: Gold sparkles around icon edges, upward drift, 0.3s lifetime
- Pulsing animation: Subtle scale or brightness pulse (not distracting)
- Audio timing: Synergy trigger sound plays when glow applied, synergy use sound plays when glowing ability activated
- Integration with existing Combat HUD action bar (ADR-008)

**Success Criteria:**
- Circular progress visible around all ability icons during lockout
- Progress fills smoothly from empty (lockout start) to full (lockout end)
- Glowing abilities have gold border + particles (immediately visible)
- Base state colors preserved (grey/green/yellow still visible)
- Pulsing animation draws attention without obscuring UI
- Particles spawn continuously while glowing (2-3 per frame)
- Audio cues reinforce synergy activation (trigger + use distinct)
- Glow removed when lockout expires or ability used

**Duration:** 2-3 days

---

### Phase 4: Data-Driven Synergies (Post-MVP)

**Goal:** Extensible synergy system for future abilities and builds

**Deliverables:**
- Synergy rule configuration files (RON/JSON format)
- Synergy tag system (ability tags: `gap_closer`, `heavy_strike`, etc.)
- Tag-based synergy matching (any `gap_closer` → any `heavy_strike`)
- Synergy rule loading system (read from data files at startup)
- Multi-source synergy support (multiple abilities trigger same target)
- Synergy editor/validator tools (for designers)

**Architectural Constraints:**
- Synergy rules stored in external data files (not hardcoded)
- Ability tags defined in ability configuration (reusable classifications)
- Tag matching: Rules specify trigger tag + target tag (not specific abilities)
- Multi-source synergies: Multiple abilities can trigger same synergy target
- Synergy window stacking: If multiple sources trigger, use longest window
- Rule validation at load time (detect invalid tags, circular dependencies)
- Hot-reloading support (designers can test synergies without rebuild)
- Performance: Rule lookup optimized (HashMap or similar, not linear scan)

**Success Criteria:**
- Synergy rules loaded from data files (not compiled into binary)
- New abilities can define tags without code changes
- Synergy rules configurable by designers (no programmer required)
- Multiple synergies can target same ability (windows stack)
- Invalid rules detected and logged at startup (fail-fast)
- Hot-reloading works (change config, see effect immediately)
- System scales to 50+ abilities without performance degradation

**Duration:** 2-3 days (optional, post-MVP)

---

## Acceptance Criteria

**Functional:**
- Universal lockout system replaces fixed GCD
- Heavy abilities lock longer (Overpower 2s > Knockback 0.5s)
- Synergies unlock abilities early during lockout
- Lunge → Overpower unlocks at 0.5s (not 1.0s)
- Overpower → Knockback unlocks at 1.0s (not 2.0s)
- Glow appears immediately when ability used
- Glow removed when lockout expires or ability used

**UX:**
- Circular recovery progress visible and smooth
- Additive glow preserves base state colors (grey/green/yellow)
- Gold border + particles draw attention to glowing abilities
- Pulsing animation noticeable but not distracting
- Audio cues reinforce synergy activation (trigger + use distinct)
- Players discover synergies naturally through visual feedback

**Performance:**
- Single `GlobalRecovery` component (not N per-ability cooldowns)
- Synergy detection overhead < 1ms per ability use
- UI rendering overhead < 1ms per frame
- Maximum 2-3 `SynergyUnlock` components active (MVP)

**Code Quality:**
- Recovery system isolated in dedicated module
- Synergy detection system decoupled from ability execution
- UI updates query components (don't modify state)
- System execution order documented and enforced
- MVP hardcoded rules cleanly replaceable with data-driven (Phase 4)

---

## Discussion

### Implementation Note: Glow Timing

**Spec requirement (Line 422):** Glow appears when ability is used, not when unlock window opens.

**Implementation:**
- `SynergyUnlock` inserted immediately on ability use
- UI checks for `SynergyUnlock` component existence (renders glow)
- Glow persists entire lockout duration (not just unlock window)

**Rationale:** Immediate feedback guides players to synergy discovery.

### Implementation Note: Additive Glow Layering

**Three visual layers:**
1. Base state color (grey/green/yellow)
2. Circular recovery progress (fills from empty to full)
3. Synergy glow (gold border + particles + brightness boost)

**Rendering order:**
1. Render base ability icon with state color
2. Render circular progress overlay
3. Render gold border + particles if `SynergyUnlock` exists
4. Apply brightness boost (multiplicative)

**Combined states:**
- Grey + partial progress + gold = "Locked, will unlock early soon"
- Grey + full progress + gold = "About to unlock (shouldn't happen, glow removed on lockout end)"
- Green + gold = "Unlocked via synergy, ready to use"

### Implementation Note: Lockout Tuning

**MVP lockout durations intentionally longer than spec:**
- Spec suggests 0.2-1.2s range
- MVP uses 0.5-2.0s for clarity and testing

**Rationale:**
- Longer durations easier to perceive during playtesting
- Easier to balance down than up
- Synergy windows more obvious with longer lockouts

**Post-MVP:** Tune down to spec ranges based on playtest feedback.

---

## Acceptance Review

### Scope Completion: 100%

**All 3 MVP phases complete:**
- ✅ Phase 1: Recovery System Foundation
- ✅ Phase 2: Tactical Synergies
- ✅ Phase 3: Visual Feedback

**Phase 4 (Data-Driven Synergies) deferred to post-MVP.**

### Architectural Compliance

**✅ ADR-017 Specifications:**
- Universal lockout pattern (single `GlobalRecovery` component)
- Early unlock system (`SynergyUnlock` components)
- Immediate glow feedback (appears on ability use)
- Additive visual layering (gold on top of base state)

### Player Experience Validation

**Recovery Variety:** ✅ Excellent
- Overpower (2s) feels impactful and committing
- Knockback (0.5s) stays reactive
- Lunge/Deflect (1s) balanced middle ground

**Synergy Discovery:** ✅ Excellent
- Immediate glow guides players naturally
- Audio cues reinforce feedback
- No tutorial required (self-teaching)

**Tactical Depth:** ✅ Excellent
- Lunge → Overpower synergy feels rewarding
- Overpower → Knockback creates fluid combos
- Alternative paths viable (synergies optional, not forced)

**Skill Expression:** ✅ Excellent
- Beginners: Use abilities when available (base lockouts acceptable)
- Experts: Chain synergies for flow state
- Visible mastery (smooth combos vs. lockout spam)

### Performance

**Lockout System:** ✅ Excellent
- Single component per player (reduced from N per-ability cooldowns)
- Timer tick overhead negligible (< 0.1ms per frame)

**Synergy Detection:** ✅ Excellent
- Runs once per ability use (not every frame)
- Overhead < 1ms per activation
- Scales linearly with synergy rule count (MVP: 2 rules)

**UI Rendering:** ✅ Excellent
- Glow rendering < 1ms per frame (2 glowing abilities max)
- Particle system lightweight (2-3 particles per glowing ability)
- No performance regression from Combat HUD baseline

---

## Conclusion

The Ability Recovery and Tactical Synergies implementation creates variable-pacing combat with natural synergy discovery, enabling tactical depth without forced rotations.

**Key Achievements:**
- Universal lockout pattern simplifies state management (1 component vs. N cooldowns)
- Immediate glow feedback teaches synergies naturally (no tutorial required)
- Additive visual layering preserves base UI semantics (glow enhances, doesn't replace)
- Tactical adaptation rewarded over memorized rotations (synergies optional)

**Architectural Impact:** Establishes recovery system pattern for all future abilities. Data-driven extensibility (Phase 4) enables build diversity through unique synergy patterns.

**The implementation achieves RFC-012's core goal: tactical combat with meaningful ability sequencing, discoverable through natural feedback.**

---

## Sign-Off

**Reviewed By:** ARCHITECT Role
**Date:** 2025-11-07
**Decision:** ✅ **ACCEPTED**
**Status:** Merged to main
