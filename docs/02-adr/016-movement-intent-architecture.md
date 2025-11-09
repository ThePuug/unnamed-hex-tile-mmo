# ADR-016: Movement Intent Architecture - "Intent then Confirmation" Pattern

## Status

**Accepted** - 2025-11-05

## Context

**Related RFC:** [RFC-011: Movement Intent System](../01-rfc/011-movement-intent-system.md)

Remote entities (NPCs, other players) lag 175-300ms behind server reality. Current system only broadcasts `Event::Loc` after movement completes. Client has no way to predict remote entity movement, causing teleporting visuals and broken projectile targeting.

### Requirements

- Reduce perceived lag for remote entities (smooth movement)
- Enable projectiles to lead moving targets
- Maintain server authority (server final say)
- Scale to MMO (100+ entities, acceptable bandwidth)
- Reuse existing client prediction patterns

### Options Considered

**Option 1: "Intent then Confirmation" Pattern** ✅ **SELECTED**
- Broadcast MovementIntent when movement starts
- Client predicts using intent
- Server confirms with Loc
- Validate prediction, snap if desync

**Option 2: Broadcast Player Input**
- Send player KeyBits (like local player)
- Remote clients predict using Input
- ❌ Doesn't work for NPCs, reveals inputs (PvP privacy)

**Option 3: No Prediction (Status Quo)**
- Keep current Loc-only broadcasting
- ❌ Remote entities lag 175-300ms (unacceptable)

**Option 4: Send Movement Vector**
- Broadcast velocity instead of destination
- ❌ More bandwidth, overkill for hex-to-hex movement

## Decision

**Use "Intent then Confirmation" pattern: broadcast movement destinations before completion, enable client prediction, validate against server confirmations.**

### Core Mechanism

**Pattern Flow:**

```
1. Server: Entity starts moving → Broadcast MovementIntent { dest, duration, seq }
2. Client: Receive intent → Predict movement toward dest (interpolate)
3. Server: Entity arrives → Broadcast Loc { qrz }
4. Client: Receive Loc → Validate prediction (match = smooth, mismatch = snap)
```

**MovementIntent Event:**

```rust
pub enum Event {
    MovementIntent {
        ent: Entity,
        destination: Qrz,      // Where entity is going
        duration_ms: u16,      // How long it takes (movement speed)
        seq: u8,               // Sequence number (detect reorders)
    },
}
```

**Server Broadcasting:**

```rust
// Triggered when Offset.state indicates movement toward different tile
// Runs after physics updates offset, before Loc updates
pub fn broadcast_movement_intent(
    query: Query<(Entity, &Loc, &Offset, &MovementIntentState), Changed<Offset>>,
    mut writer: EventWriter<Do>,
) {
    for (ent, loc, offset, intent_state) in query.iter() {
        let dest_tile = calculate_destination_tile(loc, offset.state);

        if dest_tile != *loc.qrz() && dest_tile != intent_state.last_broadcast {
            writer.write(Do {
                event: Event::MovementIntent {
                    ent,
                    destination: dest_tile,
                    duration_ms: calculate_expected_duration(offset, attrs),
                    seq: intent_state.seq.wrapping_add(1),
                }
            });

            // Track sent intent (avoid re-sending)
            intent_state.last_broadcast = dest_tile;
            intent_state.seq = intent_state.seq.wrapping_add(1);
        }
    }
}
```

**Client Prediction:**

```rust
#[derive(Component)]
pub struct MovementPrediction {
    pub predicted_dest: Qrz,
    pub predicted_arrival: Duration,
    pub intent_seq: u8,
}

pub fn apply_movement_intent(
    mut query: Query<(&mut Offset, &Loc, Option<&mut MovementPrediction>)>,
    mut reader: EventReader<Do>,
    map: Res<Map>,
    time: Res<Time>,
) {
    for &Do { event: Event::MovementIntent { ent, destination, duration_ms, seq } } in reader.read() {
        let Ok((mut offset, loc, prediction)) = query.get_mut(ent) else { continue };

        // Start interpolating toward destination
        let dest_world = map.convert(destination);
        offset.step = dest_world - map.convert(*loc.qrz());
        offset.interp_duration = duration_ms as f32 / 1000.0;
        offset.interp_elapsed = 0.0;

        // Track prediction for validation
        commands.entity(ent).insert(MovementPrediction {
            predicted_dest: destination,
            predicted_arrival: time.elapsed() + Duration::from_millis(duration_ms as u64),
            intent_seq: seq,
        });
    }
}
```

