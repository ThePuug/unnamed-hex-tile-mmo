# SOW-016: Movement System Rewrite

## Status

**In Progress** - 2025-02-08

## References

- **RFC-016:** [Movement System Rewrite](../01-rfc/016-movement-system-rewrite.md)
- **ADR-019:** [Unified Interpolation Model](../02-adr/019-unified-interpolation-model.md)
- **Supersedes:** ADR-016 (Movement Intent Architecture) - intent protocol preserved, implementation unified
- **Branch:** (proposed)
- **Implementation Time:** 14-20 hours

---

## Implementation Plan

### Phase 1: Core Model

**Goal:** Establish new component architecture and pure physics calculation

**Deliverables:**
- `common/components/position.rs` - Position and VisualPosition components
- `common/systems/physics.rs` - Refactored physics as pure functions
- Unit tests for physics calculations

**Architectural Constraints:**
- Position stores tile (Qrz) and sub-tile offset (Vec3)
- VisualPosition stores from/to/progress/duration for interpolation
- Physics calculation is a pure function: `(Position, Heading, KeyBits, dt) -> Position`
- Physics function must be deterministic (same inputs = same outputs)
- Offset component remains temporarily for compatibility

**Success Criteria:**
- Position component can represent any valid game position
- Physics calculation produces correct results for all 6 hex directions
- Physics handles tile boundary crossing correctly
- Unit tests pass for movement in all directions, stationary, and tile crossing

**Duration:** 3-4 hours

---

### Phase 2: Local Player Movement

**Goal:** Implement client-side prediction with server reconciliation

**Deliverables:**
- `client/systems/input.rs` - Updated to use Position
- `client/systems/prediction.rs` - Local player prediction logic
- `common/systems/controlled.rs` - Simplified input queue handling
- Server-side input processing updates

**Architectural Constraints:**
- Input queue invariant maintained (always >= 1 input)
- Physics runs on client for local player (prediction)
- Server validates and sends authoritative Position updates
- Client reconciles prediction with server on Loc confirmation
- Reconciliation is smooth correction, not snap (unless teleport)
- Teleport threshold: 2+ tiles difference triggers snap

**Success Criteria:**
- Local player movement feels responsive (no added latency)
- Direction changes are smooth (no jitter)
- Server corrections apply smoothly (no visible snap for small desyncs)
- Large desyncs (teleport) snap correctly
- Input queue never empties (invariant holds)

**Duration:** 4-6 hours

---

### Phase 3: Remote Entity Movement

**Goal:** Unify remote entity handling with same interpolation model

**Deliverables:**
- `client/systems/remote.rs` - Remote entity position handling
- `server/systems/actor.rs` - MovementIntent broadcasting (existing protocol)
- Integration with existing MovementIntent message type

**Architectural Constraints:**
- Remote entities receive Position updates from server
- MovementIntent provides destination and duration for prediction
- Same VisualPosition interpolation as local player
- Late packets handled gracefully (update target, continue interpolating)
- Dropped MovementIntent packets acceptable (Loc confirmation still works)
- Relevance filtering: only process entities within view distance

**Success Criteria:**
- Remote entities move smoothly (no teleporting between tiles)
- Direction changes for remote entities are smooth
- Late MovementIntent packets don't cause visual artifacts
- Existing MovementIntent protocol unchanged (wire compatibility)

**Duration:** 3-4 hours

---

### Phase 4: Visual Polish and Cleanup

**Goal:** Finalize rendering integration and remove legacy code

**Deliverables:**
- `client/systems/interpolation.rs` - Unified interpolation system
- `client/systems/actor.rs` - Simplified Transform sync
- Remove Offset component and all references
- Update GUIDANCE.md with new architecture

**Architectural Constraints:**
- Interpolation runs every frame (Update schedule)
- Transform.translation = lerp(visual.from, visual.to, visual.progress)
- Jump/fall (AirTime) integrates with Position.offset.y
- Heading rotation uses same interpolation pattern
- All Offset references removed from codebase
- No regressions in existing features (combat, abilities, terrain)

**Success Criteria:**
- Movement visually smooth at all frame rates
- Tile boundary crossing invisible (no pop/snap)
- Jump arc smooth and natural
- Heading rotation smooth
- No compiler warnings about unused Offset code
- All existing tests pass
- Manual playtest confirms no visual artifacts

**Duration:** 4-5 hours (includes testing and polish)

---

## Acceptance Criteria

**Functional:**
- Local player movement responsive and jitter-free
- Remote entity movement smooth and predicted
- Direction changes smooth for all entities
- Tile boundary crossing seamless
- Jump/fall physics correct
- Server authority maintained (Position is authoritative)

**UX:**
- No visible jitter on direction change
- No teleporting between tiles
- Movement feels "weighty but responsive"
- No regression from current system (except fixing bugs)

**Performance:**
- Per-entity overhead: < 100 bytes additional memory
- Per-frame interpolation: < 0.1ms for 100 entities
- No additional network bandwidth (same protocol)

**Code Quality:**
- Physics calculation fully unit tested
- Offset component removed (no legacy code)
- GUIDANCE.md updated with new architecture
- Clear component documentation

---

## Discussion

### Phase 1 Implementation Note: Core Model Complete

**Deliverables created:**
- `common/components/position.rs` - Position and VisualPosition components
- `common/systems/movement.rs` - Pure movement calculation functions

**Key decisions:**

1. **Position combines tile + offset** - Single component holds both discrete tile (Qrz) and continuous sub-tile offset (Vec3), matching the existing Loc + Offset pattern but with clearer semantics.

2. **VisualPosition is purely visual** - The component only handles interpolation state (from, to, progress, duration). The key method `interpolate_toward()` always starts from the current visual position, which is the core fix for direction-change jitter.

