# SOW-011: Movement Intent System

## Status

**Merged** - 2025-11-05

## References

- **RFC-011:** [Movement Intent System](../01-rfc/011-movement-intent-system.md)
- **ADR-016:** [Movement Intent Architecture - "Intent then Confirmation" Pattern](../02-adr/016-movement-intent-architecture.md)
- **Spec:** [Combat System Specification](../00-spec/combat-system.md) (Projectile targeting)
- **Branch:** (proposed)
- **Implementation Time:** 6-9 days

---

## Implementation Plan

### Phase 1: Core Intent System (Foundation)

**Goal:** Broadcast movement intentions and enable client prediction for remote entities

**Deliverables:**
- `Event::MovementIntent` message type in `common/message.rs`
- `MovementPrediction` component in `common/components/`
- `MovementIntentState` component for tracking broadcast state
- Server system to broadcast intents when movement starts
- Client system to apply intents and start prediction
- Client system to validate predictions against Loc confirmations

**Architectural Constraints:**
- Server broadcasts intent when `Offset.state` indicates movement toward different tile
- Intent includes: destination Qrz, duration_ms (from movement speed), sequence number
- Client predicts by setting `Offset.step` toward destination, starting interpolation
- Client validates when `Event::Loc` arrives - if match, smooth continuation; if mismatch, snap correction
- Sequence numbers must detect packet reordering (like Input.seq pattern)
- Intent broadcast happens AFTER physics updates offset, BEFORE Loc updates
- Server never re-broadcasts same intent (track last_broadcast to avoid spam)

**Success Criteria:**
- Remote entities start moving within 50-150ms of server movement start (network latency only)
- Prediction accuracy >95% (desyncs rare, visible snap when they occur)
- No duplicate intent broadcasts for same movement
- Sequence numbers correctly detect and ignore reordered packets
- Fallback works: if intent lost, Loc-only interpolation still functions

**Duration:** 2-3 days

---

### Phase 2: Relevance Filtering (Bandwidth Optimization)

**Goal:** Prevent broadcasting intents to distant players who can't see the entity

**Deliverables:**
- Integration with existing relevance system (30 hex radius filter)
- Intent events respect same visibility rules as Loc events

**Architectural Constraints:**
- Reuse existing relevance filtering infrastructure (don't reinvent)
- Intent broadcasts filtered by same 30 hex radius as other entity events
- No special-case logic - intents are just another event type
- Bandwidth target: <10 Kbps per player with 5-10 entities in range

**Success Criteria:**
- Players don't receive intents for entities beyond relevance radius
- Bandwidth overhead measured and within target (<10 Kbps per player)
- No performance regression from intent filtering

**Duration:** 1 day

---

### Phase 3: Projectile Targeting Integration

**Goal:** Enable projectiles to aim at predicted positions instead of stale positions

**Deliverables:**
- Update projectile targeting logic to check for `MovementPrediction` component
- Projectiles aim at predicted destination when component present
- Projectiles aim at current position when component absent (stationary targets)

**Architectural Constraints:**
- Integration with ADR-015 projectile system (entity-based projectiles)
- Targeting decision at projectile creation time (not during flight)
- If prediction exists, aim at `predicted_dest`; otherwise aim at `loc + offset.step`
- No changes to projectile flight mechanics (travel time unchanged)
- Works for all projectile sources (NPCs attacking moving players, players attacking moving NPCs)

**Success Criteria:**
- Projectiles fired at moving targets lead the target (aim at predicted position)
- Projectiles fired at stationary targets aim at current position
- Hit rate for moving targets improves from baseline (<50% without leading)
- Forest Sprite projectiles can hit moving players
- Player projectiles (when added) can hit moving Forest Sprites

**Duration:** 1-2 days

---

### Phase 4: Polish and Edge Cases

**Goal:** Handle edge cases gracefully and improve prediction robustness

**Deliverables:**
- Packet loss handling (intent lost, only Loc arrives)
- Rapid direction change handling (multiple intents in flight)
- Teleport handling (Lunge ability, dev console teleports)
- Desync telemetry (track prediction accuracy, snap frequency)

**Architectural Constraints:**
- Packet loss: Client falls back to Loc-only interpolation (graceful degradation)
- Rapid direction changes: Sequence numbers ensure correct ordering, validation catches mismatches
- Teleports: Clear prediction component on instant position changes (don't try to predict teleports)
- Telemetry: Log desync events with predicted vs. confirmed positions (for balancing)
- All edge cases result in snap correction (visible but acceptable) rather than incorrect state

**Success Criteria:**
- Packet loss doesn't cause stuck entities (Loc-only fallback works)
- Rapid direction changes don't cause visual artifacts (snaps acceptable)
- Teleports clear prediction state (no interpolation toward old destination)
- Desync rate measured and within acceptable range (<5% of movements)
- No entities stuck in invalid states (all edge cases recoverable)

**Duration:** 1-2 days

---

### Phase 5: Batching (Optional Performance)

**Goal:** Reduce message overhead by batching multiple intents into single network event

**Deliverables:**
- Batched intent messages (multiple entities per event)
- Unbatching on client (distribute to correct entities)

**Architectural Constraints:**
- OPTIONAL: Only implement if bandwidth profiling shows it's needed
- Batch up to 10 intents per message (balance latency vs. bandwidth)
- Batching window: 16ms (one frame at 60 FPS)
- Maintains per-entity sequence numbers (batching doesn't break ordering)
- Client unbatches and applies intents individually

**Success Criteria:**
- Bandwidth reduction measured (target: 30-50% reduction vs. individual messages)
- No increase in perceived latency (batching window small enough)
- Sequence number ordering still correct after unbatching

**Duration:** 1 day (optional)

---

## Acceptance Criteria

**Functional:**
- Remote entities move smoothly without teleporting between tiles
- Prediction accuracy >95% (measured via telemetry)
- Projectiles aim at predicted positions when targets moving
- Desyncs snap correctly (visible but rare, <5% of movements)
- Edge cases handled gracefully (packet loss, direction changes, teleports)

**Performance:**
- Bandwidth overhead <10 Kbps per player (5-10 entities in range)
- No CPU overhead beyond existing systems (prediction reuses interpolation)
- Memory overhead minimal (MovementPrediction cleared after validation)

**UX:**
- Combat feels responsive for all entities (not just local player)
- Ranged combat viable (projectiles can hit moving targets)
- Dodging works (changing direction evades projectiles)
- Snaps rare enough to not feel broken (<5% of movements)

**Code Quality:**
- Intent broadcasting isolated in server systems
- Prediction logic isolated in client systems
- Reuses existing patterns (sequence numbers, validation, interpolation)
- Edge cases documented and tested

---

## Discussion

*This section will be populated during implementation with questions, decisions, and deviations.*

---

## Acceptance Review

*This section will be populated after implementation is complete.*

---

## Sign-Off

**Reviewed By:** (pending)
**Date:** (pending)
**Decision:** (pending)
