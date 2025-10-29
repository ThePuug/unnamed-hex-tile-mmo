# ADR-002: Combat Foundation - Resources, State, and Attribute Integration

## Status

Proposed

## Context

### Current System State

The codebase has strong foundations for combat:

1. **Complete Attribute System** (`ActorAttributes` in `common/components/mod.rs`):
   - 3-axis attribute pairs (Might/Grace, Vitality/Focus, Instinct/Presence)
   - Axis/Spectrum/Shift mechanics fully tested
   - Derived stat methods already implemented: `max_health()`, `movement_speed()`
   - All getters functional with proper clamping

2. **Triumvirate Classification** (`ActorImpl` in `common/components/entity_type/actor.rs`):
   - Origin, Approach, Resilience enums complete
   - Struct ties classification together
   - Spec defines signature skill mappings

3. **ECS Infrastructure**:
   - Client-server event system (Do/Try pattern)
   - Shared systems philosophy (physics/behavior in `common/`)
   - Serialization support for network sync

4. **Existing GCD Foundation** (`common/systems/gcd.rs`):
   - Basic GcdType enum structure exists
   - Not yet wired into gameplay events

### Problem: No Combat Foundation

To implement the combat system defined in `docs/spec/combat-system.md`, we need:

- **Resource Management**: Health, Stamina, Mana pools with regeneration
- **Combat State Tracking**: When entities are "in combat" and associated effects
- **Attribute Integration**: Scale resources and combat stats from existing `ActorAttributes`
- **Death Handling**: Health depletion, despawn, respawn flow
- **Client-Server Sync**: Predict resources locally, confirm server-authoritative state

Without these foundations, the reaction queue system, damage pipeline, and ability system cannot function.

### MVP Requirements (from spec)

**Phase 1 Scope:**
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

## Decision

We will implement **combat foundation infrastructure** as a shared layer (`common/`) with server authority and client prediction following existing codebase patterns.

### Core Architectural Principles

#### 1. Server-Authoritative Resource Management

**Authority Model:**
- Server has final say on all resource values (HP, stamina, mana)
- Client predicts resource changes for local player (instant feedback)
- Server confirms via incremental updates (similar to `Loc`/`Heading` sync)
- Remote players: client interpolates visual changes, no prediction

**Rationale:**
- Prevents cheating (especially PvP)
- Matches existing position/movement authority model
- Allows rollback if client prediction wrong

#### 2. Shared Calculation in `common/`

**Philosophy:** All combat math lives in `common/` systems/modules, used by both client and server.

**Functions in `common/`:**
- Resource pool calculations (max HP, max stamina, max mana from attributes)
- Regeneration rate calculations
- Damage scaling formulas (Might/Focus → outgoing damage)
- Passive modifier calculations (Vitality/Focus → armor/resistance)
- Combat state transition logic

**Benefits:**
- Single source of truth for formulas
- Client predictions use identical logic to server
- Easier to test (deterministic, no network)
- Prevents client-server drift

#### 3. Component-Based Resource Tracking

**Component Structure:**

Each combat-capable entity has:
- `Health` component (state: f32, step: f32, max: f32)
- `Stamina` component (state: f32, step: f32, max: f32, regen_rate: f32, last_update: Duration)
- `Mana` component (state: f32, step: f32, max: f32, regen_rate: f32, last_update: Duration)
- `CombatState` component (in_combat: bool, last_action: Duration)

**Note on state/step pattern:**
- Mirrors existing `Offset` and `AirTime` components (established pattern for client prediction)
- `state`: Server-authoritative value (confirmed)
- `step`: Client prediction (local player) OR interpolated value (remote entities)
- Pattern applies to ANY value needing prediction, not just frequently-updated values

**Note on time tracking:**
- Use `Duration` from `Time::elapsed()`, NOT `Instant` (clock sync via `Event::Init`)
- Both client and server use Bevy's `Time` resource for consistent frame-based timing
- `last_update` and `last_action` are durations since game start, not system clock times