**Validation:**

```rust
pub fn validate_movement_predictions(
    mut query: Query<(&mut Offset, &Loc, &MovementPrediction)>,
    mut reader: EventReader<Do>,
    map: Res<Map>,
) {
    for &Do { event: Event::Incremental { ent, component: Component::Loc(confirmed_loc) } } in reader.read() {
        let Ok((mut offset, loc, prediction)) = query.get_mut(ent) else { continue };

        if confirmed_loc == prediction.predicted_dest {
            // SUCCESS: Prediction accurate
            offset.state = map.convert(confirmed_loc) - map.convert(*loc.qrz());
            offset.step = offset.state;
            commands.entity(ent).remove::<MovementPrediction>();
        } else {
            // DESYNC: Prediction wrong (entity blocked, changed direction)
            warn!("Movement desync: predicted {:?}, confirmed {:?}",
                  prediction.predicted_dest, confirmed_loc);
            // Snap to confirmed position
            offset.state = Vec3::ZERO;
            offset.step = Vec3::ZERO;
            commands.entity(ent).remove::<MovementPrediction>();
        }
    }
}
```

---

## Rationale

### 1. "Intent then Confirmation" Mirrors Local Player Pattern

**Local Player (Proven):**
- Input (intent) → Physics (prediction) → Confirmation (Loc)
- Works well, feels responsive

**Remote Entities (New):**
- MovementIntent (intent) → Prediction (interpolation) → Confirmation (Loc)
- Same pattern, different source of intent

**Impact:** Reuses proven architecture, familiar implementation.

### 2. Reduces Perceived Lag by 70%

**Before (Loc Only):**
```
T=0ms:   Server: Entity starts moving
T=125ms: Server: Entity arrives, broadcast Loc
T=175ms: Client: Receive Loc (50ms latency)
T=175ms: Client: Start interpolating
Lag: 175ms minimum
```

**After (Intent then Confirmation):**
```
T=0ms:   Server: Entity starts moving, broadcast Intent
T=50ms:  Client: Receive Intent (50ms latency)
T=50ms:  Client: Start predicting
Lag: 50ms minimum
```

**Result:** 175ms → 50ms (70% reduction), movement feels smooth.

### 3. Enables Projectile Leading

**Problem:** Projectiles fire at stale positions (where entity was 175ms ago).

**Solution:** Projectiles aim at predicted position (where entity will be).

```rust
pub fn select_projectile_target_position(
    target: Entity,
    query: Query<(&Loc, &Offset, Option<&MovementPrediction>)>,
) -> Vec3 {
    let Ok((loc, offset, prediction)) = query.get(target) else { return Vec3::ZERO };

    if let Some(prediction) = prediction {
        // Target is moving - aim at predicted destination
        map.convert(prediction.predicted_dest)
    } else {
        // Target stationary - aim at current position
        map.convert(*loc.qrz()) + offset.step
    }
}
```

**Impact:** Projectiles can hit moving targets, ranged combat viable.

### 4. Bandwidth Acceptable for MMO Scale

**Per-Intent Cost:**
```
Entity ID: 8 bytes
Destination: 4 bytes (Qrz = i16, i16)
Duration: 2 bytes (u16)
Sequence: 1 byte (u8)
Total: 15 bytes per intent
```

**Typical Load:**
```
5-10 entities in range per player (relevance filtering: 30 hexes)
150 bytes/frame (10 entities × 15 bytes)
1.2 KB/sec at 8 FPS (125ms ticks)
9.6 Kbps per player
ACCEPTABLE for MMO
```

**Optimization:** Relevance filtering prevents broadcasting to distant players.

### 5. Validates Predictions (Maintains Authority)

**Server Still Authoritative:**
- Loc confirmation is ground truth
- Intent is a hint, not command
- Client snaps if prediction wrong
- Cheating prevention unaffected

