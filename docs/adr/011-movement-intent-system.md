# ADR-011: Movement Intent System - Predictive Remote Entity Rendering

## Status

Proposed (2025-11-05)

## Context

### Current System State

Based on playtest feedback and accepted ADRs:

1. **ADR-002 through ADR-010** - Combat foundation, abilities, targeting, projectiles all implemented
2. **Client-Side Prediction** - Works well for local player (Input queue, offset.step prediction)
3. **Remote Entity Rendering** - NPCs and remote players use interpolation after Loc updates
4. **Projectile System** - Fires at target's current known position

### Problem: Ranged Combat Feels Unresponsive and Trivial

**Root Cause:** Server only broadcasts `Event::Loc` updates AFTER movement completes.

**Symptom Timeline:**
```
T=0ms:   NPC starts moving from (0,0) → (1,0)
T=125ms: NPC arrives at (1,0)
T=125ms: Server sends Event::Loc { qrz: (1,0) }
T=175ms: Client receives Loc update (50ms latency)
T=175ms: Client starts interpolating from (0,0) to (1,0)
T=300ms: Client finishes interpolation, NPC visually at (1,0)
T=300ms: NPC already at (2,0) on server (started moving at T=125ms)
```

**Player Experience:**
- Client's visual representation lags **175ms+ behind** server reality
- Projectiles fire at "ghost positions" (where NPC was 2 steps ago)
- Remote entities appear to **teleport** between tiles (no smooth movement)
- Players can **trivialize ranged enemies** by "just keep moving" (AI fires at stale positions)
- Game feels **laggy** despite local player having responsive client-side prediction

**Why Local Player Feels Good:**
- Local player sends `Event::Input` immediately (no wait for server)
- Client predicts movement using same physics as server
- Offset.step updates instantly, rendering is smooth
- Server confirmations only correct minor desyncs

**Why Remote Entities Feel Bad:**
- Remote entities only update on `Event::Loc` (after movement complete)
- Client has no prediction (just interpolation after-the-fact)
- Visual position always lags server by latency + movement duration
- Projectile targeting uses stale visual position (misses moving targets)

### Game Design Impact

From `docs/spec/combat-system.md`:
- **"Conscious but Decisive"** - Combat should feel responsive and skill-based
- **Projectile Attacks** (Lines 151-161) - Should be dodgeable, provide visual warning
- **Positioning Matters** - Movement and facing are core to combat skill

**Current State Violates Design Pillars:**
1. **Not Responsive** - Remote entities lag server by 175-300ms
2. **Not Skill-Based** - Can't lead targets (always hit ghost positions)
3. **Not Fair** - Local player has prediction advantage, remote entities don't
4. **Not Fun** - Ranged combat feels broken, melee is the only viable strategy

### Technical Constraints

**Network Reality:**
- 50ms minimum latency (LAN)
- 100-150ms typical latency (internet)
- 125ms movement duration (FixedUpdate tick rate)
- Cannot eliminate latency (physics of distance)

**Existing Architecture:**
- Client-side prediction works for local player (proven pattern)
- Offset component has `state` (authority) and `step` (predicted) fields
- Interpolation system exists (`offset.interp_elapsed`, `offset.interp_duration`)
- Server has authoritative movement (physics runs on server)

**Goals:**
1. **Reduce perceived lag** - Make remote entities move smoothly
2. **Enable predictive targeting** - Projectiles can lead moving targets
3. **Maintain authority** - Server still has final say on positions
4. **Minimize bandwidth** - MMO scale (100+ entities) must be viable
5. **Reuse existing patterns** - Leverage client prediction architecture

## Decision

We will implement a **Movement Intent System** that broadcasts entity movement destinations **before movement completes**, enabling client-side prediction for remote entities.

### Core Architectural Principle

**"Intent then Confirmation" Pattern:**
1. Server broadcasts **MovementIntent** when entity starts moving (immediately)
2. Client predicts movement using intent (smooth, responsive)
3. Server broadcasts **Loc** when movement completes (validation)
4. Client validates prediction, snaps if desync detected

