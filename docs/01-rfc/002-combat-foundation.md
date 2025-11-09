# RFC-002: Combat Foundation - Resources and State Management

## Status

**Implemented** - 2025-10-29

## Feature Request

### Player Need

We need a functioning combat system to enable the core gameplay loop defined in the combat system spec. Currently, there is no infrastructure for:
- Health, stamina, and mana resource management
- Combat state tracking (in/out of combat)
- Death and respawn mechanics
- Resource regeneration
- Client-server synchronization of combat state

**Without combat foundation, we cannot implement:**
- Reaction queue system (ADR-003)
- Ability system and targeting (ADR-004)
- Damage pipeline (ADR-005)
- AI behavior with abilities (ADR-006)

### Current System State

**We have:**
- Complete attribute system (`ActorAttributes`) with 3-axis pairs
- Derived stat methods: `max_health()`, `movement_speed()`
- Triumvirate classification (Origin/Approach/Resilience)
- ECS infrastructure with Do/Try event pattern
- Client-server sync patterns (position, heading, chunks)

**We lack:**
- Resource pools (health, stamina, mana components)
- Combat state tracking
- Resource regeneration systems
- Death/respawn flow
- UI integration for resources

### MVP Requirements

**Phase 1 Combat Scope** (from combat system spec):
- Wild Dog enemy with basic melee attack
- Player Basic Attack (instant, adjacent hex)
- Player Dodge ability (clears reaction queue)
- Reaction queue with timer visualization
- Combat state management (enter/exit combat)
- Health and stamina resources

**Success Criteria:**
- Player can engage Wild Dog
- Dog's attacks enter reaction queue with visible timer
- Player can Dodge to clear queue (stamina cost)
- Damage applies with armor reduction if no reaction
- Combat feels responsive and clear

### Desired Experience

Players should experience:
- **Immediate feedback:** Resource bars update instantly on local actions
- **Clear state:** Obvious when in/out of combat (UI changes, music)
- **Fair combat:** Server-authoritative to prevent cheating
- **Smooth visuals:** Predicted local changes, no jitter on remote players
- **Meaningful death:** Clear death state, reasonable respawn flow

### Priority Justification

**CRITICAL** - Blocks all combat-related ADRs (003-009). Without resource management foundation, we cannot implement any combat mechanics. This is the first step in the combat system implementation chain.

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Component-Based Resource Management with Server Authority**

#### Core Concept

Implement resource management using ECS components with state/step prediction pattern:
- Separate components for Health, Stamina, Mana, CombatState
- Server-authoritative values with client prediction for local player
- Shared calculation functions in `common/` for both client and server
- Regeneration runs in FixedUpdate (125ms ticks) deterministically
- Combat state transitions based on spatial queries and timers

#### Reuses Existing Patterns

1. **State/Step Prediction:** Already used in `Offset` and `AirTime` components
2. **Shared Calculations:** Physics/behavior already in `common/`
3. **FixedUpdate Schedule:** Physics runs in FixedUpdate, regeneration joins it
4. **Do/Try Events:** Death and resource updates follow established pattern
5. **Time Tracking:** Uses `Duration` from `Time::elapsed()` (not `Instant`)

#### Resource Scaling from Attributes

Leverage existing `ActorAttributes` for derived stats:
```
stamina_pool = 100 + (might * 0.5) + (vitality * 0.3)
mana_pool = 100 + (focus * 0.5) + (presence * 0.3)
max_health = ActorAttributes::max_health()  // Already implemented
armor = base_armor + (vitality / 200.0)  // Max 75% cap
resistance = base_resistance + (focus / 200.0)  // Max 75% cap
```

**Benefits:**
- Attributes investment directly powers combat effectiveness
- No separate stat system needed
- Formulas cleanly testable in isolation

#### Performance Projections

**Regeneration System:**
- Runs in FixedUpdate (125ms ticks) = 8 ticks/second
- 100 players regenerating stamina + mana
- Calculation per entity: ~10 CPU cycles (cheap math)
- Network traffic: **0 bytes/sec** (deterministic sync, no broadcasts needed)
- CPU impact: < 1% single core

**Combat State Management:**
- NNTree spatial query (20 hex radius) per entity
- 100 entities in combat checking for hostiles
- Query cost: O(log n) + filter = ~50μs per entity
- Total: 5ms per tick (8 ticks/sec) = 40ms/sec = 4% single core
- Network: Only broadcasts on state change (~1-2 events per combat encounter)

**Overall Performance:** Negligible impact, scales to 1,000 concurrent players easily.

#### Technical Risks

1. **Client-Server Desync on Regeneration**
   - *Risk:* Client and server regenerate at slightly different rates due to tick timing
   - *Mitigation:* Both use FixedUpdate with shared formula, tolerance of ±1%
   - *Fallback:* Periodic reconciliation (server sends corrected value every 5 seconds)

2. **Prediction Rollback UX**
   - *Risk:* Player predicts ability use, server rejects (low stamina), jarring snap
   - *Mitigation:* Show "Not enough stamina" client-side before allowing action
   - *Acceptance:* Some rollbacks acceptable in high-latency scenarios

