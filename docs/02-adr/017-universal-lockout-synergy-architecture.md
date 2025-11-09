# ADR-017: Universal Lockout + Early Unlock Architecture

## Status

**Accepted** - 2025-11-07

## Context

**Related RFC:** [RFC-012: Ability Recovery and Tactical Synergies](../01-rfc/012-ability-recovery-and-synergies.md)

Fixed 0.5s GCD creates uniform pacing regardless of ability commitment. Heavy strikes (Overpower) recover as fast as quick reactions (Knockback), removing tactical weight. No reward for ability sequencing - all orders feel identical.

### Requirements

- Variable recovery durations reflect ability commitment (heavy vs. quick)
- Reward tactical sequencing without forcing memorized rotations
- Fluid combos through reduced recovery on smart follow-ups
- Self-teaching system (discover synergies naturally)
- Maintain "conscious but decisive" philosophy (no button mashing)

### Options Considered

**Option 1: Universal Lockout + Early Unlock** ✅ **SELECTED**
- Using ability locks ALL abilities for recovery duration
- Synergies unlock specific abilities early during lockout
- Immediate glow guides players to synergies

**Option 2: Per-Ability Cooldowns**
- Each ability has independent cooldown
- ❌ Encourages button mashing, no pacing rhythm

**Option 3: Fixed GCD + Cooldown Reduction**
- Keep 0.5s GCD, synergies reduce cooldowns
- ❌ GCD still creates uniform pacing, doesn't create fluid combos

**Option 4: Rotation System (Memorized Combos)**
- Fixed sequences grant bonuses
- ❌ Forces memorization, punishes experimentation

## Decision

**Use universal lockout pattern: single `GlobalRecovery` component locks all abilities for variable duration. Synergies allow early unlock via `SynergyUnlock` components. Glow appears immediately to guide tactical choices.**

### Core Mechanism

**Universal Lockout Pattern:**

```rust
#[derive(Component)]
pub struct GlobalRecovery {
    pub remaining: f32,          // Seconds until ALL abilities unlock
    pub duration: f32,           // Total lockout duration
    pub triggered_by: AbilityKey, // Which ability triggered lockout
}
```

**Flow:**
1. Player uses Overpower → Insert `GlobalRecovery { remaining: 2.0, duration: 2.0, triggered_by: Overpower }`
2. ALL abilities locked (check for `GlobalRecovery` component)
3. Timer decrements each frame (`remaining -= dt`)
4. Lockout expires → Remove `GlobalRecovery` → All abilities available

**Early Unlock via Synergies:**

```rust
#[derive(Component)]
pub struct SynergyUnlock {
    pub ability_key: AbilityKey,  // Which ability can unlock early
    pub unlock_at: f32,           // Lockout time when available
    pub triggered_by: AbilityKey, // Which ability triggered synergy
}
```

**Synergy Flow:**
1. Player uses Lunge → Insert `GlobalRecovery { remaining: 1.0, duration: 1.0, triggered_by: Lunge }`
2. Synergy detection runs (same frame) → Insert `SynergyUnlock { ability_key: Overpower, unlock_at: 0.5, triggered_by: Lunge }`
3. Overpower glows immediately (gold border + particles)
4. At t=0.5s: `recovery.remaining <= synergy.unlock_at` → Overpower usable early
5. At t=1.0s: Lockout expires → Remove `GlobalRecovery` and `SynergyUnlock`

**MVP Recovery Durations:**
- Lunge (Gap Closer): 1.0s lockout
- Overpower (Heavy Strike): 2.0s lockout
- Knockback (Push): 0.5s lockout
- Deflect (Defensive): 1.0s lockout

**MVP Synergies:**
- Lunge → Overpower unlocks 0.5s early (available at 0.5s instead of 1.0s)
- Overpower → Knockback unlocks 1.0s early (available at 1.0s instead of 2.0s)

---

## Rationale

### 1. Universal Lockout Creates Commitment-Based Pacing

**Before (Fixed GCD):**
- All abilities recover in 0.5s
- No tactical weight (Overpower feels like Knockback)
- Button mashing viable strategy

**After (Universal Lockout):**
- Heavy abilities lock you out longer (Overpower 2s)
- Quick reactions stay responsive (Knockback 0.5s)
- Commitment creates pacing rhythm

**Impact:** Abilities feel distinct through recovery duration, not just damage/cost.

### 2. Early Unlock Rewards Sequencing Without Forcing Rotations

**Flexible Synergies:**
- Lunge → Overpower: Close gap, capitalize with heavy strike
- Overpower → Knockback: Heavy hit destabilizes, push to create space
- Both sequences tactical, not memorized rotation

**Alternative Paths:**
- Can use Lunge without following with Overpower (no penalty)
- Can use Overpower first (no synergy, but still valid)
- Synergies are rewards, not requirements

**Impact:** Tactical adaptation valued over memorized combos.

### 3. Immediate Glow Guides Natural Discovery