This mirrors the existing local player pattern:
- **Input** (intent) → **Physics** (prediction) → **Confirmation** (Loc update)

### Design Decisions

#### Decision 1: Movement Intent Message Format

**New Event Type:**
```rust
pub enum Event {
    // Existing events...

    /// Server → Client: Entity intends to move to destination
    /// Sent when movement starts (before completion)
    MovementIntent {
        ent: Entity,
        destination: Qrz,      // Target tile
        duration_ms: u16,      // Expected travel time (for speed scaling)
        seq: u8,               // Sequence number (detect duplicates/reorders)
    },
}
```

**Rationale:**
- **destination**: Where entity is going (client can start interpolating immediately)
- **duration_ms**: How long movement takes (accounts for movement speed attribute)
- **seq**: Detect packet reordering/loss (same pattern as Input.seq)

**Alternative Considered: Send KeyBits for Players**
- Pro: Reuse Input system exactly
- Con: Doesn't work for NPCs (no KeyBits)
- Con: Reveals player inputs (privacy concern for PvP)
- Con: More complex (need separate path for NPCs vs players)
- **Rejected**: Unified approach (destination) works for all entities

**Alternative Considered: Send Movement Vector**
- Pro: Works for curves, knockbacks, complex paths
- Con: More bandwidth (Vec3 vs Qrz)
- Con: Overkill for MVP (all movement is hex-to-hex)
- **Deferred**: Add if needed for future features (dashes, knockbacks)

#### Decision 2: When to Send Intent

**Server Flow:**
```rust
// In controlled::apply (after physics updates offset.state)
pub fn broadcast_movement_intent(
    query: Query<(Entity, &Loc, &Offset, &MovementIntentState), Changed<Offset>>,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
) {
    for (ent, loc, offset, intent_state) in query.iter() {
        // Detect tile crossing: offset.state moving toward different tile
        let current_tile = *loc.qrz();
        let dest_tile = calculate_destination_tile(loc, offset.state);

        if dest_tile != current_tile && dest_tile != intent_state.last_broadcast {
            // Entity started moving to new tile - broadcast intent
            let duration_ms = calculate_expected_duration(offset.state, attrs.movement_speed());

            writer.write(Do {
                event: Event::MovementIntent {
                    ent,
                    destination: dest_tile,
                    duration_ms,
                    seq: intent_state.seq.wrapping_add(1),
                }
            });

            // Track that we sent this intent (don't re-send until next movement)
            intent_state.last_broadcast = dest_tile;
            intent_state.seq = intent_state.seq.wrapping_add(1);
        }
    }
}
```

**Timing:**
- **Trigger**: When `Offset.state` indicates movement toward different tile
- **Frequency**: Once per tile transition (not every frame)
- **Schedule**: After physics updates offset, before Loc updates

**Why After Physics:**
- Physics calculates movement direction and speed
- Need offset.state to determine destination
- Ensures intent matches actual movement

**Why Before Loc Update:**
- Intent arrives before Loc confirmation (gives client time to predict)
- Maintains "intent then confirmation" ordering

#### Decision 3: Client Prediction Using Intent

**New Component:**
```rust
#[derive(Component)]
pub struct MovementPrediction {
    pub predicted_dest: Qrz,        // Where we think entity is going
    pub predicted_arrival: Duration, // When we expect arrival (Time::elapsed() + duration)
    pub intent_seq: u8,              // Sequence number of intent we're predicting
    pub prediction_start: Duration,  // When we started predicting (Time::elapsed())
}
```

