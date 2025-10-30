# ADR-004: Player Feedback on Ability System Implementation

## Purpose

This document captures PLAYER role feedback on ADR-004 implementation priorities. The architect has designed the directional targeting system - this document highlights what matters most to player experience.

**Key Principle:** Directional combat lives or dies on **feel**. Technical correctness is necessary but not sufficient - the system must feel responsive, predictable, and fair.

---

## 🔴 CRITICAL: Target Indicator Must Be ROCK SOLID

**Reference:** ADR-004 Lines 1061-1088 (Phase 3: Target Indicator Rendering)

This is **THE MOST IMPORTANT** thing to get right. If players don't trust the indicator, the entire directional combat system fails.

### What Must Work:

1. **Updates every frame** - NO lag, NO stutter
2. **Indicator position matches target exactly** - Not offset, not floating weirdly
3. **Clear visual hierarchy** - Red indicator is OBVIOUS, not subtle
4. **Instant feedback** - When player moves/turns, indicator snaps immediately to new target
5. **Disappears cleanly** - When no targets, don't flicker or leave ghost indicators

### Critical Test Case:

```
Test: Circle Strafe Around Wild Dog
1. Spawn Wild Dog
2. Move in complete circle around it using arrow keys
3. Red indicator should smoothly update as heading changes
4. At NO point should player wonder "wait, who am I targeting?"
5. Indicator should never lag behind actual target
```

### Why This Matters:

Players make split-second decisions based on the indicator. If they see red indicator on Enemy A, press Q, and hit Enemy B instead, they lose trust in the entire system. Trust lost = combat feels broken.

---

## ⚠️ HIGH PRIORITY: Facing Cone (60°) Needs Careful Tuning

**Reference:** ADR-004 Lines 199-223 (Facing Cone Calculation)

60° is **NARROW**. This might feel restrictive or frustrating if not implemented carefully.

### What Could Go Wrong:

- Player presses Q (attack) and nothing happens → Why? Enemy was "just outside" 60° cone
- Player swears they were facing enemy but game disagrees
- Cone edges feel arbitrary or unfair

### What Developer Should Do:

1. **Visual cone indicator** - SHOW the 60° cone, at least in tutorial/testing
   - Optional overlay mentioned in ADR-004 line 42
   - Players need to learn what "60° facing cone" means through play

2. **Generous edge handling** - If target is at 29.9° from heading, treat as inside
   - Don't be pixel-perfect strict on boundaries
   - Small tolerance prevents frustration

3. **Playtest with real arrow key movement** - Context-sensitive diagonal movement (from combat-system.md) might make aiming the cone harder than expected

### Critical Test Case:

```
Test: Facing Cone Boundaries
1. Spawn 3 Wild Dogs in front of player
   - Dog A: Directly in front (0° from heading)
   - Dog B: 30° from heading (edge of cone)
   - Dog C: 60° from heading (outside cone)
2. Which ones can player target?
3. Does it FEEL fair when Dog C isn't targetable?
4. Can player quickly adjust facing to target Dog C?
```

### Why This Matters:

If players constantly miss attacks because enemies are "just outside" the cone, they'll blame the controls, not their positioning. The 60° cone needs to feel generous enough that skilled positioning is rewarded, but not so strict that every attack is a gamble.

---

## ⚠️ HIGH PRIORITY: Geometric Tiebreaker Must Feel Predictable

**Reference:** ADR-004 Lines 294-320 (Geometric Tiebreaker Logic)

When two enemies are equidistant, "closest to exact heading angle" picks one. This MUST feel predictable.

### What Could Go Wrong:

- Player wants to hit Enemy A but keeps hitting Enemy B
- Can't tell why one is being selected over the other
- Feels random or unfair
- Player repositions to switch targets but indicator doesn't move as expected

### What Developer Should Do:

1. **Visual feedback on selection reasoning**
   - Maybe indicate angle to target (subtle visual cue)
   - Brighter indicator on selected target if multiple are close

2. **Consistent behavior** - Same positions always = same target
   - Deterministic tiebreaker (ADR-004 line 324: "same inputs → same target")
   - No frame-to-frame flicker between equidistant targets

3. **Player education** - Tutorial explains "face enemies directly to target them reliably"
   - Positioning matters by design (from combat-system.md)
   - This is a feature, not a bug

### Critical Test Case:

```
Test: Equidistant Target Selection
1. Spawn 2 Dogs equidistant from player (both range 2)
   - Dog A at 20° from heading
   - Dog B at 40° from heading
   - Both within 60° cone, both same distance
2. Face exactly between them (30° heading)
3. Does one consistently get targeted?
4. Can player predict which one?
5. If player adjusts heading slightly (28° instead of 30°), does target switch predictably?
```