**Desync Handling:**
- Match: Prediction accurate (smooth)
- Mismatch: Snap correction (visible but rare <5%)
- No intent: Fallback to normal interpolation (graceful)

---

## Consequences

### Positive

**1. Smooth Remote Entity Movement**
- No more teleporting between tiles
- Interpolation starts immediately (not after 175ms delay)
- Combat feels responsive for all entities

**2. Fair Projectile Targeting**
- Projectiles can hit moving targets (lead prediction)
- Ranged combat viable (not trivial to avoid)
- Dodging by changing direction works (skill-based)

**3. Reuses Existing Architecture**
- Offset component (state/step pattern from ADR-002)
- Interpolation system (interp_duration, interp_elapsed)
- Validation pattern (Loc confirmations)
- Sequence numbers (like Input.seq)

**4. Scalable Bandwidth**
- Relevance filtering (only nearby entities)
- Intent per tile transition (not every frame)
- ~10 Kbps per player (acceptable)

**5. Enables Advanced Features**
- Predictive AI (NPCs can lead moving players)
- Dodge mechanics (change direction to evade)
- Movement speed matters (Grace attribute creates advantage)

### Negative

**1. Increased Network Complexity**
- New message type (MovementIntent)
- Sequence number tracking (detect reorders)
- Prediction state management (MovementPrediction component)
- Validation logic (desync detection)

**Mitigation:** Reuse existing patterns (seq from Input, validation from Loc).

**2. Potential Desyncs**
- Intent says going to (5,5), entity blocked → arrives at (4,5)
- Visible snap correction (rare, <5% of movements)
- More frequent with obstacles, knockbacks

**Mitigation:** Validate all predictions, accept rare snaps as acceptable trade-off.

**3. Prediction Staleness**
- Intent arrives at T=50ms (latency)
- Entity already 40% through movement on server
- Client prediction starts late → still lags slightly

**Mitigation:** Still 70% better than no prediction (175ms → 50ms lag).

**4. Edge Cases**
- Packet loss (intent lost, only Loc arrives)
- Packet reordering (stale intent after Loc)
- Rapid direction changes (multiple intents in flight)
- Teleports (Lunge, dev console)

**Mitigation:**
- Sequence numbers detect reorders
- Validation catches all edge cases (Loc is truth)
- Fallback path exists (no intent = normal interpolation)

### Neutral

**1. Local Player Unaffected**
- Local player still uses Input prediction
- Server still sends intent for local player (other clients need it)
- Client skips self-intent processing

**2. Server Authority Unchanged**
- Server still authoritative for all positions
- Intent is hint, Loc is truth
- Cheating prevention unaffected

---

## Implementation Notes

**File Structure:**
```
src/server/systems/actor.rs       - Intent broadcasting
src/common/components/mod.rs      - MovementPrediction component
src/common/message.rs             - Event::MovementIntent
src/common/systems/world.rs       - Intent application, validation
```

**Integration Points:**
- Server: Broadcast intent after physics, before Loc updates
- Client: Apply intent, start predicting
- Validation: Check Loc confirmations against predictions
- Projectiles: Aim at predicted positions

**Network:**
- MovementIntent event (15 bytes per intent)
- Relevance filtered (30 hex radius)
- Sequence numbers (detect reorders like Input.seq)

---

## Validation Criteria

**Functional:**
- Remote entities move smoothly (no teleporting)
- Prediction accuracy >95% (desyncs rare)
- Projectiles can hit moving targets (leading works)
- Desyncs snap correctly (visible but acceptable)

**Performance:**
- Bandwidth: <10 Kbps per player
- CPU: <1ms per frame for prediction updates
- Memory: MovementPrediction cleared after validation (no leaks)

**UX:**
- Combat feels responsive (playtest feedback)
- Ranged combat viable (not trivial to avoid)
- Dodging works (change direction to evade)

---

## References

- **RFC-011:** Movement Intent System
- **ADR-002:** Combat Foundation (Offset component, state/step pattern)
- **ADR-015:** Projectile System (targeting integration)
- **Existing:** Client prediction (Input queue), Loc confirmations

## Date

2025-11-05