**Client Flow:**
```rust
pub fn apply_movement_intent(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    mut query: Query<(&mut Offset, &Loc, Option<&mut MovementPrediction>)>,
    map: Res<Map>,
    time: Res<Time>,
    buffers: Res<InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::MovementIntent { ent, destination, duration_ms, seq } } = message
            else { continue };

        // Skip intent for local player (we predict using Input, not Intent)
        if buffers.get(&ent).is_some() { continue; }

        let Ok((mut offset, loc, o_prediction)) = query.get_mut(ent) else { continue };

        // Calculate target position (destination tile center + heading offset)
        let dest_world = map.convert(destination);
        // TODO: Add heading-based offset for accurate positioning

        // Calculate current visual position (mid-interpolation if applicable)
        let current_visual = calculate_current_visual_position(&offset);

        // Set up interpolation: current visual → predicted destination
        offset.prev_step = current_visual;
        offset.step = dest_world - map.convert(*loc.qrz()); // Offset from current tile
        offset.state = offset.step; // Authority will be corrected by Loc confirmation

        // Set interpolation duration based on intent
        offset.interp_duration = duration_ms as f32 / 1000.0;
        offset.interp_elapsed = 0.0;

        // Track prediction state (for validation against Loc confirmation)
        if let Some(mut prediction) = o_prediction {
            prediction.predicted_dest = destination;
            prediction.predicted_arrival = time.elapsed() + Duration::from_millis(duration_ms as u64);
            prediction.intent_seq = seq;
            prediction.prediction_start = time.elapsed();
        } else {
            commands.entity(ent).insert(MovementPrediction {
                predicted_dest: destination,
                predicted_arrival: time.elapsed() + Duration::from_millis(duration_ms as u64),
                intent_seq: seq,
                prediction_start: time.elapsed(),
            });
        }
    }
}
```

**Key Points:**
- **Immediate Interpolation**: Start moving toward destination as soon as intent arrives
- **Visual Continuity**: Use current visual position as interpolation start (no snap)
- **Duration-Based**: Use intent duration for speed (accounts for Grace attribute)
- **Prediction Tracking**: Store prediction for later validation

#### Decision 4: Validating Intent with Loc Confirmations

**Validation Flow:**
```rust
pub fn validate_movement_predictions(
    mut query: Query<(&mut Offset, &Loc, &mut MovementPrediction)>,
    mut reader: EventReader<Do>,
    map: Res<Map>,
    time: Res<Time>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::Loc(confirmed_loc) } } = message
            else { continue };

        let Ok((mut offset, loc, mut prediction)) = query.get_mut(ent) else { continue };

        // Check if Loc confirmation matches our prediction
        if confirmed_loc == prediction.predicted_dest {
            // SUCCESS: Prediction was accurate
            // Snap to exact confirmed position (minor correction)
            offset.state = map.convert(confirmed_loc) - map.convert(*loc.qrz());
            offset.step = offset.state; // Interpolation complete

            // Clear prediction (no longer needed)
            commands.entity(ent).remove::<MovementPrediction>();
        } else {
            // DESYNC: Prediction was wrong (entity changed direction, got blocked, etc.)
            warn!("Movement prediction desync: predicted {:?}, confirmed {:?}",
                  prediction.predicted_dest, confirmed_loc);

            // Snap to confirmed position immediately (visible correction)
            offset.state = Vec3::ZERO;
            offset.step = Vec3::ZERO;
            offset.prev_step = Vec3::ZERO;
            // Loc update will handle positioning via normal interpolation

            // Clear prediction
            commands.entity(ent).remove::<MovementPrediction>();
        }
    }
}
```

**Desync Scenarios:**
- Entity changed direction mid-movement (player changed input)
- Entity was blocked by obstacle (physics collision)
- Entity was knocked back (ability interrupted movement)
- Packet loss (intent lost, Loc arrives without prior intent)

**Handling Desyncs:**
- **Minor desync** (< 1 hex): Snap smoothly via interpolation
- **Major desync** (> 1 hex): Hard snap to confirmed position (visible but rare)
- **No prediction**: If no intent arrived, Loc update works as normal (fallback path)

#### Decision 5: Bandwidth Optimization

**Problem:** Broadcasting intent for every entity every movement = expensive.

**Optimizations:**