3. **Movement functions are pure** - Extracted calculation logic into pure functions that take explicit inputs and return explicit outputs. This enables thorough unit testing and ensures determinism.

4. **Kept Offset for now** - The existing Offset component remains for compatibility during migration. It will be removed in Phase 4 after all systems are updated.

**Test coverage:**
- 13 Position/VisualPosition tests (component behavior, jitter prevention)
- 11 Movement tests (determinism, horizontal/vertical movement, heading)

### Phase 2 Implementation Note: Local Player Movement Complete

**Deliverables created:**
- `client/systems/prediction.rs` - Prediction systems (sync_position, update_visual_target, advance_interpolation)

**Deliverables modified:**
- `client/systems/actor.rs` - do_spawn adds Position + VisualPosition; update() uses VisualPosition.current() for local player
- `common/systems/world.rs` - do_incremental handles Position + VisualPosition on Loc changes (teleport snap, smooth crossing)
- `run-client.rs` - Registered prediction systems in FixedPostUpdate and Update schedules

**Key decisions:**

1. **Additive migration** - New components (Position, VisualPosition) are added alongside existing (Loc, Offset). Both pathways coexist. Old physics writes to Offset; prediction systems copy Offset.step → Position; VisualPosition interpolates toward Position. This allows gradual migration without breaking existing functionality.

2. **Schedule placement** - sync_position and update_visual_target run in FixedPostUpdate (after physics::update). advance_interpolation runs in Update before actor::update. This ensures Position is synced before VisualPosition targets it, and interpolation advances before rendering reads it.

3. **Local player only** - Phase 2 only applies VisualPosition to entities with InputQueues (local player). Remote entities continue using old Offset-based interpolation (Phase 3).

4. **Teleport handling** - do_incremental detects teleport (2+ hex distance) and calls VisualPosition.snap_to() for instant snap. Smooth tile crossings update Position.tile but let VisualPosition continue interpolating.

**Test coverage:**
- 4 prediction tests (interpolation flow, direction change jitter, teleport, smooth correction)

### Phase 3 Implementation Note: Remote Entity Movement Complete

**Modified files:**
- `client/systems/actor.rs` - `update()` uses VisualPosition for all entities (not just local). `apply_movement_intent` uses VisualPosition.interpolate_toward() for remote entities.
- `common/systems/world.rs` - Remote entity Loc handling updates Position. VisualPosition continues in world-space (no coordinate conversion needed).
- `client/systems/prediction.rs` - Updated module docs to reflect unified model.

**Key decisions:**

1. **Unified rendering path** - `actor::update()` now uses `VisualPosition.current()` for ALL entities that have it, regardless of local/remote. Fallback to legacy Offset interpolation only for entities without VisualPosition.

2. **VisualPosition is world-space** - Unlike Offset (which is tile-relative), VisualPosition stores world-space coordinates. When Loc changes for a remote entity, no coordinate conversion is needed - the interpolation target is already correct.

3. **apply_movement_intent simplified** - Uses `VisualPosition.interpolate_toward(dest_world, duration)` instead of manually calculating interpolation fractions and setting Offset fields. Offset.state still updated for combat distance calculations.

4. **Wire compatibility preserved** - No changes to MovementIntent protocol. Same unreliable channel, same destination + duration_ms format.

### Phase 4 Implementation Note: Client-Side Cleanup Complete

**Modified files:**
- `client/systems/camera.rs` - Replaced Offset component on camera entity with `CAMERA_OFFSET` const. Camera no longer depends on Offset.
- `client/systems/animator.rs` - Uses `VisualPosition.is_complete()` to detect movement (legacy fallback preserved for entities without VisualPosition).
- `client/systems/attack_telegraph.rs` - Uses `Transform.translation` instead of `Loc + Offset.step` for attack line positions. Since actor::update writes VisualPosition to Transform, this gets smooth positions for free.
- `client/systems/actor.rs` - Simplified `update()`: removed `Time<Fixed>`, `Time`, `InputQueues`, `&mut Offset` from query. Now just reads VisualPosition or falls back to tile center. Removed unused `Aabb` import.

**Key decisions:**

1. **Pragmatic scope** - Offset component NOT removed from codebase. Server-side systems (physics, chase, kite, actor, controlled, combat) still use Offset extensively. Full Offset removal deferred to a future SOW. This phase focused on client rendering path only.

2. **Camera uses const** - The camera's Offset was only ever used as a fixed `Vec3(0, 30, 40)` displacement. Replaced with a module-level const, eliminating the camera's dependency on the Offset component entirely.

3. **Animator uses VisualPosition** - Instead of reading `Offset.step` to detect movement, the animator checks `!vis.is_complete() && from.distance(to) > threshold`. This is more accurate since it reflects actual visual movement.

4. **Attack telegraph uses Transform** - Since `actor::update` already writes `VisualPosition.current()` to `Transform.translation`, the attack telegraph can simply read Transform for world positions. This removes the telegraph's direct dependency on Offset.

**Remaining Offset usage (deferred):**
- `do_spawn` still inserts `Offset::default()` (needed by physics::update)
- `prediction::sync_position` reads `Offset.step` → Position (bridge layer)
- `apply_movement_intent` writes `Offset.state` for combat distance calculations
- `do_incremental` adjusts Offset on tile crossing (bridge layer)
- All server-side systems (physics, chase, kite, controlled::tick/apply)

---

## Acceptance Review

*This section is populated after implementation is complete.*

---

## Sign-Off

**Reviewed By:** (pending)
**Date:** (pending)
**Decision:** (pending)
**Status:** Planned
