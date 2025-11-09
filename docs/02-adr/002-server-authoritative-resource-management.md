# ADR-002: Server-Authoritative Resource Management with Client Prediction

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-002: Combat Foundation](../01-rfc/002-combat-foundation.md)

We need to manage combat resources (health, stamina, mana) for potentially 1,000 concurrent players in a client-server architecture. The fundamental architectural question: **How do we synchronize frequently-changing resource values between client and server while maintaining responsiveness and preventing cheating?**

### Requirements

1. **Server authority:** Prevent client-side cheating (especially for PvP)
2. **Client responsiveness:** Instant feedback for local player actions
3. **Network efficiency:** Avoid spamming resource updates every tick
4. **Deterministic simulation:** Client and server can independently calculate same values
5. **Existing pattern reuse:** Match proven movement/position sync model

### Options Considered

#### Option 1: Server Broadcasts Every Change (Push Model)
- Server calculates all resource changes
- Broadcasts updates to clients every tick (125ms)
- Clients display server values

**Pros:** Simple, always accurate
**Cons:**
- Network spam: 100 players × 3 resources × 8 ticks/sec = 2,400 messages/sec
- Poor UX: 125ms latency on local actions
- Doesn't scale

#### Option 2: Client Authority (Trust Model)
- Clients calculate their own resources
- Send final values to server
- Server validates and broadcasts

**Pros:** Instant local feedback, minimal network traffic
**Cons:**
- Cheating trivial (client sends fake values)
- Unsuitable for PvP
- Violates "server is authority" principle

#### Option 3: Server Authority + Client Prediction (Hybrid)
- Server is authoritative for all values
- Client predicts local player changes (instant feedback)
- Both run identical deterministic systems (regeneration)
- Network only syncs on discrete events (damage, ability use)
- Server sends corrections if prediction wrong

**Pros:**
- Responsive UX (instant local feedback)
- Cheat-proof (server validates)
- Network efficient (no regeneration spam)
- Scales to 1,000+ players

**Cons:**
- More complex (prediction + rollback logic)
- Potential desync (requires reconciliation)

## Decision

**We will implement Option 3: Server-Authoritative Resource Management with Client Prediction and Deterministic Simulation.**

### Core Architectural Pattern

#### Component Structure: State/Step Prediction Pattern

```rust
pub struct Health {
    pub state: f32,   // Server-authoritative value (confirmed)
    pub step: f32,    // Client prediction (local) OR interpolation target (remote)
    pub max: f32,     // Max HP from attributes
}

pub struct Stamina {
    pub state: f32,
    pub step: f32,
    pub max: f32,
    pub regen_rate: f32,
    pub last_update: Duration,
}

// Similar for Mana, CombatState
```

**Pattern Reuse:** This mirrors existing `Offset` and `AirTime` components (proven in movement system).

**Rationale:**
- `state` = server truth
- `step` = client prediction (local player) OR smooth interpolation (remote players)
- UI displays `step` (responsive for local, smooth for remote)
- Server periodically confirms `state` (corrections if needed)

#### Separation of Concerns: Multiple Components

**Decision:** Health, Stamina, Mana, CombatState as separate components (NOT one "Resources" component)

**Rationale:**
- Not all entities need all resources (decorators have no stamina/mana)
- Selective querying: `Query<&Health>` vs `Query<(&Health, &Stamina)>`
- Composition over monolith (ECS best practice)
- Easier to extend (add new resource without changing existing)
- Matches existing granularity (`Loc`, `Offset`, `Heading` are separate)

---

### Key Design Decisions

#### 1. Deterministic Regeneration (Zero Network Traffic)

**Decision:** Regeneration runs identically on client and server, no synchronization needed.

**System:** `common/systems/resources::regenerate_resources`
- Runs in `FixedUpdate` (125ms ticks) on both client and server
- Uses shared formula: `new_value = current + (regen_rate * dt)`
- Both use `Time::elapsed()` Duration (not Instant) for consistent timing