**1. Relevance Filtering (Server-Side)**
```rust
pub fn filter_movement_intent_recipients(
    intent_entity: Entity,
    intent_loc: Qrz,
    all_players: &Query<(Entity, &Loc)>,
) -> Vec<Entity> {
    all_players.iter()
        .filter(|(player_ent, player_loc)| {
            // Only send intent if entity is within player's relevance radius
            let distance = intent_loc.distance_to(player_loc.qrz());
            distance <= RELEVANCE_RADIUS // 30 hexes (larger than FOV)
        })
        .map(|(ent, _)| ent)
        .collect()
}
```

**Rationale:**
- Players only care about nearby entities
- Matches existing chunk discovery pattern (relevance-based)
- Reduces bandwidth linearly with player density

**2. Intent Batching**
```rust
// Instead of individual messages, batch multiple intents per frame
pub struct BatchedMovementIntents {
    pub intents: Vec<(Entity, Qrz, u16, u8)>, // (ent, dest, duration, seq)
}
```

**Rationale:**
- Reduce protocol overhead (1 message header instead of N)
- Typical frame has 10-20 entities moving (batch = 200-400 bytes)
- Easier to implement compression (repeated Qrz patterns)

**3. Intent De-duplication**
```rust
pub struct MovementIntentState {
    pub last_broadcast: Qrz,   // Last destination we sent
    pub seq: u8,                // Current sequence number
}
```

**Rationale:**
- Don't re-send intent if entity hasn't moved to new tile
- Prevents intent spam if entity oscillates on tile boundary
- Tracked per-entity on server

**Bandwidth Analysis:**
```
Per-Intent Cost:
- Entity ID: 8 bytes (u64)
- Destination: 4 bytes (Qrz = i16, i16)
- Duration: 2 bytes (u16)
- Sequence: 1 byte (u8)
Total: 15 bytes per intent

Scenario: 100 players, 50 NPCs, 50% moving per frame (75 entities)
- Without batching: 75 * 15 = 1,125 bytes/frame
- With batching: ~1,200 bytes/frame (small overhead from batch header)
- At 8 FPS (125ms ticks): 9,600 bytes/sec = 9.6 KB/sec = 76.8 Kbps

Per-player bandwidth (with relevance filtering to 30 hexes):
- Typical: 5-10 entities in range
- Cost: 150 bytes/frame = 1.2 KB/sec = 9.6 Kbps
- ACCEPTABLE for MMO scale
```

#### Decision 6: NPC vs Player Intent

**Question:** Should NPCs and Players use the same intent system?

**Chosen: YES - Unified Intent System**

**Rationale:**
- Both NPCs and players move via physics (same underlying system)
- Client doesn't care who the entity is (rendering is identical)
- Reduces code duplication (one prediction path)
- Simplifies testing (uniform behavior)

**Alternative Considered: Players use Input broadcasting**
- Pro: Reuses Input queue infrastructure
- Pro: More accurate prediction (client knows all inputs)
- Con: Reveals player inputs (PvP privacy concern)
- Con: Separate code path for NPCs (complexity)
- Con: More bandwidth (Input every frame vs Intent per tile transition)
- **Rejected**: Unified approach is simpler and more private

**Special Case: Local Player**
- Local player skips intent processing (already has prediction via Input)
- Server still sends intent (other clients need it for remote player prediction)
- `if buffers.get(&ent).is_some() { continue; }` skips self-intent

#### Decision 7: Projectile Targeting Integration

**Problem:** Projectiles need to lead moving targets.

**Solution: Target Predicted Position**
```rust
pub fn select_projectile_target_position(
    target_ent: Entity,
    query: Query<(&Loc, &Offset, Option<&MovementPrediction>)>,
    map: Res<Map>,
) -> Vec3 {
    let Ok((loc, offset, o_prediction)) = query.get(target_ent) else {
        return Vec3::ZERO; // Invalid target
    };

    if let Some(prediction) = o_prediction {
        // Target is moving - aim at predicted destination
        let predicted_world = map.convert(prediction.predicted_dest);
        // TODO: Add leading calculation (account for projectile travel time)
        predicted_world
    } else {
        // Target is stationary - aim at current position
        map.convert(*loc.qrz()) + offset.step
    }
}
```