**Why separate components:**
- Not all entities need all resources (decorators have no stamina)
- Allows selective querying (Query<&Health> vs Query<(&Health, &Stamina)>)
- Easier to add/remove resource types per entity
- Mirrors existing component granularity (Loc, Offset, Heading are separate)

### Detailed Design Decisions

#### Decision 1: Resource Pool Scaling from Attributes

**From spec:**
```
stamina_pool = 100 + (might * 0.5) + (vitality * 0.3)
mana_pool = 100 + (focus * 0.5) + (presence * 0.3)
max_health = ActorAttributes::max_health()  // Already implemented
```

**Implementation Location:** `common/systems/resources.rs` (new module)

**Functions:**
```rust
pub fn calculate_max_stamina(attrs: &ActorAttributes) -> f32;
pub fn calculate_max_mana(attrs: &ActorAttributes) -> f32;
pub fn calculate_stamina_regen_rate(attrs: &ActorAttributes) -> f32; // Base 10/sec, may scale later
pub fn calculate_mana_regen_rate(attrs: &ActorAttributes) -> f32; // Base 8/sec, may scale later
```

**Authority:**
- Server calculates on spawn and whenever attributes change
- Client uses same functions for local prediction
- Server sends `Event::Incremental` with updated max values on attribute change

**Considerations:**
- Attribute changes are rare (level-up, gear) → network updates infrequent
- Clamp current resource to new max if max decreases
- Formulas in `common/` allow easy balance tuning

---

#### Decision 2: Regeneration in FixedUpdate

**Schedule:** Resource regeneration runs in `FixedUpdate` (125ms ticks)

**Rationale:**
- Matches physics simulation schedule (existing pattern)
- Predictable tick rate for regeneration math
- Prevents variable frame rate affecting regen speed
- Client and server regenerate at same rate

**System:** `common/systems/resources::regenerate_resources`

**Logic:**
- Query entities with `(Health, Stamina, Mana, CombatState)`
- Calculate `dt` since `last_update` (Duration stored per-resource)
- Add `regen_rate * dt` to current value, clamp to max
- Update `last_update` to `Time::elapsed()`
- Combat state does NOT affect regen (per spec: "Regenerates in and out of combat")

**Edge Case: Reconnection**
- If client disconnects and reconnects, `last_update` may be stale
- Solution: Server sends full resource state on reconnect (current, max)
- Server sets `last_update` to `Time::elapsed()` on reconnect
- Prevents "burst regen" on reconnect

---

#### Decision 3: Combat State Management

**State Transitions (from spec):**

**Enter Combat:**
- Deal damage to another entity
- Take damage from another entity
- Within aggro radius of hostile entity
- Use offensive ability (even if miss)

**Exit Combat:**
- No hostile entities within 20 hex radius (requires spatial query)
- 5 seconds since last damage dealt/taken
- Entity dies

**Implementation:**
- `CombatState` component: `{ in_combat: bool, last_action: Duration }`
- System: `common/systems/combat_state::update_combat_state` in `FixedUpdate`
- Use existing `NNTree` for 20 hex radius hostile check (returns all entities, filter manually)
- Server-authoritative: server decides when combat ends
- Client predicts entry (instant), accepts server exit confirmation

**NNTree Query Details:**
- `NNTree::locate_within_distance(loc, 20*20)` returns ALL entities within squared distance
- Query does NOT filter by type or faction (raw spatial data)
- Must manually check `EntityType::Actor` after querying
- Filter logic: `if matches!(entity_type, EntityType::Actor(_)) { /* is NPC or player */ }`

**Effects of Combat State:**
- UI shows/hides combat elements (reaction queue, resource bars)
- Prevent mounting/fast travel (client-side check, server validates)
- Combat music toggle (client-only)
- Cannot interact with vendors (server validates)

**Network Sync:**
- `Event::Incremental { component: Component::CombatState(bool) }`
- Only send on state change (not every frame)
- Client prediction: set in_combat=true immediately on offensive action
- Server confirmation: accept server's exit decision (may differ from client's 5sec timer due to latency)