**Glow Timing:**
```
t=0.0s: Use Lunge → Overpower glows immediately (not at t=0.5s)
t=0.5s: Overpower unlocked early (can use)
t=1.0s: Lockout expires, glow removed
```

**Why immediate glow:**
- Appears when ability used (cause-and-effect clear)
- Persists entire lockout (always visible)
- Delayed glow easy to miss during combat

**Impact:** Players discover synergies naturally through visual feedback.

### 4. Additive Glow Preserves Base UI State

**Visual Layering:**
- Base layer: Green (available), Yellow (out of range), Grey (locked)
- Glow layer: Gold border + particles + brightness boost
- Combined: Grey + Gold = "Locked but will unlock early"

**Why additive:**
- Base state still communicates availability/range
- Glow communicates synergy (separate concern)
- Three states visible simultaneously (locked + synergy + range)

**Impact:** UI complexity managed through layering, not state replacement.

### 5. Single Component Simplifies State Management

**Component Count:**
- Before: N per-ability cooldowns (Lunge.cooldown, Overpower.cooldown, etc.)
- After: 1 `GlobalRecovery` + M `SynergyUnlock` (M = active synergies)
- MVP: Maximum 2 synergies active simultaneously

**State Queries:**
- Before: Check N cooldown components
- After: Check 1 `GlobalRecovery` + lookup synergy

**Impact:** Fewer components, simpler queries, clearer semantics.

---

## Consequences

### Positive

**1. Variable Commitment Pacing**
- Heavy abilities (2s lockout) feel impactful
- Quick reactions (0.5s) stay responsive
- Pacing rhythm emerges naturally

**2. Tactical Depth Without Memorization**
- Synergies reward smart sequencing
- No forced rotations (alternative paths viable)
- Experimentation encouraged

**3. Self-Teaching System**
- Immediate glow guides players
- Audio cues reinforce feedback
- No tutorial required

**4. Build Diversity Foundation**
- Future: Weapons/armor define unique synergy patterns
- Future: Attributes scale lockout durations
- Extensible to data-driven rules

**5. Skill Expression**
- Beginner: Base lockouts (acceptable)
- Expert: Chained synergies (flow state)
- Visible mastery

### Negative

**1. UI Complexity (Additive Glow)**
- Three visual layers (base state + glow + range)
- Requires careful opacity/brightness tuning
- Playtest iteration needed

**2. Balancing Challenge**
- Lockout durations require iteration
- Synergy unlock times must feel right
- MVP uses longer durations for clarity (tune down later)

**3. State Tracking Complexity**
- Universal lockout + per-ability synergy unlocks
- Multiple `SynergyUnlock` components per player
- Cleanup required when lockout expires

**4. Animation Timing Dependencies**
- Lockout must feel natural with ability animations
- Overlap acceptable (lockout starts on use, not animation end)
- Requires coordination with animation system

### Neutral

**1. Replaces Fixed GCD**
- Commitment to variable recovery system
- No backward compatibility with old GCD

**2. Data-Driven Future**
- MVP uses hardcoded synergy rules
- Post-MVP: RON/JSON configuration
- Extensibility designed in

**3. Performance**
- Synergy detection runs per ability use (not every frame)
- Overhead: negligible for 4-12 abilities

---

## Implementation Notes

**System Execution Order:**
```
1. ability_execution_system → Trigger ability effects
2. trigger_recovery_lockout → Insert GlobalRecovery
3. detect_synergies_system → Insert SynergyUnlock, spawn audio
4. global_recovery_system → Decrement timer (every frame)
5. synergy_cleanup_system → Remove SynergyUnlock when lockout expires
6. update_recovery_ui → Render progress + glow
```

**Critical Dependencies:**
- Synergy detection MUST run AFTER lockout insertion (needs `GlobalRecovery`)
- UI MUST read latest state (run after recovery/synergy systems)
- Glow MUST NOT override base state colors (additive only)

**Integration Points:**
- Ability execution: Insert `GlobalRecovery` on use
- Combat HUD: Action bar shows recovery progress + glow
- Audio: Synergy trigger + use sounds

---

## Validation Criteria

**Functional:**
- Heavy abilities lock longer (Overpower 2s > Knockback 0.5s)
- Synergies unlock early (Lunge → Overpower at 0.5s)
- Glow appears immediately when ability used
- Additive glow preserves base state colors

**UX:**
- Players discover synergies naturally (glow + audio)
- Combat feels responsive (no forced rotations)
- Skill ceiling visible (expert chaining vs. beginner spam)

**Performance:**
- Single `GlobalRecovery` component (not N cooldowns)
- Synergy detection < 1ms per ability use
- UI rendering < 1ms per frame

---

## References

- **RFC-012:** Ability Recovery and Tactical Synergies
- **ADR-008:** Combat HUD (action bar integration point)
- **ADR-009:** MVP Ability Set (abilities receiving recovery timers)
- **ADR-003:** Reaction Queue System (circular timer UI pattern)

## Date

2025-11-07