**Advanced: Projectile Leading**
```rust
// Calculate intercept point for moving target
pub fn calculate_intercept(
    shooter_pos: Vec3,
    target_pos: Vec3,
    target_velocity: Vec3,
    projectile_speed: f32,
) -> Vec3 {
    // Solve quadratic: |target_pos + t * target_velocity - shooter_pos| = projectile_speed * t
    // Returns intercept point where projectile will meet target
    // (Standard motion prediction algorithm - see "predictive aim" references)
}
```

**Rationale:**
- Projectiles feel fair (can actually hit moving targets)
- Rewards positioning skill (moving makes you predictable)
- Maintains "conscious but decisive" combat (not twitch-based)

**Edge Case: Prediction Desync During Flight**
- Projectile launched at predicted position
- Target changes direction (prediction invalidated)
- Projectile misses (dodged via movement change)
- **Intended behavior**: Dodging by changing direction is skill-based

## Consequences

### Positive

#### 1. Responsive Combat

- Remote entities move smoothly (no teleporting)
- Projectiles can lead targets (hit moving entities)
- Combat feels fair (local and remote entities both predicted)
- Matches design pillar: "Conscious but Decisive"

#### 2. Reuses Existing Architecture

- Intent pattern mirrors Input pattern (proven)
- Offset interpolation already exists (just needs better data)
- Validation pattern matches Input confirmations (familiar)
- No major refactor required (additive changes)

#### 3. Scalable Bandwidth

- Relevance filtering prevents intent spam (only nearby entities)
- Intent per tile transition (not every frame)
- Batching reduces protocol overhead
- ~10 Kbps per player (acceptable for MMO)

#### 4. Enables Advanced Features

- Predictive AI (NPCs can lead moving players)
- Dodge mechanics (change direction to evade projectiles)
- Movement speed matters (Grace attribute creates real advantage)
- PvP combat viable (fair targeting for all players)

### Negative

#### 1. Increased Network Complexity

- New message type (MovementIntent)
- Sequence number tracking (detect reorders/duplicates)
- Prediction state management (per remote entity)
- Validation logic (detect desyncs)

**Mitigation:**
- Use existing patterns (seq from Input, validation from Loc)
- Start simple (no batching in MVP, add if needed)
- Thorough testing (latency simulation, packet loss)

#### 2. Potential Desyncs

- Intent says "going to (5,5)" but entity blocked → arrives at (4,5)
- Client predicts (5,5), server confirms (4,5) → visible snap
- More frequent with obstacles, knockbacks, input changes

**Mitigation:**
- Validate all intents with Loc confirmations
- Snap corrections smoothly when desync small
- Accept visible snaps for major desyncs (rare, acceptable)
- Log desyncs for analysis (tune prediction confidence)

#### 3. Prediction Staleness

- Intent arrives at T=50ms (latency)
- Entity already 40% through movement on server
- Client prediction starts late → still lags slightly

**Mitigation:**
- Better than no prediction (175ms lag → 50ms lag = 70% improvement)
- Duration_ms accounts for remaining travel time
- Future: Send timestamp, client extrapolates from send time

#### 4. Edge Cases Require Handling

- **Packet loss**: Intent lost, only Loc arrives → fall back to normal interpolation
- **Packet reordering**: Stale intent arrives after Loc → ignore based on seq
- **Rapid direction changes**: Multiple intents in flight → use latest seq
- **Teleports**: Lunge, dev console → send Offset=ZERO, skip intent

**Mitigation:**
- Sequence numbers detect reorders (same as Input system)
- Validation catches all edge cases (Loc is ground truth)
- Fallback path exists (no intent = normal interpolation)

### Neutral

#### 1. Local Player Unaffected

- Local player still uses Input prediction (no change)
- Intent system only for remote entities
- Server still sends intent for local player (other clients need it)

#### 2. No Backward Compatibility

