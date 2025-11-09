# ADR-007: Timer Synchronization via Insertion Time

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-003: Reaction Queue System](../01-rfc/003-reaction-queue-system.md)

Each queued threat has a timer (0.5-1.5s). Client needs smooth countdown animations, server needs to know when expired. **How do we synchronize timers without network spam?**

### Scale Problem

100 players × 3 threats × 60 fps = 18,000 updates/sec = **216 KB/sec** (unacceptable)

### Options Considered

**Option 1: Server Broadcasts Timer Updates** (every FixedUpdate tick)
- Network: 100 players × 3 threats × 8 ticks/sec = 2,400 msg/sec = 38.4 KB/sec
- ✅ Always accurate
- ❌ Network spam doesn't scale
- ❌ Not smooth (125ms granularity, visible steps)

**Option 2: Client Calculates Independently** (no insertion time sent)
- Network: 0 bytes/sec for timers
- ✅ Zero traffic, smooth animations
- ❌ Desync inevitable (latency causes permanent mismatch)
- ❌ No correction mechanism

**Option 3: Client Calculates from Server's Insertion Time**
- Network: Threat insert only (~40 bytes), timer updates 0 bytes/sec
- ✅ Zero traffic for timers
- ✅ Deterministic (both use same formula)
- ✅ Smooth animations (every frame)
- ⚠️ Requires time synchronization

## Decision

**Client calculates from server's insertion time (Option 3).**

### Core Mechanism

**Server sends insertion time once:**
```rust
pub struct QueuedThreat {
    pub source: Entity,
    pub damage: f32,
    pub inserted_at: Duration,      // Server's Time::elapsed()
    pub timer_duration: Duration,   // Instinct-scaled
}
```

**Both calculate remaining identically:**
```rust
// Shared formula (common/)
fn calculate_remaining(threat: &QueuedThreat, now: Duration) -> Duration {
    threat.timer_duration.saturating_sub(now - threat.inserted_at)
}

fn is_expired(threat: &QueuedThreat, now: Duration) -> bool {
    now >= threat.inserted_at + threat.timer_duration
}
```

**Network synchronization:**
- Initial: Server sends `inserted_at` on threat insert
- Continuous: **None** (both calculate locally)
- Expiry: Server sends resolution event when expired

---

## Rationale

### 1. Time::elapsed() Synchronization

Bevy's `Time::elapsed()` synchronized via `Event::Init` on connection:
- Server sends baseline: `server_time: time.elapsed()`
- Client sets local time to match epoch
- Both measure from same starting point
- Drift < 1ms/sec (negligible)

**Enables deterministic calculations:**
- Server: `inserted_at = 5.000s`
- Client receives same `inserted_at = 5.000s`
- At `now = 5.500s`: both calculate `remaining = 0.500s`

### 2. Zero Network Traffic

**Bandwidth comparison:**
- Option 1 (broadcast): 38.4 KB/sec
- Option 3 (insertion time): **0 bytes/sec** for timers
- **100% reduction** in timer sync traffic

**Scales infinitely:** Network cost doesn't grow with player count.

### 3. Smooth Client Animations

- Client updates every frame (~16ms at 60fps)
- Server updates every 125ms (FixedUpdate)
- Client animation much smoother than server broadcast could provide

### 4. Determinism with Tolerance

**Acceptable variance:**
- ±125ms (FixedUpdate granularity)
- Visual timer reaches 0, damage applies within 1 tick
- Human reaction time ~200ms (125ms variance imperceptible)

**Server authoritative:**
- Client timer visual only (not gameplay-affecting)
- Server determines actual expiry
- Brief delay acceptable if client sees 0 before server resolves

---

## Implementation

### Threat Structure

```rust
pub struct QueuedThreat {
    pub inserted_at: Duration,      // When inserted (Time::elapsed())
    pub timer_duration: Duration,   // How long until expiry (Instinct-scaled)
    // ... other fields
}
```

**Both fields serialized** in network messages - client needs both to calculate remaining.

### Server: Insertion

```rust
// Create threat with server's time
let threat = QueuedThreat {
    inserted_at: time.elapsed(),  // Server's current time
    timer_duration: calculate_timer_duration(attrs),
    // ...
};
```

### Server: Expiry Check

```rust
// FixedUpdate (125ms ticks)
fn check_expired_threats(queue: &ReactionQueue, time: Res<Time>) {
    let now = time.elapsed();
    while let Some(threat) = queue.threats.front() {
        if now >= threat.inserted_at + threat.timer_duration {
            // Expired - remove and resolve
        } else {
            break;  // Not expired yet
        }
    }
}
```

### Client: Timer Visualization

```rust
// Update (every frame)
fn update_queue_ui(queue: &ReactionQueue, time: Res<Time>) {
    let now = time.elapsed();
    for threat in &queue.threats {
        let remaining = calculate_remaining(threat, now);
        let progress = (remaining.as_secs_f32() / threat.timer_duration.as_secs_f32()).clamp(0.0, 1.0);
        render_timer_ring(progress);
    }
}
```

---

## Consequences

### Positive

- **Zero network traffic:** Timer bandwidth = 0 bytes/sec
- **Infinite scalability:** Cost doesn't grow with players
- **Smooth animations:** 60fps client updates (vs 8fps server broadcast)
- **Deterministic expiry:** Shared formula prevents divergence
- **Latency compensation:** Client calculates from original insertion time even if message delayed

**Example:**
```
T=0.0s: Server inserts (inserted_at=0.0s, duration=1.0s)
T=0.1s: Client receives (100ms latency)
Client calculates: remaining = 1.0s - (0.1s - 0.0s) = 0.9s ✓ Correct
```

### Negative

- **Requires time sync:** Depends on `Time::elapsed()` synchronization via `Event::Init`
- **Latency visible:** Timer already started when client receives (shows 0.9s not 1.0s)
- **Expiry variance:** FixedUpdate granularity causes ±125ms timing variance
- **No correction:** Once sent, no timer updates (drift accumulates)

### Mitigations

- Time sync via init event (reliable TCP handshake)
- Latency shows truth (threat did start 100ms ago)
- ±125ms variance acceptable (human reaction time ~200ms)
- Drift minimal (< 1ms/sec, imperceptible)
- Server expiry authoritative (client timer cosmetic)

---

## References

- **RFC-003:** Reaction Queue System
- **ADR-006:** Server-Authoritative Reaction Queue
- **ADR-004:** Deterministic Resource Regeneration (same pattern)
- **Pattern:** Physics simulation (deterministic local calculations)

## Date

2025-10-29