**MVP Hostile Entity Detection:**
- Query all entities within 20 hexes via `NNTree::locate_within_distance()`
- Filter to `EntityType::Actor` (all actors are potential hostiles)
- MVP assumption: All NPCs (`Behaviour != Controlled`) are hostile, all players (`Behaviour::Controlled`) are friendly
- Simple check: if any non-Controlled Actor within 20 hexes, stay in combat
- Future: Add `Faction` component for PvP, friendly NPCs, guild systems (out of MVP scope)

---

#### Decision 4: Death State and Respawn

**Death Trigger:** `Health.current <= 0.0`

**Server Flow:**
1. Detect death in `common/systems/resources::check_death` system
2. Emit `Try { event: Event::Death { ent } }` (server-internal, follows Do/Try pattern)
3. Server death handler processes `Try::Death` events
4. For players: schedule respawn at hub after 5 seconds, set resources to 0
5. For NPCs: mark for despawn, notify parent spawner for respawn timer
6. Broadcast `Do { event: Event::Despawn { ent } }` to clients (note: Despawn already exists in Event enum)

**Client Flow:**
1. Receive `Event::Despawn { ent }`
2. Play death animation (if time allows before despawn)
3. Remove entity from world
4. If local player: show respawn UI overlay

**Resource State on Death:**
- Set all resources to 0 immediately (prevents "zombie" state)
- On respawn: restore to full (max_health, max_stamina, max_mana)

**Edge Case: Mutual Destruction**
- Two entities have lethal damage queued simultaneously
- Both die (per spec: "Both entities die" - no tie-breaker)
- Server processes both deaths independently in same frame
- Outcome: Both despawn, both respawn (if players)

**Implementation Note:**
- Death system runs AFTER damage application system
- Allows damage to apply, then check for death
- Prevents race conditions (damage applied after death check)

---

#### Decision 5: Armor and Resistance as Derived Stats

**From spec:**
```
armor = base_armor + (vitality / 200.0)  // Max 75% cap
resistance = base_resistance + (focus / 200.0)  // Max 75% cap
```

**Design Choice: Calculate on-demand, not store**

**Rationale:**
- Armor/resistance derive from attributes (already stored)
- Avoid duplicate state (attributes + armor component both exist)
- Calculation is cheap (division + addition)
- Matches existing pattern (`ActorAttributes::max_health()` calculates on-call)

**Implementation:**
- Functions in `common/systems/combat.rs`:
  ```rust
  pub fn calculate_armor(attrs: &ActorAttributes) -> f32;
  pub fn calculate_resistance(attrs: &ActorAttributes) -> f32;
  ```
- Called during damage application (ADR-004 scope)
- No network sync needed (attributes already synced)

**Alternative Considered: Store in component**
- Would require syncing armor/resistance on every attribute change
- More network traffic
- Risk of desync if calculation logic changes
- Rejected: Keep calculations derived

---

#### Decision 6: Client Prediction Strategy

**Local Player (Controlled):**
- **Predict:** Resource expenditure (stamina for Dodge), damage taken (when hit confirmed)
- **Rollback:** Server sends corrected resource values if prediction wrong
- **Visual:** Use predicted values for UI (smooth experience)

**Remote Players:**
- **No prediction:** Display server-confirmed values only
- **Interpolation:** Smooth health bar changes over 100-200ms
- **Visual:** Lag slightly but always accurate