**Network Impact:**
- Regeneration traffic: **0 bytes/sec**
- Only sync on discrete events (damage, ability use, spawn)
- Scales to unlimited players (regeneration cost is local CPU only)

**Reconciliation:**
- Client and server drift < 1% due to identical formulas
- Server sends correction only if client-server diff > 5%
- Rare corrections prevent unbounded drift

**Why This Works:**
- Regeneration is pure math (deterministic)
- Both use same tick rate (FixedUpdate at 125ms)
- No external inputs (no random factors)
- Formula lives in `common/` (shared code)

---

#### 2. Shared Calculations in `common/`

**Decision:** All combat formulas live in `common/` modules, used by both client and server.

**Functions in `common/systems/resources.rs`:**
```rust
pub fn calculate_max_stamina(attrs: &ActorAttributes) -> f32;
pub fn calculate_max_mana(attrs: &ActorAttributes) -> f32;
pub fn calculate_stamina_regen_rate(attrs: &ActorAttributes) -> f32;
pub fn calculate_mana_regen_rate(attrs: &ActorAttributes) -> f32;
pub fn calculate_armor(attrs: &ActorAttributes) -> f32;
pub fn calculate_resistance(attrs: &ActorAttributes) -> f32;
```

**Benefits:**
- Single source of truth for all formulas
- Client predictions use identical logic to server
- Easier to test (deterministic, no network)
- Prevents client-server drift by design
- Balance tuning changes one location

**Testing Strategy:**
- Unit tests for each formula (extreme values, edge cases)
- Verify attribute scaling matches spec exactly
- Test caps (armor at 75% max)
- No integration testing needed (pure functions)

---

#### 3. Time Tracking with Duration (Not Instant)

**Decision:** Use `Duration` from `Time::elapsed()`, NOT `Instant::now()`.

**Rationale:**
- `Instant::now()` uses system clock (different on client and server)
- `Time::elapsed()` synchronized via `Event::Init` on connection
- Both client and server measure durations from same epoch
- Enables time-based calculations (regeneration, combat state timeout)

**Implementation:**
```rust
pub struct Stamina {
    pub last_update: Duration,  // NOT Instant
}

// In regeneration system
let now = time.elapsed();  // Bevy's Time resource
let dt = now - stamina.last_update;
```

**Edge Case Handling:**
- Player reconnects: Server resets `last_update` to current `Time::elapsed()`
- Prevents burst regeneration from stale timestamps

---

#### 4. Combat State Management via Spatial Queries

**Decision:** Combat state transitions use NNTree spatial queries + timeout.

**Enter Combat:**
- Deal/take damage
- Use offensive ability
- Within 20 hex radius of hostile entity

**Exit Combat:**
- No hostile entities within 20 hexes (NNTree query)
- 5 seconds since last action
- Entity dies

**System:** `common/systems/combat_state::update_combat_state`
- Runs in FixedUpdate (server only)
- Query: `NNTree::locate_within_distance(loc, 20*20)` (squared distance)
- Filter: Manually check `EntityType::Actor` after query (NNTree returns ALL entities)
- Timeout: `last_action.saturating_sub(Duration::from_secs(5))`

**Network:**
- Only broadcast on state change (enter/exit)
- Client predicts entry (instant UX)
- Server confirms exit (authoritative timeout)

---

#### 5. Armor/Resistance as Derived Stats (Not Stored)

**Decision:** Calculate armor/resistance on-demand from attributes, do NOT store in component.

**Rationale:**
- Armor/resistance derive from attributes (no independent state)
- Calculation is cheap: `armor = base + (vitality / 200.0)`, capped at 75%
- Avoids duplicate state (attributes + armor component)
- Matches existing pattern (`ActorAttributes::max_health()` calculates on-call)
- No network sync needed (attributes already synced)

**Alternative Rejected:** Storing in component
- Would require syncing on every attribute change
- More network traffic
- Risk of desync if calculation changes
- No performance benefit (calculation ~10 CPU cycles)

---

#### 6. Death System Schedule Order

**Decision:** Death detection runs AFTER damage application in schedule.

**Critical Invariant:** Damage must apply before checking for death.