- Requires protocol version bump (new message type)
- Clients without intent support will see old behavior (teleporting)
- Server can detect client version, skip intent for old clients

#### 3. Server Authority Unchanged

- Server still authoritative for all positions
- Intent is a hint, Loc is truth
- Cheating prevention unaffected (server validates all movement)

## Implementation Phases

### Phase 1: Core Intent System (Foundation)

**Goal:** Broadcast intents, apply client prediction, validate with Loc

**Tasks:**
1. Add `Event::MovementIntent` to message.rs
2. Create `MovementPrediction` component
3. Create `MovementIntentState` component (server-side)
4. Implement server-side intent broadcasting (after physics, before Loc)
5. Implement client-side intent application (predict movement)
6. Implement validation (Loc confirmation checks prediction)
7. Add unit tests (intent → prediction → validation)

**Success Criteria:**
- Remote players move smoothly (no teleporting between tiles)
- Client and server agree on final positions (validation passes)
- Desyncs logged and snapped correctly

**Duration:** 2-3 days

---

### Phase 2: Relevance Filtering (Bandwidth Optimization)

**Goal:** Only send intents to players within relevance radius

**Tasks:**
1. Implement relevance filtering (30 hex radius)
2. Add per-player recipient lists (who gets which intents)
3. Test bandwidth with 100 entities (measure actual costs)
4. Add metrics (intents sent per frame, per player)

**Success Criteria:**
- Players only receive intents for nearby entities
- Bandwidth scales linearly with local entity density
- No visible difference in prediction quality

**Duration:** 1 day

---

### Phase 3: Projectile Targeting Integration

**Goal:** Projectiles fire at predicted positions

**Tasks:**
1. Modify projectile targeting to check `MovementPrediction`
2. Aim at predicted destination if moving
3. Add leading calculation (intercept point for travel time)
4. Test ranged combat (Forest Sprite, player projectiles)

**Success Criteria:**
- Projectiles hit moving targets reliably
- Dodging by changing direction works (skill-based)
- Ranged combat feels fair and responsive

**Duration:** 1-2 days

---

### Phase 4: Polish and Edge Cases

**Goal:** Handle packet loss, reordering, rapid changes

**Tasks:**
1. Sequence number validation (ignore stale intents)
2. Packet loss handling (Loc without prior intent)
3. Rapid direction changes (multiple intents in flight)
4. Teleport detection (Lunge, dev console → skip intent prediction)
5. Add visual debugging (show predicted path, desyncs)

**Success Criteria:**
- No crashes from edge cases
- Desyncs rare and minor
- Visual debugging aids iteration

**Duration:** 1-2 days

---

### Phase 5: Batching (Optional Performance)

**Goal:** Batch multiple intents per message

**Tasks:**
1. Create `BatchedMovementIntents` message type
2. Collect intents per frame, send as batch
3. Measure bandwidth improvement
4. Profile CPU impact (batch vs individual)

**Success Criteria:**
- Bandwidth reduced by 20-30% (protocol overhead savings)
- No increase in latency (batching within frame)

**Duration:** 1 day (optional, only if bandwidth issues)

---

## Validation Criteria

### Functional Tests

**1. Smooth Movement**
- Spawn remote player, watch them move 10 tiles
- **Expected**: Smooth interpolation, no visible teleports
- **Pass Criteria**: Visual position updates every frame

**2. Prediction Accuracy**
- Measure desync rate over 1000 movements
- **Expected**: >95% accuracy (intent matches Loc confirmation)
- **Pass Criteria**: <5% desyncs, all minor (<0.5 hex error)

**3. Projectile Targeting**
- Fire 20 projectiles at moving target (straight line movement)
- **Expected**: 80%+ hit rate
- **Pass Criteria**: Projectiles lead target, hit predicted position

**4. Edge Case Handling**
- Simulate packet loss (drop 10% of intents)
- **Expected**: Fallback to normal interpolation, no crashes
- **Pass Criteria**: Smooth degradation, Loc confirmations still work

