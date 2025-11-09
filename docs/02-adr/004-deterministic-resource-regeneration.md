# ADR-004: Deterministic Resource Regeneration

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-002: Combat Foundation](../01-rfc/002-combat-foundation.md)

Resources (stamina, mana) regenerate continuously over time. With 1,000 concurrent players, we need a network-efficient approach. The fundamental architectural question: **How do we synchronize resource regeneration between client and server without network spam?**

### Scale Requirements

- **Target:** 1,000 concurrent players
- **Regenerating resources:** 2 per player (stamina, mana)
- **Regeneration rate:** Continuous (10/sec stamina, 8/sec mana)
- **Network constraint:** Minimize bandwidth for continuous state changes

### Options Considered

#### Option 1: Server Broadcasts Every Tick

Server calculates regeneration and broadcasts updates:

```rust
// Server FixedUpdate (125ms = 8 ticks/sec)
fn regenerate_and_broadcast(
    mut query: Query<(Entity, &mut Stamina, &mut Mana)>,
    mut writer: EventWriter<Do>,
) {
    for (ent, mut stamina, mut mana) in &mut query {
        stamina.state += stamina.regen_rate * 0.125;
        mana.state += mana.regen_rate * 0.125;

        // Broadcast every tick
        writer.write(Do { event: Event::Stamina { ent, current: stamina.state, .. } });
        writer.write(Do { event: Event::Mana { ent, current: mana.state, .. } });
    }
}
```

**Network cost:**
- 1,000 players × 2 resources × 8 ticks/sec = **16,000 messages/sec**
- Message size: ~16 bytes each
- Bandwidth: **256 KB/sec**

**Pros:**
- Simple implementation
- Always accurate (no desync possible)
- Client just displays received values

**Cons:**
- **Network spam:** 16,000 msg/sec is unacceptable
- Doesn't scale beyond ~100 players
- Wastes bandwidth on predictable state changes

---

#### Option 2: Client Authority (Calculate Locally)

Client calculates regeneration and sends final values to server:

```rust
// Client FixedUpdate
fn regenerate_local(
    mut query: Query<(Entity, &mut Stamina, &mut Mana)>,
    mut writer: EventWriter<Try>,
) {
    for (ent, mut stamina, mut mana) in &mut query {
        stamina.step += stamina.regen_rate * 0.125;
        mana.step += mana.regen_rate * 0.125;

        // Send to server periodically (every 5 seconds)
        if should_sync {
            writer.write(Try { event: Event::UpdateStamina { ent, value: stamina.step } });
        }
    }
}
```

**Network cost:**
- 1,000 players × 2 resources × (1/5 sec) = **400 messages/sec**
- Much lower bandwidth

**Pros:**
- Low network traffic
- Instant local feedback

**Cons:**
- **Cheating trivial:** Client sends fake values
- Server has no validation (client says "I have 9999 stamina", server accepts)
- Unsuitable for PvP
- Violates server authority principle

---

#### Option 3: Deterministic Simulation (Shared Logic)

Both client and server run identical regeneration system independently:

```rust
// common/systems/resources.rs (shared code)
pub fn regenerate_resources(
    mut query: Query<(&mut Stamina, &mut Mana)>,
    time: Res<Time>,
) {
    for (mut stamina, mut mana) in &mut query {
        let now = time.elapsed();
        let dt = (now - stamina.last_update).as_secs_f32();

        stamina.state = (stamina.state + stamina.regen_rate * dt).min(stamina.max);
        stamina.last_update = now;

        let dt_mana = (now - mana.last_update).as_secs_f32();
        mana.state = (mana.state + mana.regen_rate * dt_mana).min(mana.max);
        mana.last_update = now;
    }
}

// Both client and server add this system to FixedUpdate
app.add_systems(FixedUpdate, regenerate_resources);
```

**Network cost:**
- Initial sync on spawn: 1,000 players × 1 message = **1,000 messages** (one-time)
- Continuous regeneration: **0 messages/sec**
- Periodic corrections (if drift > 5%): ~10 messages/sec

**Total sustained bandwidth: ~160 bytes/sec** (corrections only)

**Pros:**
- **Zero network traffic for regeneration** (massive win)
- Scales to unlimited players (regeneration cost is local CPU only)
- Server maintains authority (can send corrections)
- Client gets instant feedback (local simulation)

**Cons:**
- Potential desync (client and server drift over time)
- Requires identical logic on both sides (shared code)
- Needs periodic reconciliation

## Decision

**We will use Option 3: Deterministic Simulation with Shared Logic.**

### Core Mechanism

