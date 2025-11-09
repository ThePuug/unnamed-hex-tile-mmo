# RFC-004: Ability System and Directional Targeting

## Status

**Implemented** - 2025-10-30

## Feature Request

### Player Need

From the combat system spec: **"Directional combat"** - Face your enemies, position matters, no cursor required.

**Current Problem:**
Without directional targeting and abilities:
- No way to attack enemies (movement only)
- No tactical positioning (facing doesn't matter)
- No ability usage (Basic Attack, Dodge unavailable)
- Combat system incomplete (reaction queue exists but no threat sources)

**We need a system that:**
- Targets enemies based on facing direction (no cursor/clicking)
- Selects nearest hostile in 60° facing cone automatically
- Executes abilities with keyboard only (Q/E keys)
- Works for both player (BasicAttack) and AI (Wild Dog attacks)
- Integrates with existing Heading component (6 directions)

### Desired Experience

Players should experience:
- **Keyboard-only combat:** Arrow keys move and face, Q/E cast abilities
- **Automatic targeting:** Red indicator shows which enemy will be hit
- **Directional gameplay:** Positioning matters, face enemies to attack
- **Instant feedback:** Abilities execute immediately (client prediction)
- **Controller-friendly:** No mouse required, gamepad-ready

### Specification Requirements

From `docs/00-spec/combat-system.md`:

**1. Heading System:**
- Movement updates heading (6 directions: NE, E, SE, SW, W, NW)
- Heading persists after movement stops
- Facing cone: 60° (one hex-face direction)

**2. Automatic Target Selection:**
- Default: Nearest hostile in facing direction
- Geometric tiebreaker: Closest to exact facing angle
- No clicking required

**3. Target Indicators:**
- Red indicator: Current hostile target
- Updates smoothly as player moves/turns

**4. MVP Abilities:**
- **BasicAttack** (Q key): Instant, adjacent hex, hits indicated target
- **Dodge** (E key): Self-target, clears queue, no targeting

**5. Enemy Abilities:**
- Wild Dog: Basic melee attack (auto-targets nearest player)

### MVP Scope

**Phase 1 includes:**
- Heading → 60° facing cone conversion
- Automatic target selection (nearest in facing direction)
- Red hostile indicator (basic visual)
- BasicAttack ability (player and Wild Dog)
- Dodge ability (clears reaction queue)
- Keyboard controls (Q/E for abilities)

**Phase 1 excludes:**
- Tier lock system (1/2/3 keys) - defer to Phase 2
- TAB cycling - defer to Phase 2
- Green ally indicator - defer to Phase 2
- Complex patterns (Line, Radius, Adjacent)
- Projectiles (instant abilities only)

### Priority Justification

**CRITICAL** - This enables combat gameplay. Without abilities, players can't attack enemies or defend themselves. Reaction queue (ADR-003) is incomplete without threat sources (BasicAttack).

**Blocks:**
- Combat testing (can't fight Wild Dogs)
- Damage pipeline (ADR-005)
- AI behavior (ADR-006)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Heading-Based Directional Targeting with Automatic Selection**

#### Core Mechanism

**Heading to Facing Cone:**
- Existing `Heading` component has 6 directions (NE, E, SE, SW, W, NW)
- Convert heading to angle: Heading::E → 90°
- Facing cone extends ±30° from heading angle (60° total)
- Angular check determines if target in cone

**Target Selection Algorithm:**
1. Query all hostiles within max range (NNTree spatial index)
2. Filter to entities within 60° facing cone
3. Select nearest by hex distance
4. Geometric tiebreaker: Closest to exact heading angle

**Benefits:**
- Reuses existing Heading component (no new infrastructure)
- Deterministic (same inputs → same target)
- Client and server use identical logic
- Fast (NNTree query + simple filtering)

#### Performance Projections

**Targeting Performance:**
- NNTree query: < 0.5ms for 100 entities
- Angular filtering: < 0.1ms (dot product)
- Indicator update: Every frame for local player (~16ms budget)
- Total: < 1ms per target selection

**Network Bandwidth:**
- Ability use: ~50 bytes per event
- 10 abilities/sec (combat): ~500 bytes/sec
- Prediction reduces latency (no wait for server)

**Scaling:**
- 100 players in combat: 10-20 abilities/sec total
- 1,000-2,000 bytes/sec sustained
- CPU: < 5% for targeting + ability execution

#### Technical Risks

**1. Heading Discretization**
- *Risk:* Only 6 directions may feel limiting
- *Mitigation:* Hex grid naturally discrete, tiebreaker handles boundaries
- *Tolerance:* Geometric tiebreaker provides sub-60° precision

**2. Target Indicator Update Frequency**
- *Risk:* Every frame update may lag
- *Mitigation:* NNTree fast, only local player updates
- *Optimization:* Dirty flag if profiling shows issue

**3. Client-Server Target Mismatch**
- *Risk:* Latency causes client/server to select different targets
- *Impact:* Ability fails with "Invalid target" error
- *Mitigation:* Server accepts client's target if "close enough"
- *Frequency:* Rare for stationary targets (Wild Dog)

**4. Prediction Rollback**
- *Risk:* Client predicts ability, server denies
- *Impact:* Stamina/queue state rolls back (jarring)
- *Mitigation:* Local validation before prediction, show predicted state visually

### System Integration

**Affected Systems:**
- Movement (heading already updated by controlled.rs)
- Reaction queue (Dodge clears queue, BasicAttack inserts threats)
- Resource management (Dodge consumes stamina)
- AI behavior (Wild Dog uses targeting for attacks)
- Network protocol (UseAbility, AbilityUsed, AbilityFailed events)

**Compatibility:**
- ✅ Existing Heading component (NE, E, SE, SW, W, NW)
- ✅ Resource management (stamina/mana costs)
- ✅ Reaction queue (ClearQueue effect)
- ✅ GCD system (ability cooldowns)

### Alternatives Considered

#### Alternative 1: Click Targeting (Mouse-Based)

Click on enemies to target, press ability key to execute.

**Rejected because:**
- Requires cursor (violates "no cursor required" spec)
- Slower combat flow (extra input step)
- Not controller-friendly
- Doesn't emphasize positioning/facing

#### Alternative 2: Tab Targeting (MMO-Style)

TAB to cycle through enemies, press ability key.

**Rejected because:**
- No directional element (facing doesn't matter)
- Extra input burden (TAB before every target change)
- Violates "directional combat" design philosophy
- Deferred to Phase 2 as manual override option

#### Alternative 3: Continuous 360° Heading

Heading as continuous angle (not discrete 6 directions).

**Rejected because:**
- Hex grid naturally discrete (6 edges)
- More complexity (smooth rotation, turn-in-place)
- Doesn't match existing movement system
- Diminishing returns (geometric tiebreaker provides precision)

#### Alternative 4: Fixed Targeting (No Auto-Select)

Require tier lock or TAB to select every target.

**Rejected because:**
- High input burden (manual targeting every attack)
- Poor UX (90% of attacks use default nearest target)
- Doesn't match "automatic targeting" spec requirement

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Reusing existing Heading component eliminates need for new infrastructure.

**Pattern Recognition:**
- Similar to collision detection (spatial query + filtering)
- Deterministic shared logic (client/server use identical function)
- Client prediction pattern (predict ability, server confirms)

**Extensibility:**
Foundation enables future features:
- Tier lock system (filter by range tier)
- TAB cycling (manual target override)
- Complex patterns (Line, Radius, Adjacent)
- Projectile system (traveling abilities)
- Flanking bonuses (target's heading matters)

### PLAYER Validation

**UX Requirements:**
- ✅ Face enemies with arrow keys
- ✅ Red indicator shows target clearly
- ✅ Press Q to attack (instant feedback)
- ✅ Press E to dodge (instant queue clear)
- ✅ No cursor required

**Combat Feel:**
- Directional: Positioning matters (face enemies)
- Responsive: Abilities execute in < 16ms (predicted)
- Clear: Indicator shows which enemy will be hit
- Simple: Two abilities (Q/E), automatic targeting

**Acceptance Criteria:**
- ✅ Player can attack Wild Dog by facing and pressing Q
- ✅ Red indicator updates smoothly as player moves
- ✅ Dodge clears queue instantly when pressed
- ✅ Wild Dog attacks player using same targeting system
- ✅ Abilities fail with clear error if no valid target

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- PLAYER: ✅ Enables directional combat gameplay
- ARCHITECT: ✅ Reuses existing systems, scalable, extensible

**Scope Constraint:** Fits in one SOW (estimated 11-13 days across 6 phases)

**Dependencies:**
- ADR-002/003/004/005: Combat Foundation (resources, queue, GCD)
- Existing: Heading component, NNTree, movement systems

**Next Steps:**
1. ARCHITECT creates ADR-009 documenting targeting and ability execution decision
2. ARCHITECT creates SOW-004 with 6-phase implementation plan
3. DEVELOPER begins Phase 1 (heading to facing cone conversion)

**Date:** 2025-10-30