### Performance Tests

**1. Bandwidth**
- 100 players, 50 NPCs, 50% moving
- **Expected**: <10 KB/sec per player
- **Pass Criteria**: Measure actual bandwidth, verify within budget

**2. CPU**
- 100 entities with active predictions
- **Expected**: <1ms per frame for prediction updates
- **Pass Criteria**: Profile prediction systems, no frame drops

**3. Memory**
- Track `MovementPrediction` component count over time
- **Expected**: Cleared after validation, no leaks
- **Pass Criteria**: Memory stable over 10 minute session

### UX Tests

**1. Responsiveness**
- Player observes remote player moving
- **Expected**: Feels smooth, not laggy
- **Pass Criteria**: Playtest feedback, no "teleporting" complaints

**2. Combat Fairness**
- Ranged combat against moving target
- **Expected**: Projectiles can hit, dodging works
- **Pass Criteria**: Playtest feedback, combat feels fair

**3. Desync Visibility**
- Force desyncs (server blocks movement)
- **Expected**: Snap correction noticeable but not jarring
- **Pass Criteria**: Playtest tolerance for rare snaps

## Open Questions

### Design Questions

**1. Should we send intents for stationary entities?**
- MVP: No (only send when movement starts)
- Future: Consider heartbeat intent (confirm still stationary)
- Decision: Test desync rate, add heartbeat if needed

**2. What relevance radius should we use?**
- MVP: 30 hexes (larger than FOV = 25 hexes)
- Concern: Entities appear at edge already moving (no startup interpolation)
- Decision: Tune based on playtest (may increase to 40 hexes)

**3. How much leading should projectiles do?**
- MVP: Aim at predicted destination (simple)
- Advanced: Calculate intercept point (travel time + target velocity)
- Decision: Start simple, add intercept if needed

**4. Should we validate intent arrival timing?**
- Concern: If intent arrives after Loc, client never predicts
- Solution: Server could timestamp intents, client extrapolates
- Decision: Defer to Phase 4, measure late arrival rate first

### Technical Questions

**1. How do we handle chained movements?**
- Scenario: Entity moves A→B→C, intents for both in flight
- Current: Latest seq wins, old intent ignored
- Concern: Might skip B entirely (A→C directly)
- Decision: Acceptable for MVP, monitor desync rate

**2. Should we compress intent batches?**
- Potential: Delta encoding (relative to previous position)
- Potential: Huffman coding (common Qrz patterns)
- Bandwidth savings: 20-30%
- Complexity: High
- Decision: Defer, only if bandwidth issues in testing

**3. How do we debug prediction issues?**
- Server logs: Track desync rate per entity
- Client visualization: Show predicted path vs actual path
- Network capture: Record intent/Loc pairs for analysis
- Decision: Add debug visualizations in Phase 4

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

**1. Intent Timestamps**
- Server sends timestamp with intent (when movement started)
- Client extrapolates from send time (reduce lag)
- Requires clock sync (already exists via Event::Init)

**2. Curved Path Prediction**
- Intent includes waypoints (multi-tile path)
- Client interpolates along curve (smoother for fleeing AI)
- Higher bandwidth (multiple Qrz per intent)

**3. Velocity-Based Prediction**
- Intent includes velocity vector (for knockbacks, dashes)
- Client simulates physics (more accurate for complex movement)
- Requires shared physics (already exists)

**4. Confidence Scores**
- Server sends confidence (0-100%) with intent
- Client uses lower confidence = slower interpolation (hedge bets)
- Adaptive prediction based on recent desync rate

**5. Client-Side Obstacle Awareness**
- Client predicts movement but checks local terrain
- If obstacle detected, don't predict through wall
- Reduces desyncs from blocked movement

### Optimization

**1. Intent Compression**
- Delta encoding (Qrz relative to current position)
- Run-length encoding (repeated movements)
- Custom binary format (smaller than MessagePack)

**2. Adaptive Relevance**
- Increase radius for fast-moving entities (visible sooner)
- Decrease radius for stationary entities (less spam)
- Dynamic per-entity tuning