**Schedule:**
```rust
app.add_systems(Update, (
    apply_damage_system,   // Reduces health
    check_death,           // Detects HP <= 0
    handle_death,          // Processes death events
).chain());
```

**Rationale:**
- Prevents race condition (checking death before damage applied)
- Enables mutual destruction (both entities die if lethal damage queued)
- Clear causality: damage → death → despawn

**Edge Case: Mutual Destruction**
- Two entities have lethal damage queued
- Both damages apply in same frame
- Both death checks trigger
- Both entities die (per spec: "no tie-breaker")

---

## Consequences

### Positive

**1. Performance at Scale**
- Regeneration network traffic: **0 bytes/sec** (vs 2,400 msg/sec if broadcast)
- Scales to 1,000+ players with negligible CPU impact
- Network only used for discrete events (damage, abilities)

**2. Responsive UX**
- Local player sees instant feedback (< 16ms)
- Remote players have smooth interpolation (no jitter)
- Matches responsiveness of commercial MMOs

**3. Cheat-Proof**
- Server has final authority on all values
- Client predictions validated and corrected
- Suitable for PvP without additional security

**4. Reuses Proven Patterns**
- State/step prediction (movement system)
- Shared calculations (physics systems)
- FixedUpdate scheduling (physics simulation)
- Do/Try event pattern (terrain discovery)

**5. Testability**
- Pure functions in `common/` (easy unit tests)
- Deterministic systems (reproducible behavior)
- No network mocking needed for formula tests

### Negative

**1. Complexity**
- Prediction + rollback logic (more code than server-only)
- Two codepaths: local (predict) vs remote (interpolate)
- Requires understanding state/step pattern

**2. Potential Desync**
- Client and server may drift due to timing differences
- Requires periodic reconciliation (adds network traffic)
- Tolerance threshold needed (how much drift is acceptable?)

**3. Rollback UX**
- Predicted action may be rejected by server (jarring snap)
- Example: Client predicts Dodge, server says "insufficient stamina"
- Mitigation: Client-side validation before prediction

**4. Testing Burden**
- Must test both client and server regeneration
- Must test prediction accuracy (within tolerance)
- Must test rollback scenarios (server rejection)

### Mitigations

**Desync Prevention:**
- Use shared code in `common/` (guaranteed identical logic)
- FixedUpdate at same rate (125ms) on both sides
- Periodic corrections (server sends state if drift > 5%)

**Rollback UX:**
- Client validates before predicting (e.g., check stamina before Dodge)
- Show "Not enough stamina" client-side (prevent prediction failure)
- Smooth rollback animation (no sudden snap)

---

## Alternatives Rejected

**Store all resources in one component:**
- Violates ECS composition principle
- Forces over-fetching in queries
- Harder to extend
- **Rejected:** Use separate components

**Broadcast regeneration ticks:**
- 2,400 messages/sec for 100 players (doesn't scale)
- Wastes network bandwidth
- Defeats purpose of deterministic systems
- **Rejected:** Use deterministic simulation

**Client authority:**
- Enables cheating (unacceptable for PvP)
- Violates server authority principle
- **Rejected:** Server must be authoritative

---

## Implementation Notes

See [SOW-002](../03-sow/002-combat-foundation.md) for 6-phase implementation plan covering:
1. Core components and calculations
2. Resource initialization on spawn
3. Regeneration system
4. Combat state management
5. Death and respawn
6. UI integration

---

## References

- **RFC-002:** Feature request and feasibility analysis
- **ADR-003:** [Component-Based Resource Separation](003-component-based-resource-separation.md) - Why separate components
- **ADR-004:** [Deterministic Resource Regeneration](004-deterministic-resource-regeneration.md) - Zero network traffic approach
- **ADR-005:** [Derived Combat Stats](005-derived-combat-stats.md) - On-demand calculation vs storage
- **SOW-002:** Implementation plan and acceptance review
- **Existing Pattern:** `Offset` component (movement prediction)
- **Existing Pattern:** `common/systems/physics.rs` (shared calculations)

---

## Date

2025-10-29