**Shared Code in `common/`:**
- Regeneration formula identical on client and server
- Both use `Time::elapsed()` Duration (synchronized via `Event::Init`)
- Both run in FixedUpdate at same tick rate (125ms)

**Network Synchronization:**
- **Initial sync:** Server sends resource values on spawn
- **Continuous sync:** None (both simulate independently)
- **Corrections:** Server sends update only if client-server diff > 5%
- **Discrete events:** Server sends updates on damage, ability use (discrete state changes)

**Determinism Guarantees:**
1. Same formula (shared code in `common/`)
2. Same tick rate (FixedUpdate at 125ms)
3. Same starting state (initial sync)
4. Same time tracking (`Time::elapsed()` synchronized)

**Result:** Client and server drift < 1% under normal conditions.

---

## Rationale

### 1. Network Efficiency

**Performance comparison:**

| Approach | Messages/sec | Bandwidth | Scalability |
|----------|--------------|-----------|-------------|
| Broadcast every tick | 16,000 | 256 KB/sec | ❌ ~100 players max |
| Client authority | 400 | 6.4 KB/sec | ⚠️ Enables cheating |
| Deterministic simulation | ~10 (corrections) | 160 bytes/sec | ✅ Scales to 10,000+ |

**Deterministic simulation achieves 1,600× reduction in network traffic.**

### 2. Regeneration is Predictable Math

Regeneration formula:
```
new_value = current_value + (regen_rate * time_delta)
```

**Properties:**
- No random factors (deterministic)
- No external inputs (self-contained)
- Simple arithmetic (cheap CPU)

**Perfect candidate for client-side simulation:**
- Client can predict exact outcome
- Server can validate without continuous sync
- Both calculate same result independently

### 3. Desync Mitigation

**Sources of potential desync:**
1. Tick timing differences (client 125.1ms, server 125.0ms)
2. Floating-point precision differences
3. Different optimization levels (debug vs release)

**Mitigations:**
1. **Shared code in `common/`:**
   - Identical formulas compiled for both client and server
   - No divergence in logic possible

2. **Fixed tick rate:**
   - Both use `FixedUpdate` at 125ms
   - Bevy's `Time::elapsed()` synchronized via `Event::Init`

3. **Periodic corrections:**
   - Server checks `client_value - server_value`
   - If diff > 5%, server sends correction
   - Client snaps to server value (authority)

4. **Tolerance threshold:**
   - Accept ±1% drift (±1.2 stamina for 120 max)
   - Invisible to player
   - Prevents unnecessary corrections

**Result:** Drift stays < 1% in practice, rare corrections maintain sync.

### 4. Server Authority Maintained

Even with client-side simulation, server remains authoritative:

**On discrete events (damage, ability use):**
- Client predicts resource change
- Server validates and sends confirmed value
- Client accepts server's value (may rollback prediction)

**On regeneration:**
- Client simulates locally (instant feedback)
- Server simulates independently (authority)
- Server sends correction if client drifts > 5%

**Cheating prevented:**
- Client cannot fake regeneration (server validates)
- Client predictions are local-only (not sent to server)
- Server always makes final decision

---

## Consequences

### Positive

**1. Massive Network Savings**

Regeneration bandwidth: **0 bytes/sec** (vs 256 KB/sec for broadcast)

**Scaling:**
- 100 players: 0 bytes/sec
- 1,000 players: 0 bytes/sec
- 10,000 players: 0 bytes/sec

**Regeneration cost scales with player count: NOT AT ALL** (each client simulates their own)

**2. Instant Client Feedback**

Client regeneration runs locally:
- No network latency (0ms)
- Smooth UI updates (every frame)
- Responsive player experience

**3. CPU Efficiency**

Regeneration is cheap math:
- `state + (regen_rate * dt)` per resource
- ~10 CPU cycles per resource
- 1,000 players × 2 resources = 20,000 cycles per tick
- **< 0.1ms CPU time** on both client and server

**4. Reuses Existing Patterns**

**Physics simulation:**
- Already runs in FixedUpdate
- Already uses `Time::elapsed()` for determinism
- Already synchronized between client and server

**Regeneration:**
- Same pattern applied to resources
- Proven approach (physics has no desync issues)

### Negative

**1. Potential Desync**

Client and server may drift over time:
- Different tick timing (±1ms variance)
- Floating-point rounding differences
- One side paused (client minimized, server lagging)

**Mitigation:**
- Periodic corrections (server checks diff > 5%)
- Tolerance threshold (±1% acceptable)
- Shared code prevents logic divergence

**2. Requires Shared Code**

