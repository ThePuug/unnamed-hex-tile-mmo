# RFC-011: Movement Intent System

## Status

**Approved** - 2025-11-05

## Feature Request

### Player Need

From player perspective: **Responsive, fair combat** - Remote entities should move smoothly, projectiles should be able to hit moving targets.

**Current Problem:**
Without movement intent system:
- Remote entities (NPCs, other players) lag 175-300ms behind server reality
- Entities appear to teleport between tiles (no smooth movement)
- Projectiles fire at "ghost positions" (where entity was 2 steps ago)
- Ranged combat feels broken (can trivialize by "just keep moving")
- Game feels laggy despite local player having responsive client prediction

**Technical Root Cause:**
- Server only broadcasts `Event::Loc` AFTER movement completes (125ms delay)
- Client has no way to predict remote entity movement
- Visual position lags server by: network latency (50-150ms) + movement duration (125ms) = 175-300ms
- Local player feels good (client-side prediction via Input queue)
- Remote entities feel bad (no prediction, only post-completion interpolation)

**We need a system that:**
- Reduces perceived lag for remote entities (smooth movement)
- Enables projectiles to lead moving targets (predictive targeting)
- Maintains server authority (server still has final say)
- Scales to MMO (100+ entities with acceptable bandwidth)
- Reuses existing client prediction architecture

### Desired Experience

Players should experience:
- **Smooth Movement:** Remote entities move smoothly (no teleporting)
- **Fair Combat:** Projectiles can hit moving targets (not ghost positions)
- **Responsive Feel:** Game feels responsive for all entities (not just local player)
- **Skill-Based:** Dodging projectiles by changing direction (skill expression)
- **Consistent:** Local and remote entities behave similarly

### Specification Requirements

**MVP Movement Intent:**

**1. Intent Broadcasting:**
- Server broadcasts `MovementIntent` when entity starts moving (before completion)
- Contains: destination (Qrz), duration (ms), sequence number
- Sent to players within relevance radius (30 hexes)

**2. Client Prediction:**
- Client receives intent, starts interpolating toward destination immediately
- Uses intent duration for movement speed (accounts for Grace attribute)
- Predicts smooth movement (no wait for completion)

**3. Validation:**
- Server broadcasts `Loc` confirmation when movement completes
- Client validates prediction against confirmation
- If match: prediction accurate (smooth)
- If mismatch: snap correction (desync)

**4. Projectile Integration:**
- Projectiles target predicted position (not current position)
- Enables hitting moving targets (no more ghost aiming)
- Dodging by changing direction works (skill-based)

**Timeline:**
```
T=0ms:   Server: NPC starts moving (0,0) → (1,0)
T=0ms:   Server: Broadcast MovementIntent { dest: (1,0), duration: 125ms }
T=50ms:  Client: Receive intent (network latency)
T=50ms:  Client: Start predicting movement toward (1,0)
T=125ms: Server: NPC arrives at (1,0)
T=125ms: Server: Broadcast Loc { qrz: (1,0) }
T=175ms: Client: Receive Loc confirmation
T=175ms: Client: Validate prediction (match = success)
```

**Result:** Client visual lags 50ms (network only), not 175ms (network + movement).

### MVP Scope

**Phase 1 includes:**
- MovementIntent event type
- Intent broadcasting (server, after physics, before Loc)
- Client prediction (apply intent, interpolate to destination)
- Validation (Loc confirmation checks prediction)
- Relevance filtering (only send to nearby players, 30 hex radius)
- Projectile targeting integration (aim at predicted position)

**Phase 1 excludes:**
- Intent batching (multiple intents per message - optimization)
- Intent timestamps (extrapolation from send time - Phase 2)
- Advanced leading (intercept calculation for projectile travel - Phase 2)
- Compression (delta encoding, Huffman - optimization)
- Velocity-based prediction (knockbacks, dashes - Phase 2)

### Priority Justification

**CRITICAL** - Blocks responsive combat, ranged combat currently broken.