### Why This Matters:

Geometric tiebreaker creates tactical depth (positioning off-center matters). But if players can't predict or understand the selection, it feels arbitrary. The ranged player flanking scenario from combat-system.md discussion relies on this working intuitively.

---

## ⚠️ MEDIUM PRIORITY: Server/Client Target Mismatch

**Reference:** ADR-004 Lines 699-720 (Server Target Validation)

The ADR says server recalculates target and client's choice might not match. If this happens frequently, it's **infuriating**.

### What Could Go Wrong:

- Player sees red indicator on Dog, presses Q, gets "Invalid target" error
- Enemy moved between key press and server validation (latency)
- Feels like the game is broken or laggy

### What Developer Should Do:

1. **Tolerance** - Server should accept client's target if "close enough"
   - Within 60° cone AND same distance tier = accept
   - Small latency window (100-200ms) shouldn't cause failures

2. **Visual prediction** - If server might reject, show indicator as "tentative"
   - Pulsing indicator = uncertain target
   - Solid indicator = confident target

3. **Error messages that make sense**
   - ❌ "Invalid target" (unhelpful)
   - ✅ "Target moved!" or "Out of range!" (explains what happened)

### Critical Decision Point:

**ADR-004 Line 1243 says:** "MVP: Strict match (must be same entity), relax if issues arise"

**PLAYER FEEDBACK:**
- ❌ **DO NOT do strict match** - This will cause constant frustration with latency
- ✅ **Start with tolerance** - Accept client target if it's:
  - Within 60° cone (revalidate on server)
  - Same distance tier (close/mid/far)
  - Entity still exists and is valid target

### Critical Test Case:

```
Test: Latency Tolerance
1. Add artificial 150ms latency
2. Spawn moving Wild Dog (walking in circle)
3. Player faces Dog, presses Q
4. Does attack succeed or fail with "Invalid target"?
5. If fails frequently, players will think combat is broken
```

### Why This Matters:

Real-world latency (50-200ms) means entity positions are always slightly out of sync. Strict matching punishes players for network conditions outside their control. Generous server validation makes combat feel responsive even on mediocre connections.

---

## ⚠️ MEDIUM PRIORITY: Q and E Keys Must Feel INSTANT

**Reference:** ADR-004 Lines 1092-1119 (Ability Input Handling)

When player presses Q to attack, it must execute **immediately** (< 16ms). Any delay kills the directional combat feel.

### What Could Go Wrong:

- Input lag between key press and ability execution
- Buffering issues (multiple Q presses = multiple attacks queued)
- Key press missed because system didn't poll fast enough
- Animation doesn't play instantly

### What Developer Should Do:

1. **Process input FIRST** in update loop
   - Input handling → Prediction → Rendering → Network
   - Don't wait for network before showing feedback

2. **Client prediction (Phase 5) must be immediate**
   - Stamina drops on client before server responds
   - Attack animation plays instantly
   - Queue clears visually before server confirms

3. **Visual feedback locks in commitment**
   - Ability animation plays = "I pressed Q, game received it"
   - Even if server denies later, player saw instant response

### Critical Test Case:

```
Test: Input Responsiveness
1. Rapid Q presses (spam attack button)
2. Should attack once per GCD (0.5s from ADR-002)
3. Should NOT queue up 10 attacks
4. Each press should give instant visual feedback (animation start)
5. Measure frame delay: Key down → Animation start (target: < 16ms)
```

### Why This Matters:

Players judge responsiveness within 100ms. If Q press → attack animation takes 200ms, combat feels sluggish even if technically correct. The "no twitch mechanics" design from combat-system.md requires instant feedback to work - players need to react to threats, not fight input lag.

---

## ✅ GOOD: MVP Scope is Correct

**Reference:** ADR-004 Lines 853-876 (MVP Scope)

The ADR correctly defers tier lock (1/2/3) and TAB cycling to Phase 2. This is **smart**.

### Why This is Good:

- MVP validates the core: heading → target selection → attack
- Don't overcomplicate before proving automatic targeting works
- Wild Dog combat is simple enough that tier lock isn't needed yet
- Allows iteration on fundamentals before adding complexity

### What Developer Should NOT Do:

- ❌ Try to implement tier lock "just in case" - Resist scope creep!
- ❌ Add fancy visual polish before core targeting works
- ❌ Build Phase 2 features "while I'm in there" - Stay focused on MVP

### Why This Matters:

If automatic targeting doesn't feel good, tier lock won't save it. Get the foundation solid first. The warrior-vs-ranged-player scenario from combat-system.md discussion can wait until Phase 2.

---

## 📊 Testing Priority for Developer

### Priority 1: Breaks Everything If Wrong

1. **Target indicator appears and updates every frame**
   - Phase 3 deliverable
   - Without this, directional combat is unplayable

2. **Pressing Q attacks the indicated target**
   - Phase 4 deliverable
   - Core ability execution flow

3. **Facing cone (60°) includes targets player expects**
   - Phase 1-2 deliverable
   - Cone calculation must be correct

### Priority 2: Annoying If Wrong

4. **Geometric tiebreaker picks the "right" enemy**
   - Phase 2 deliverable
   - Equidistant scenarios feel predictable

5. **Server doesn't reject client's target frequently**
   - Phase 4 deliverable
   - Latency tolerance prevents frustration

6. **Arrow key movement updates heading correctly**
   - Already exists in controlled.rs
   - Integration test needed

### Priority 3: Polish (Can Iterate)

7. **Visual clarity of indicator** (color, size, brightness)
8. **Error messages make sense** ("Target moved!" not "Invalid")
9. **Performance** (60fps maintained with indicator updates)

---

## 🎯 Developer: Remember This One Thing

**Reference:** ADR-004 Lines 203-213 (`is_in_facing_cone` function)

This function runs **a lot**:
- Every frame for indicator update
- Every ability use for validation
- Every AI tick for enemy targeting

If it's buggy or slow, everything breaks.

### Developer Should:

✅ **Write comprehensive unit tests** (ADR-004 lines 1014-1025 cover this)
- Test all 6 headings (NE, E, SE, SW, W, NW)
- Test edge cases: exactly 30° (in cone), 30.1° (out of cone)
- Test boundary: -30° to +30° from heading

✅ **Test wrap-around angles**
- Heading::W (270°), target at 5° → wraps around 360°, should be in cone
- Heading::NE (30°), target at 350° → wraps around 0°, should be in cone

✅ **Performance test**
- 100 entities, select_target runs every frame
- Target: < 1ms total CPU time (ADR-004 line 1200)

### Critical Test Cases from ADR-004:

```rust
#[test]
fn facing_cone_east_target_at_80_degrees() {
    // Heading::E = 90°, target at 80° → delta = 10° → IN cone (< 30°)
    assert!(is_in_facing_cone(Heading::E, caster_loc, target_loc));
}

#[test]
fn facing_cone_east_target_at_150_degrees() {
    // Heading::E = 90°, target at 150° → delta = 60° → OUT of cone (> 30°)
    assert!(!is_in_facing_cone(Heading::E, caster_loc, target_loc));
}

#[test]
fn facing_cone_wrap_around() {
    // Heading::NW = 330°, target at 350° → delta = 20° → IN cone
    assert!(is_in_facing_cone(Heading::NW, caster_loc, target_loc));
}
```

---

## Final PLAYER Take

The ADR-004 architecture is **solid**. The directional targeting design makes sense. But directional combat lives or dies on **feel**, and feel comes from:

1. **Instant, obvious target indicator** ← Most important
2. **Predictable targeting** ← Second most important
3. **Responsive input** ← Third most important

### Success Criteria:

**Players should feel:**
- "I know who I'm going to hit before I press Q" (indicator trust)
- "Positioning matters but feels fair" (geometric tiebreaker)
- "Combat is responsive" (instant feedback)

**Players should NOT feel:**
- "Wait, why didn't I hit that enemy?" (cone confusion)
- "The targeting is broken" (server/client mismatch)
- "Combat feels laggy" (input delay)

### If Those Three Things Work:

✅ Directional combat will feel good
✅ Players will engage with positioning tactics
✅ The "no cursor required" design will be praised

### If Any One Fails:

❌ Players will hate it and say "the targeting is broken"
❌ Combat will feel unfair or unresponsive
❌ All the architectural elegance won't matter

---

## Recommendation to Developer

**Start with Phase 3 (Target Indicator) and get it PERFECT before moving on.**

Everything else depends on players trusting what the indicator shows. Spend extra time making it:
- Responsive (every frame update)
- Clear (obvious visual)
- Accurate (matches actual selection)

If indicator feels good, players will forgive other rough edges. If indicator feels bad, nothing else matters.

**From PLAYER role perspective:** I'd rather have a rock-solid indicator with basic visuals than a fancy indicator that lags or flickers.

---

## Date

2025-10-30 (Player feedback on ADR-004 directional targeting system)