**Rationale:**
- Matches existing client-side prediction model (movement in `controlled.rs`)
- Local player gets instant feedback (critical for combat feel)
- Remote players: accuracy > responsiveness (observing others' combat)
- Prevents "health desync" perception issues in PvP

**Implementation Pattern (from existing `Offset` component):**
```rust
pub struct Health {
    pub state: f32,      // Server-authoritative HP
    pub step: f32,       // Client prediction (local) OR interpolated (remote)
    pub max: f32,        // Max HP from attributes
}
```

**Prediction Flow:**
1. Local player uses Dodge → predict `stamina.step -= 30.0`
2. Client updates stamina bar immediately (uses `step`)
3. Server confirms → sends `Event::Incremental { Stamina(current: 70.0) }`
4. Client sets `stamina.state = 70.0`, adjusts `step` if needed

**Rollback Scenario:**
- Client predicts: stamina = 30, uses Dodge (predicts 0 stamina)
- Server calculates: stamina = 28 (regen desync), Dodge fails (insufficient stamina)
- Server sends: `stamina.current = 28`, `Dodge failed` event
- Client snaps `step = 28`, shows "Not enough stamina" message

---

#### Decision 7: Attribute Change Propagation

**Scenario:** Attributes change (level-up, gear, temporary buff)

**Server Flow:**
1. Update `ActorAttributes` component
2. Recalculate derived maxes: `max_health()`, `calculate_max_stamina()`, `calculate_max_mana()`
3. Clamp current values to new maxes (if max decreased)
4. Emit `Event::Incremental` with updated `ActorAttributes`
5. Emit `Event::Incremental` for each resource with new max

**Client Flow:**
1. Receive `Event::Incremental { ActorAttributes }`
2. Update local `ActorAttributes` component
3. Receive `Event::Incremental { Health/Stamina/Mana }` with new maxes
4. Recalculate local maxes (redundant check, validates server)
5. Update UI (health bar max, stamina bar max)

**Why send both attributes and resources:**
- Attributes needed for damage calculations (Might/Focus)
- Resource maxes needed for UI display
- Separating allows partial updates (e.g., temporary stamina buff without attribute change)

**Frequency:**
- Rare in MVP (no leveling, no gear)
- Future: May optimize with batching if frequent

---

#### Decision 8: Resource Network Message Structure

**New Event Types (add to `common/message.rs`):**

```rust
pub enum Event {
    // Existing events...
    Despawn { ent: Entity },  // Already exists - reuse for death despawn

    /// Update health (Do event - broadcast to clients)
    Health { ent: Entity, current: f32, max: f32 },

    /// Update stamina (Do event - broadcast to clients)
    Stamina { ent: Entity, current: f32, max: f32, regen_rate: f32 },

    /// Update mana (Do event - broadcast to clients)
    Mana { ent: Entity, current: f32, max: f32, regen_rate: f32 },

    /// Combat state change (Do event - broadcast to clients)
    CombatState { ent: Entity, in_combat: bool },

    /// Entity died (Try event - server-internal only)
    Death { ent: Entity },
}
```

**Event Classification (Do vs Try):**
- **Try events** (server-internal requests): `Death`
- **Do events** (server → client broadcasts): `Health`, `Stamina`, `Mana`, `CombatState`, `Despawn`
- Follows existing pattern: `DiscoverChunk` (Try), `ChunkData` (Do)

**Message Frequency:**
- **Health**: Send on damage application, death, spawn, attribute change
- **Stamina/Mana**: Send on ability use, spawn, attribute change (NOT every regen tick)
- **CombatState**: Send on state transition only (enter/exit combat)
- **Death**: Send immediately on HP <= 0

**Bandwidth Consideration:**
- Resources do NOT sync every frame (unlike position)
- Regeneration handled locally (client and server both run regen system)
- Only send on discrete events (damage, ability use, state change)
- For 100 players in combat: ~500 bytes/sec/player (acceptable)

**Optimization: Batch Updates**
- If multiple resources change simultaneously (e.g., use ability costs stamina, deals damage, reduces target's health), batch into single message
- Future enhancement (out of MVP scope)

---

## Consequences

### Positive

#### 1. Solid Foundation for Combat

- All future combat systems (reaction queue, abilities, damage) build on these components
- Resource management tested independently before complex interactions
- Clear authority model prevents cheating

#### 2. Reuses Existing Patterns

- `Health.state/step` mirrors `Offset.state/step` prediction model
- Shared calculations in `common/` follows physics/behavior precedent
- FixedUpdate regeneration matches physics schedule
- Do/Try event pattern consistent with movement

#### 3. Attribute System Pays Off

- Investment in `ActorAttributes` directly enables combat scaling
- Formulas in spec map cleanly to attribute getters
- No additional stat-tracking needed (attributes are the stats)

#### 4. MVP Scoped Correctly

- Wild Dog + Basic Attack + Dodge tests entire resource flow
- Can validate prediction/confirmation cycle before complex abilities
- Death and respawn included (prevents "immortal player" prototype)

#### 5. Extensible for Future Systems

- Combat state hooks allow mounting/travel restrictions
- Resource components support future types (energy, rage, etc.)
- Derived stats pattern scales to more passive modifiers (evasion, leech, etc.)

### Negative

#### 1. Network Message Expansion

- 5 new event types increase protocol complexity
- Must version protocol or document breaking changes
- Serialization/deserialization testing burden

**Mitigation:**
- MVP only uses Health, Stamina, CombatState
- Mana can be added later without breaking changes (additive)
- Keep messages minimal (no redundant data)

#### 2. Client Prediction Complexity

- Health prediction harder than position (affects life-or-death decisions)
- Rollback on failed ability use can feel bad (predicted success, server denied)
- Requires careful UX (show "waiting for confirmation" during prediction)

**Mitigation:**
- Start with simple predictions (stamina cost only)
- Add visual feedback for "predicted state" vs "confirmed state"
- Tune prediction confidence (predict conservatively)

#### 3. Combat State Detection Overhead

- Checking "hostiles within 20 hexes" every tick is expensive
- NNTree query every 125ms for every player in combat

**Mitigation:**
- MVP: Only check when considering combat exit (in_combat=true already)
- Optimize: Batch queries, cache results, use spatial events
- Future: Faction system reduces query complexity (only check hostile factions)

#### 4. Death Flow Interrupts Gameplay

- Player death → respawn at hub → lose position
- Could feel punishing (lose progress, long walk back)

**Mitigation:**
- MVP: Accept this (deaths rare in early testing)
- Future: Respawn at nearest haven, death penalties, graveyards
- Spec defines Haven system for respawns (separate ADR)

#### 5. Resource Regeneration Determinism

- Client and server must agree on regen timing
- Desync causes prediction errors and rollbacks
- `last_update_time` must be carefully managed (especially on reconnect)

**Mitigation:**
- Always use FixedUpdate (same tick rate)
- Server sets `last_update_time` on spawn and reconnect
- Test with varying latency (simulate packet loss)

### Neutral

#### 1. No Resource Cost for Movement

- Spec does not mention stamina drain for walking/running
- Could be added later if desired (sprinting costs stamina)
- MVP: Free movement, stamina only for combat

#### 2. No Mana in MVP

- Wild Dog + Basic Attack + Dodge only use stamina
- Mana system designed but not exercised until magic abilities added
- May discover issues when Mana finally used

**Acceptance:** Mana is simple (copy of stamina), low risk of issues

#### 3. No Health Regeneration

- Stamina/mana regen defined (10/sec, 8/sec)
- Health regen not in spec (healing abilities only?)
- MVP: No passive health regen (deaths more meaningful)

**Consideration:** May need to add passive regen out-of-combat for playability

## Implementation Phases

### Phase 1: Core Components and Calculations (Foundation)

**Goal:** Add resource components and calculation functions

**Tasks:**
1. Create `common/components/health.rs`:
   - `Health { state: f32, step: f32, max: f32 }`
   - `Stamina { state: f32, step: f32, max: f32, regen_rate: f32, last_update: Instant }`
   - `Mana { state: f32, step: f32, max: f32, regen_rate: f32, last_update: Instant }`
   - `CombatState { in_combat: bool, last_action: Instant }`

2. Create `common/systems/resources.rs`:
   - `calculate_max_stamina(attrs: &ActorAttributes) -> f32`
   - `calculate_max_mana(attrs: &ActorAttributes) -> f32`
   - `calculate_stamina_regen_rate(attrs: &ActorAttributes) -> f32`
   - `calculate_mana_regen_rate(attrs: &ActorAttributes) -> f32`
   - `calculate_armor(attrs: &ActorAttributes) -> f32`
   - `calculate_resistance(attrs: &ActorAttributes) -> f32`

3. Add tests for all calculation functions:
   - Test with attribute extremes (-100, 0, 100)
   - Validate caps (armor at 75% max)
   - Verify formulas match spec

**Success Criteria:**
- All components compile and serialize
- All calculation tests pass
- No behavior changes yet (components not used)

**Duration:** 1 day

---

### Phase 2: Resource Initialization on Spawn

**Goal:** Entities spawn with correct resource values

**Tasks:**
1. Update `server/systems/renet.rs::do_manage_connections` (player spawn on connect):
   - Calculate initial resources from `ActorAttributes` using `common/systems/resources.rs` functions
   - Insert `Health`, `Stamina`, `Mana` components with `state`/`step` initialized to max, `last_update` to `Time::elapsed()`
   - Initialize `CombatState { in_combat: false, last_action: Time::elapsed() }`
   - Location: Inside `ServerEvent::ClientConnected` handler (currently line 54-85)

2. Update `server/systems/spawner.rs::spawn_npc` (NPC spawn from spawner):
   - Same resource initialization for NPCs using their `ActorAttributes`
   - Insert all four components (Health, Stamina, Mana, CombatState)
   - Location: Inside `spawn_npc` function (currently line 77+)

3. Network sync:
   - Extend existing `Event::Spawn` to include initial resources (OR send separate events)
   - Client receives and inserts matching components with `state`/`step` initialized

**Success Criteria:**
- Wild Dog spawns with 100 HP, calculated stamina
- Player spawns with attribute-scaled resources
- Client receives and displays correct values
- UI shows resource bars (next phase implements UI)

**Duration:** 1 day

---

### Phase 3: Resource Regeneration System

**Goal:** Stamina and mana regenerate over time

**Tasks:**
1. Create `common/systems/resources::regenerate_resources`:
   - Run in FixedUpdate
   - Query `(Stamina, Mana)` components
   - Calculate `dt` from `last_update`
   - Add `regen_rate * dt`, clamp to max
   - Update `last_update`

2. Add to schedules:
   - Server: `app.add_systems(FixedUpdate, regenerate_resources)`
   - Client: `app.add_systems(FixedUpdate, regenerate_resources)`

3. Client prediction:
   - Local player: regen uses predicted `step`
   - Remote players: regen uses server `state`

4. Test regeneration:
   - Drain stamina, verify regen to full
   - Measure regen rate (should be 10/sec for stamina)

**Success Criteria:**
- Resources regenerate at correct rate
- Client and server agree on regenerated values (within tolerance)
- UI bars visibly fill over time

**Duration:** 1 day

---

### Phase 4: Combat State Management

**Goal:** Entities enter and exit combat state correctly

**Tasks:**
1. Create `common/systems/combat_state::update_combat_state`:
   - Run in FixedUpdate
   - Check 5 seconds since `last_action` → exit combat
   - Query NNTree for hostiles within 20 hexes
   - Update `in_combat` flag
   - Emit `Event::CombatState` on change (server only)

2. Trigger combat entry:
   - When damage dealt (future system calls `set_in_combat(ent)`)
   - When damage taken (same)
   - When ability used (same)

3. Client response:
   - Receive `Event::CombatState`
   - Update local component
   - Show/hide combat UI elements
   - Play combat music (toggle)

**Success Criteria:**
- Player enters combat when attacking Wild Dog
- Combat state persists while fighting
- Combat exits 5 seconds after disengaging
- UI responds correctly to state changes

**Duration:** 1-2 days

---

### Phase 5: Death and Respawn

**Goal:** Entities die at 0 HP and respawn

**Tasks:**
1. Create `common/systems/resources::check_death`:
   - Run after damage application (later phase)
   - Query entities with `Health.current <= 0.0`
   - Emit `Event::Death { ent }`

2. Server death handler:
   - Receive `Event::Death`
   - For NPCs: despawn immediately, mark respawn timer
   - For players: wait 5 seconds, respawn at hub
   - Restore resources to full on respawn
   - Emit `Event::Despawn` to clients

3. Client death handler:
   - Receive `Event::Despawn`
   - Remove entity from world
   - If local player: show respawn UI

**Success Criteria:**
- Wild Dog despawns when HP reaches 0
- Player dies and respawns at hub after 5 seconds
- Resources restored to full on respawn
- Death feels responsive (no "zombie" lingering)

**Duration:** 1-2 days

---

### Phase 6: UI Integration

**Goal:** Display health and stamina bars

**Tasks:**
1. Create health bar widget (above entity):
   - Green bar, depletes left-to-right
   - Show for all entities in combat
   - Hide when full and out of combat

2. Create stamina bar widget (player HUD):
   - Yellow bar, shows current/max
   - Always visible in combat
   - Dim when out of combat

3. Hook to components:
   - Query `(Health, Stamina)` for display
   - Use `step` for local player (predicted)
   - Use `state` for remote players (confirmed)
   - Interpolate changes smoothly (no snapping)

4. Combat state UI toggle:
   - Show/hide bars based on `CombatState.in_combat`
   - Fade in/out transitions

**Success Criteria:**
- Health bars visible on Wild Dog when damaged
- Player stamina bar always visible
- Bars update smoothly (no jitter)
- Predicted stamina shows instant changes

**Duration:** 2 days

---

## Validation Criteria

### Functional Tests

- **Resource Initialization:** Spawn 100 entities, verify all have correct resources from attributes
- **Regeneration Accuracy:** Drain stamina to 0, measure time to full (should be 10 seconds)
- **Combat State Transitions:** Attack NPC, verify combat entry; disengage, verify exit after 5 seconds
- **Death Flow:** Reduce entity HP to 0, verify despawn and respawn (for players)
- **Attribute Changes:** Modify attributes, verify resource maxes update and clamp current values

### Network Tests

- **Sync Accuracy:** Client and server resources match within 1% after 60 seconds
- **Prediction Rollback:** Force server denial (low stamina), verify client rolls back
- **Reconnection:** Disconnect client, reconnect, verify resources restored correctly (no stale timestamps)

### Performance Tests

- **Combat State Queries:** 100 entities checking hostiles every tick, CPU usage < 5% (single core)
- **Resource Updates:** 1000 entities regenerating, no frame drops (125ms ticks maintained)

### UX Tests

- **Responsiveness:** Local player stamina bar updates within 16ms of ability use (1 frame)
- **Clarity:** Player can see health/stamina at all times during combat
- **Feedback:** Death is obvious (screen effect, UI message, respawn timer)

## Open Questions

### Design Questions

1. **Health Regeneration Out of Combat?**
   - Spec doesn't define passive health regen
   - MVP: No regen (healing abilities only)
   - Concern: Players may need to return to town too often
   - Decision: Test without regen, add if needed for playability

2. **Respawn Location?**
   - Spec mentions "respawn at hub"
   - Which hub? (closest? last visited? home hub?)
   - MVP: Hardcode respawn at world origin (0,0)
   - Future: Haven system (ADR for hub/haven respawns)

3. **Death Penalty?**
   - Lose items? Experience? Time?
   - MVP: No penalty (respawn is inconvenience enough)
   - Future: Consider gear durability, experience debt

4. **Resource Bar Positioning?**
   - Above character (MMO style) or bottom HUD (MOBA style)?
   - MVP: Player bars in bottom HUD, enemy bars above entities
   - Iterate based on playtest feedback

### Technical Questions

1. **Prediction Confidence Thresholds?**
   - When should client NOT predict? (high latency? low confidence?)
   - MVP: Always predict local player, never predict remote
   - Future: Adaptive prediction based on latency

2. **Combat State Spatial Query Optimization?**
   - NNTree query every tick expensive for many entities
   - Alternatives: Spatial hashing, zone-based tracking, event-driven
   - MVP: Accept cost (optimize if profiling shows bottleneck)

3. **Resource Clamp on Max Decrease?**
   - If max_stamina drops from 150 → 100, and current is 120:
   - Clamp to 100? Or preserve overage temporarily?
   - MVP: Clamp immediately (simpler, prevents exploits)

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

- **Health Regeneration:** Out-of-combat regen at 5 HP/sec
- **Resource Types:** Energy, Rage, etc. (class-specific)
- **Status Effects:** Buffs/debuffs that modify regen rates or maxes
- **Death Penalties:** Durability loss, experience debt
- **Respawn System:** Haven-based respawns, graveyards
- **Combat Log:** Text feed of damage dealt/taken (debugging and feedback)

### Optimization

- **Batch Resource Updates:** Send multiple resource changes in single message
- **Spatial Event System:** Trigger combat exit via spatial events (no polling)
- **Resource Compression:** Send deltas instead of absolute values (save bandwidth)

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (resource formulas, MVP scope)
- **Attribute System:** `docs/spec/attribute-system.md` (axis/spectrum mechanics)
- **Triumvirate System:** `docs/spec/triumvirate.md` (signature skill mapping)

### Codebase

- **Existing Attributes:** `src/common/components/mod.rs` - `ActorAttributes`
- **Prediction Pattern:** `src/common/components/offset.rs` - `Offset.state/step` model
- **Client Prediction:** `GUIDANCE/ControlledPlugin.md` - input queue and prediction
- **Fixed Update Systems:** `src/common/systems/physics.rs` - FixedUpdate schedule example
- **Network Events:** `src/common/message.rs` - Do/Try pattern

### Related ADRs

- **(Future) ADR-003:** Reaction Queue System Architecture
- **(Future) ADR-004:** Ability System and Targeting
- **(Future) ADR-005:** Damage Pipeline and Combat Resolution
- **(Future) ADR-006:** Hub and Haven Respawn System

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md`
- Existing codebase patterns: client-side prediction, shared systems in `common/`

## Date

2025-10-29

---

## Implementation Clarifications (Developer Q&A)

### Q1: Should resources use state/step pattern given their update frequency?

**A:** Yes. The pattern is about **prediction vs authority**, not update frequency.

- `Offset`, `AirTime` both use state/step (established pattern)
- Resources like Stamina change frequently in combat (ability use, regen)
- Client needs instant feedback without server round-trip
- Pattern is the right abstraction for any predicted value

### Q2: Should we use Instant or Duration for time tracking?

**A:** Use `Duration` from `Time::elapsed()`, NOT `Instant`.

- Server has `RunTime` resource for timing offset (already handles clock sync)
- All systems use `Res<Time>` (bevy's frame-based timing)
- Server sends time offset in `Event::Init` on connection
- Components store `Duration` from `Time::elapsed()`, not system clock `Instant`

### Q3: Does NNTree support filtering by entity type/faction?

**A:** No. Query returns ALL entities, manual filtering required.

- `NNTree::locate_within_distance()` returns raw spatial data (all entities)
- Must query `EntityType` component after spatial query
- MVP: Filter to `EntityType::Actor`, assume all NPCs hostile (`Behaviour != Controlled`)
- Future: Add `Faction` component for PvP/friendly NPCs (out of MVP scope)

### Q4: Is Event::Death a Try or Do event?

**A:** `Death` is a **Try event** (server-internal). `Despawn` is a **Do event** (broadcast).

**Correct Flow:**
1. System detects HP <= 0 → emit `Try { event: Event::Death { ent } }`
2. Server death handler processes Try::Death
3. Handler emits `Do { event: Event::Despawn { ent } }` (already exists in Event enum)
4. Clients receive Do::Despawn and remove entity

Follows existing pattern: `DiscoverChunk` (Try), `ChunkData` (Do)

### Q5: Where should resource initialization happen on spawn?

**A:** Two locations (player spawn and NPC spawn):

**Player spawn:** `server/systems/renet.rs::do_manage_connections`
- Inside `ServerEvent::ClientConnected` handler (line 54-85)
- Add Health/Stamina/Mana/CombatState components here

**NPC spawn:** `server/systems/spawner.rs::spawn_npc`
- Inside `spawn_npc` function (line 77+)
- Add same four components for NPCs

Both use `common/systems/resources.rs` calculation functions to derive resources from `ActorAttributes`.