3. **Death State Race Conditions**
   - *Risk:* Entity dies while queued ability is processing
   - *Mitigation:* Death system runs AFTER damage application in schedule
   - *Testing:* Validate mutual destruction scenario (both entities die)

4. **Stale Timestamps on Reconnect**
   - *Risk:* Player disconnects, reconnects hours later, `last_update` causes burst regen
   - *Mitigation:* Server resets `last_update` to current `Time::elapsed()` on reconnect
   - *Validation:* Reconnect integration test

### System Integration

**Affected Systems:**
- Server spawn logic (initialize resources on player/NPC spawn)
- Client renet (receive resource updates)
- UI system (health/stamina/mana bars)
- Future damage system (apply damage, check death)
- Future ability system (stamina/mana costs)

**Compatibility:**
- ✅ ECS architecture (components fit naturally)
- ✅ Existing event system (Health/Stamina/Mana events)
- ✅ Client prediction pattern (state/step reused)
- ✅ Attribute system (derives resource maxes)
- ⚠️ Network protocol (new event types, non-breaking addition)

### Alternatives Considered

#### Alternative 1: Single "Resources" Component

Store all resources in one component:
```rust
struct Resources {
    health: f32,
    stamina: f32,
    mana: f32,
    // ...
}
```

**Rejected because:**
- Not all entities need all resources (decorators have no stamina/mana)
- Forces over-fetching in queries (need stamina, get health/mana too)
- Breaks ECS composition principle
- Harder to extend (adding new resource type requires component changes)

#### Alternative 2: Store Armor/Resistance as Components

Pre-calculate and store armor/resistance values:
```rust
struct CombatStats {
    armor: f32,
    resistance: f32,
}
```

**Rejected because:**
- Duplicates state (armor derives from attributes)
- Requires sync on every attribute change (more network traffic)
- Risk of desync if calculation logic changes
- Calculation is cheap (division + addition), no performance gain

#### Alternative 3: Regeneration via Network Updates

Server broadcasts regeneration ticks to clients:
```rust
// Every FixedUpdate tick
Event::Stamina { ent, current: 75.2 }
```

**Rejected because:**
- **Massive network spam:** 100 players × 2 resources × 8 ticks/sec = 1,600 messages/sec
- Defeats purpose of deterministic systems
- Doesn't scale
- Wastes server/client bandwidth

**Chosen Approach:** Deterministic regeneration (both run same system, no sync needed)

---

## Discussion

### ARCHITECT Notes

This RFC leverages existing architectural patterns exceptionally well:
- State/step prediction (proven in movement)
- Shared calculations in `common/` (proven in physics)
- FixedUpdate scheduling (proven in physics)
- Do/Try event pattern (proven in terrain discovery)

**Key Architectural Insight:** Resources are deterministic state that can be simulated identically on client and server. This eliminates the need for continuous synchronization (regeneration traffic = 0).

**Critical Invariants to Maintain:**
1. Regeneration formula identical on client and server (shared code)
2. Death system runs AFTER damage system (schedule order)
3. Time tracking uses `Duration` from `Time::elapsed()` (not `Instant`)
4. Server has final authority on all resource values

**Extensibility:**
This foundation enables:
- Temporary buffs (modify regen rate, max values)
- Gear system (attributes change → resources scale)
- Status effects (modify combat state, regen)
- PvP combat (server authority prevents cheating)

### PLAYER Validation

From player perspective, this should feel like any standard MMO combat:
- Health bars deplete when hit
- Stamina drains when using abilities, regenerates quickly
- Mana pools for magic users
- Clear "in combat" state (can't mount, vendors unavailable)

**UX Requirements:**
- ✅ Instant feedback on local actions (predicted)
- ✅ Smooth health bars on remote players (interpolated)
- ✅ Clear death state (no confusion about "am I dead?")
- ✅ Reasonable respawn flow (5 seconds, spawn at hub)

**Acceptance Criteria:**
- ✅ Combat feels responsive (< 16ms local feedback)
- ✅ No jittery resource bars (smooth interpolation)
- ✅ Death is obvious and immediate
- ✅ Server prevents cheating (health hacks impossible)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- PLAYER: ✅ Meets UX requirements, enables combat gameplay loop
- ARCHITECT: ✅ Reuses proven patterns, scalable, maintainable

**Scope Constraint:** Fits in one SOW (estimated 7-9 days across 6 phases)

**Dependencies:**
- None (builds on existing attribute system)

**Blocks:**
- ADR-003: Reaction Queue System (needs combat state)
- ADR-004: Ability System and Targeting (needs stamina/mana)
- ADR-005: Damage Pipeline (needs health, armor)
- ADR-006: AI Behavior and Ability Integration (needs all resources)

**Next Steps:**
1. ARCHITECT creates ADR-002 documenting resource management pattern
2. ARCHITECT creates SOW-002 with 6-phase implementation plan
3. DEVELOPER begins Phase 1 (components and calculations)

**Date:** 2025-10-29