**Why critical:**
- Ranged combat feels trivial (players can "just keep moving" to avoid all projectiles)
- Game feels laggy (remote entities teleport)
- Violates "conscious but decisive" design pillar (not responsive)
- Unfair (local player has prediction, remote entities don't)

**Benefits:**
- Smooth remote entity movement (no teleporting)
- Fair projectile targeting (can hit moving targets)
- Enables ranged builds (currently non-viable)
- Validates combat foundation (responsive feel)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: "Intent then Confirmation" Pattern**

#### Core Mechanism

**Pattern:**
1. Server broadcasts MovementIntent when movement starts (immediately)
2. Client predicts movement using intent (smooth, responsive)
3. Server broadcasts Loc when movement completes (validation)
4. Client validates prediction, snaps if desync

Mirrors existing local player pattern:
- Input (intent) → Physics (prediction) → Confirmation (Loc)

**MovementIntent Event:**
```rust
pub enum Event {
    MovementIntent {
        ent: Entity,
        destination: Qrz,      // Target tile
        duration_ms: u16,      // Travel time
        seq: u8,               // Sequence number
    },
}
```

**Client Prediction:**
- Receive intent → start interpolating toward destination
- Use intent duration for speed
- Store prediction for validation

**Validation:**
- Loc confirmation arrives
- Check if matches predicted destination
- Match: prediction accurate (smooth)
- Mismatch: snap correction (desync, rare)

#### Performance Projections

**Bandwidth:**
```
Per-Intent Cost: 15 bytes (Entity 8, Qrz 4, duration 2, seq 1)
Typical: 5-10 entities in range per player
Cost: 150 bytes/frame = 1.2 KB/sec = 9.6 Kbps per player
ACCEPTABLE for MMO scale
```

**Lag Reduction:**
```
Before: 175-300ms lag (network + movement)
After: 50-150ms lag (network only)
Improvement: 70% reduction in perceived lag
```

**Development Time:**
- Phase 1 (Core): 2-3 days
- Phase 2 (Relevance filtering): 1 day
- Phase 3 (Projectile integration): 1-2 days
- Phase 4 (Edge cases): 1-2 days
- Total: 5-8 days

#### Technical Risks

**1. Prediction Desyncs**
- *Risk:* Intent says going to (5,5), but entity blocked → arrives at (4,5)
- *Mitigation:* Validate all predictions, snap corrections smoothly
- *Impact:* Rare (<5% of movements), acceptable snaps

**2. Packet Loss**
- *Risk:* Intent lost, only Loc arrives → no prediction
- *Mitigation:* Fallback to normal interpolation, Loc still works
- *Impact:* Graceful degradation, no crashes

**3. Prediction Staleness**
- *Risk:* Intent arrives late (entity already 40% through movement)
- *Mitigation:* Still better than no prediction (175ms → 50ms lag)
- *Impact:* 70% improvement even with latency

**4. Bandwidth Scaling**
- *Risk:* 100+ entities moving = intent spam
- *Mitigation:* Relevance filtering (only nearby), intent per tile (not per frame)
- *Impact:* Scales linearly with local density

### System Integration

**Affected Systems:**
- Networking (new event type, broadcasting, validation)
- Movement (physics calculates destination, triggers intent)
- Rendering (client prediction, interpolation)
- Projectile targeting (aim at predicted position)

**Compatibility:**
- ✅ Reuses Offset component (state/step pattern from ADR-002)
- ✅ Reuses validation pattern (Loc confirmations)
- ✅ Extends interpolation system (existing interp_duration)
- ✅ Server-authoritative (Loc is ground truth)

### Alternatives Considered

#### Alternative 1: Broadcast Input (Players Only)

Broadcast player inputs (like local player), remote clients predict using Input.

**Rejected because:**
- Reveals player inputs (PvP privacy concern)
- Doesn't work for NPCs (no KeyBits)
- More bandwidth (Input every frame vs Intent per tile)
- Separate code path for NPCs (complexity)

#### Alternative 2: No Prediction (Status Quo)

Keep current system (only Loc after completion).

**Rejected because:**
- Remote entities lag 175-300ms (unacceptable)
- Projectiles can't hit moving targets (broken combat)
- Violates "conscious but decisive" design pillar

#### Alternative 3: Send Movement Vector

Broadcast velocity vector instead of destination.

**Rejected for MVP because:**
- More bandwidth (Vec3 vs Qrz)
- Overkill for hex-to-hex movement
- Defer to Phase 2 (knockbacks, dashes)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** "Intent then Confirmation" pattern enables client prediction for remote entities while maintaining server authority. Mirrors existing local player pattern (proven).

**Lag reduction critical:** 175-300ms lag → 50-150ms lag (70% improvement). Makes combat feel responsive, projectiles viable.

**Bandwidth acceptable:** ~10 Kbps per player with relevance filtering. Scales to MMO (100+ entities).

**Extensibility:**
- Future: Intent timestamps (extrapolate from send time)
- Future: Velocity-based prediction (knockbacks, dashes)
- Future: Curved path prediction (multi-tile waypoints)

### PLAYER Validation

**From combat-system.md spec:**

**Success Criteria:**
- ✅ "Conscious but Decisive" - Combat responsive and skill-based
- ✅ Projectile attacks dodgeable - Visual warning, travel time
- ✅ Positioning matters - Movement and facing core to combat

**Responsive Validation:**
- Remote entities move smoothly (no teleporting)
- Projectiles can hit moving targets (fair)
- Combat feels responsive (all entities, not just local)

**Skill Expression:**
- Dodging projectiles by changing direction (prediction invalidated)
- Leading targets with projectiles (predictive aiming)
- Positioning creates advantage (movement matters)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Clean pattern, reuses existing architecture, scalable bandwidth
- PLAYER: ✅ Solves lag issue, enables ranged combat, skill-based dodging

**Scope Constraint:** Fits in one SOW (5-8 days for MVP)

**Dependencies:**
- ADR-002: Offset component (state/step pattern, interpolation)
- ADR-015: Projectile system (targeting integration)
- Existing: Client prediction (Input queue), Loc confirmations

**Next Steps:**
1. ARCHITECT creates ADR-016 documenting "Intent then Confirmation" architecture
2. ARCHITECT creates SOW-011 with 4-phase implementation plan
3. DEVELOPER begins Phase 1 (core intent system)

**Date:** 2025-11-05