Regeneration logic must live in `common/`:
- Both client and server compile same code
- Changes affect both sides
- Can't have client-specific optimizations

**Mitigation:**
- Shared code is already established pattern (physics)
- Regeneration is simple (no need for optimizations)
- Benefits outweigh constraints

**3. Complexity**

More complex than server-only:
- Two codepaths: local player (simulate) vs remote (display server state)
- Correction logic needed
- Testing requires both client and server

**Mitigation:**
- Complexity is one-time (foundation)
- Once established, future resources reuse same pattern
- Testing is easier (deterministic, reproducible)

### Neutral

**Correction Frequency:**

How often do corrections occur?

**Estimated:**
- Drift rate: ~0.01% per tick (empirical from physics)
- Threshold: 5% diff
- Correction frequency: ~1 per 500 ticks = **1 per minute**

**Network cost:** 1,000 players × 2 resources × (1/60 sec) = **33 messages/sec** = **528 bytes/sec**

**Still 485× better than broadcast approach** (256 KB/sec → 528 bytes/sec)

---

## Critical Implementation Details

### 1. Time Tracking with Duration

**DO NOT use `Instant::now()`:**
```rust
pub struct Stamina {
    pub last_update: Instant,  // ❌ WRONG - system clock differs client/server
}
```

**DO use `Time::elapsed()` Duration:**
```rust
pub struct Stamina {
    pub last_update: Duration,  // ✅ CORRECT - synchronized via Event::Init
}

fn regenerate_resources(
    time: Res<Time>,
) {
    let now = time.elapsed();  // Duration since game start
    let dt = now - stamina.last_update;
}
```

**Why:**
- `Instant::now()` uses system clock (different on client and server)
- `Time::elapsed()` synchronized via `Event::Init` on connection
- Both measure from same epoch

### 2. Schedule in FixedUpdate

Regeneration MUST run in `FixedUpdate`:
```rust
app.add_systems(FixedUpdate, regenerate_resources);
```

**Why:**
- Predictable tick rate (125ms, not variable frame rate)
- Matches physics schedule (proven determinism)
- Both client and server use same tick rate

### 3. Shared Code in `common/`

Regeneration logic MUST live in `common/systems/resources.rs`:

**Why:**
- Compiled identically for client and server
- Changes affect both sides simultaneously
- Impossible to have divergent logic

### 4. NO EventWriter in Regeneration System

**CRITICAL WARNING:**

```rust
// ❌ WRONG - Creates 16,000 messages/sec network spam
pub fn regenerate_resources(
    mut writer: EventWriter<Do>,  // ❌ NEVER ADD THIS
    mut query: Query<(&mut Stamina, &mut Mana)>,
) {
    for (mut stamina, mut mana) in &mut query {
        // Regenerate
        writer.write(Do { event: Event::Stamina { .. } });  // ❌ DISASTER
    }
}
```

**✅ CORRECT - No EventWriter:**
```rust
pub fn regenerate_resources(
    mut query: Query<(&mut Stamina, &mut Mana)>,
    time: Res<Time>,
) {
    for (mut stamina, mut mana) in &mut query {
        // Regenerate locally, NO network sync
    }
}
```

**Why:**
- Defeats entire purpose of deterministic simulation
- Would create network spam (16,000 msg/sec)
- Both sides already running same logic

---

## Alternatives Rejected

**Periodic broadcast (every 5 seconds):**
```rust
// Server sends updates every 5 seconds
if tick % 40 == 0 {  // 40 ticks = 5 seconds
    writer.write(Event::Stamina { .. });
}
```

**Rejected because:**
- Still 400 messages/sec (vs 0 for deterministic)
- Doesn't solve core problem (continuous state sync)
- Client-server desync visible (5-second snaps)
- Deterministic approach is strictly better

---

## Implementation Notes

See [SOW-002](../03-sow/002-combat-foundation.md) Phase 3 for implementation details.

**System location:** `common/systems/resources::regenerate_resources`

**Added to schedules:**
- Server: `app.add_systems(FixedUpdate, regenerate_resources)`
- Client: `app.add_systems(FixedUpdate, regenerate_resources)`

**Formulas:**
- Stamina: `state + (10.0 * dt)` clamped to max
- Mana: `state + (8.0 * dt)` clamped to max

---

## References

- **RFC-002:** Combat Foundation feature request
- **ADR-002:** Server-Authoritative Resource Management (authority model)
- **ADR-003:** Component-Based Resource Separation (component structure)
- **Existing Pattern:** Physics simulation in FixedUpdate with deterministic Time::elapsed()

---

## Date

2025-10-29