**3. Priority-Based Sending**
- High priority: Intents for entities player is targeting
- Low priority: Intents for distant entities
- Drop low-priority intents under bandwidth pressure

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (responsive, skill-based combat)
- **Attribute System:** `docs/spec/attribute-system.md` (movement speed from Grace)

### Codebase

- **Offset Component:** `src/common/components/offset.rs` - state/step pattern
- **Client Prediction:** `src/common/systems/behaviour/controlled.rs` - Input queue prediction
- **Loc Updates:** `src/common/systems/world.rs::do_incremental` - existing interpolation
- **Network Events:** `src/common/message.rs` - Do/Try pattern
- **Projectile Targeting:** `src/common/systems/combat/abilities/` - targeting logic

### Related ADRs

- **ADR-002:** Combat Foundation (resources, prediction pattern)
- **ADR-004:** Ability System and Targeting (directional targeting)
- **ADR-006:** AI Behavior (NPC movement patterns)
- **ADR-010:** Combat Variety Phase 1 (projectiles, ranged enemies)

### External References

- **Predictive Aim Algorithms:** [Intercept point calculation](https://en.wikipedia.org/wiki/Proportional_navigation)
- **Client-Side Prediction:** [Valve's Source Engine Prediction](https://developer.valvesoftware.com/wiki/Source_Multiplayer_Networking)
- **Dead Reckoning:** [Networked Physics Prediction](https://en.wikipedia.org/wiki/Dead_reckoning)

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md` (responsive combat)
- Playtest feedback: Ranged combat feels broken (ghost targeting, teleporting NPCs)
- Existing codebase patterns: client-side prediction (Input queue), offset interpolation

## Date

2025-11-05

---

## Notes for Implementation

### Integration Points

**Server Systems (src/server/):**
1. `server/systems/actor.rs` - Add intent broadcasting after movement physics
2. `server/systems/renet.rs` - Add relevance filtering, track recipient lists

**Common Systems (src/common/):**
1. `common/components/mod.rs` - Add MovementPrediction component
2. `common/message.rs` - Add Event::MovementIntent
3. `common/systems/world.rs` - Add intent application, validation

**Client Systems (src/client/):**
1. `client/systems/renet.rs` - Process incoming intents
2. `client/systems/targeting.rs` - Use predictions for projectile targeting
3. `client/systems/debug.rs` - Visualize predicted paths (Phase 4)

### Testing Strategy

**Unit Tests:**
- Intent broadcasting logic (trigger conditions, sequence numbers)
- Prediction application (offset calculations, interpolation setup)
- Validation logic (desync detection, snap corrections)

**Integration Tests:**
- Full intent → prediction → validation cycle
- Multiple intents in flight (rapid direction changes)
- Packet loss scenarios (missing intents, missing Locs)

**Playtest Scenarios:**
- Ranged combat against moving Wild Dog (projectile leading)
- PvP dueling (remote player movement smoothness)
- High-latency simulation (100-200ms, verify still playable)

### Rollout Plan

**Phase 1 (Core):**
- Feature flag: `enable_movement_intent = true/false`
- Default: Disabled (opt-in testing)
- Metrics: Log desync rate, bandwidth usage

**Phase 2 (Optimization):**
- Enable by default (all players)
- Monitor server CPU, bandwidth
- A/B test: With vs without intent (player feedback)

**Phase 3 (Polish):**
- Remove feature flag (always on)
- Optimize based on metrics
- Document final performance characteristics

### Success Metrics

**Technical:**
- Desync rate: <5% of movements
- Bandwidth: <10 KB/sec per player
- CPU: <1ms per frame for predictions

**UX:**
- Playtest feedback: "Combat feels responsive"
- Ranged hit rate: 60-80% on moving targets
- No "teleporting NPC" complaints

**Business:**
- Player retention: Combat engagement increases
- Ranged builds viable (not just melee meta)
- PvP participation increases (fair combat)
