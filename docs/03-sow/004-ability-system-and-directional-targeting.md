# SOW-004: Ability System and Directional Targeting

## Status

**Proposed** - 2025-10-30

## References

- **RFC-004:** [Ability System and Directional Targeting](../01-rfc/004-ability-system-and-directional-targeting.md)
- **ADR-009:** [Heading-Based Directional Targeting](../02-adr/009-heading-based-directional-targeting.md)
- **Branch:** `ability-system-directional-targeting` (proposed)
- **Implementation Time:** 11-13 days

---

## Implementation Plan

### Phase 1: Heading to Facing Cone Conversion (1 day)

**Goal:** Convert 6-direction heading into 60° facing cone for targeting

**Deliverables:**
- `common/systems/targeting.rs` module
- `Heading::to_angle()` method (NE=30°, E=90°, SE=150°, etc.)
- `is_in_facing_cone(heading, caster_loc, target_loc) -> bool`
- Unit tests for all 6 headings and various target angles

**Architectural Constraints:**
- Pure functions (no state, testable)
- Shared in `common/` (client and server use identically)
- Angular math handles wrap-around (360° → 0°)
- Facing cone exactly ±30° from heading angle

**Success Criteria:** All heading→angle conversions correct, facing cone detection accurate within 60° arc

---

### Phase 2: Automatic Target Selection System (2 days)

**Goal:** Implement target selection algorithm with geometric tiebreaker

**Deliverables:**
- `select_target(loc, heading, tier_lock, nntree, query) -> Option<Entity>`
- `get_range_tier(distance) -> RangeTier` (close 1-2, mid 3-6, far 7+)
- Algorithm: Query NNTree → filter facing cone → nearest → tiebreaker
- Unit tests for determinism and tiebreaker edge cases

**Architectural Constraints:**
- Pure function (deterministic, same inputs → same target)
- Uses existing NNTree for spatial queries
- Filters to `EntityType::Actor` only (no decorators)
- Geometric tiebreaker: Closest to exact heading angle
- MVP: No tier lock filtering (parameter exists for Phase 2)

**Success Criteria:** Target selection deterministic, tiebreaker resolves equidistant targets, performance < 1ms for 100 entities

---

### Phase 3: Client-Side Target Indicator Rendering (2 days)

**Goal:** Show red indicator on current hostile target

**Deliverables:**
- `client/systems/targeting_ui.rs` module
- `render_target_indicators` system (runs in Update, every frame)
- Query local player's Loc, Heading
- Call `select_target` to get hostile
- Spawn/update red sprite at target position

**Architectural Constraints:**
- Client-side only (server doesn't render indicators)
- Update every frame for smooth movement (~60fps)
- Spawn indicator when target exists, despawn when no valid target
- Indicator follows target position (updates every frame)

**Success Criteria:** Red indicator visible on nearest hostile, updates smoothly as player moves/turns, disappears when no targets

---

### Phase 4: Ability Execution with Directional Targeting (3 days)

**Goal:** BasicAttack and Dodge integrate with targeting system

**Deliverables:**
- `common/systems/abilities.rs` - Ability definition system
- `AbilityType` enum (BasicAttack, Dodge)
- `TargetingPattern` enum (SingleTarget, SelfTarget)
- Client input handling: Q key → BasicAttack, E key → Dodge
- Server validation: Recalculate target, validate range/resources
- Network events: UseAbility, AbilityUsed, AbilityFailed

**Architectural Constraints:**
- Server recalculates target (doesn't trust client)
- Server uses identical `select_target` algorithm (deterministic)
- Client validates locally before sending (range, resources)
- BasicAttack range = 1 hex (adjacent only)
- Dodge cost = 30 stamina (from spec, may update to 15% max)

**Success Criteria:** Player faces Wild Dog, presses Q → BasicAttack hits Dog; Player presses E → Dodge clears queue

---

### Phase 5: Client Prediction for Abilities (2 days)

**Goal:** Client predicts ability usage for local player

**Deliverables:**
- Client prediction: Spend stamina, apply effects optimistically
- Send `Try::UseAbility` to server
- Rollback handling: Receive `AbilityFailed` → undo predictions
- Confirmation handling: Receive `AbilityUsed` → prediction correct

**Architectural Constraints:**
- Predict only for local player (remote players wait for server)
- Update `step` fields (not `state` - server owns state)
- Show predicted state with visual distinction (if rollback needed)
- Local validation before prediction (prevent bad predictions)

**Success Criteria:** Local player presses E → stamina drops instantly (predicted), server confirms within 100ms

---

### Phase 6: Enemy AI Directional Targeting (2-3 days)

**Goal:** Wild Dog uses directional targeting to attack player

**Deliverables:**
- Update `server/systems/ai/wild_dog.rs`
- Query Dog's Loc, Heading
- Use `select_target` to find nearest player
- If in range 1: Attack (send `Try::UseAbility`)
- If farther: Pathfind toward player, update heading

**Architectural Constraints:**
- Server-side only (AI doesn't predict)
- Uses same `select_target` algorithm as player
- Heading updates during movement (face movement direction)
- Attack every 2 seconds (existing behavior)

**Success Criteria:** Wild Dog faces player and attacks, inserts threats into player's reaction queue

---

## Acceptance Criteria

**Functionality:**
- ✅ Heading conversion correct (all 6 directions → angles)
- ✅ Facing cone detection accurate (60° arc)
- ✅ Target selection deterministic (same inputs → same target)
- ✅ Red indicator shows current target, updates smoothly
- ✅ BasicAttack hits indicated target
- ✅ Dodge clears queue instantly
- ✅ Wild Dog attacks player using same targeting

**Performance:**
- ✅ Target selection: < 1ms for 100 entities
- ✅ Indicator update: 60fps maintained
- ✅ AI targeting: < 5% CPU for 100 Wild Dogs

**Network:**
- ✅ Prediction within 100ms of server confirmation
- ✅ Target mismatch rare (< 1% of abilities)
- ✅ Rollback within 1 frame if ability fails

**Code Quality:**
- ✅ Shared logic in `common/systems/targeting.rs`
- ✅ Pure functions (deterministic, testable)
- ✅ Comprehensive unit tests
- ✅ Client/server use identical algorithm

---

## Discussion

### Design Decision: Server Recalculates Target

**Context:** Client sends target entity to server. Should server trust it?

**Decision:** Server recalculates target using identical `select_target` algorithm.

**Rationale:**
- Prevents cheating (client can't send fake target)
- Server validates target is actually in facing cone and range
- Latency tolerance: Server accepts client's target if "close enough"
- Deterministic algorithm ensures client/server usually agree

**Impact:** Rare "Invalid target" errors when latency causes mismatch (< 1% of abilities)

---

### Design Decision: Geometric Tiebreaker

**Context:** Multiple enemies at same distance. Which to select?

**Decision:** Select entity closest to exact heading angle.

**Rationale:**
- Deterministic (no arbitrary selection)
- Rewards precise positioning (face exactly at target)
- Consistent across client/server (same algorithm)

**Example:**
- Caster facing E (90°), entities at 80° and 100° (both distance 2)
- Select entity at 80° (10° delta vs 10° delta - actually a tie, pick first)
- Provides sub-60° targeting precision

---

### Implementation Note: Tier Lock Deferred

**MVP excludes:** Tier lock system (1/2/3 keys to lock range tiers)

**Rationale:**
- MVP combat simple (Wild Dog melee only, no need for tier selection)
- Automatic targeting sufficient for single-enemy encounters
- Phase 2 adds tier lock before complex multi-tier encounters

**Prepared for future:** `select_target` already has `tier_lock: Option<RangeTier>` parameter

---

### Implementation Note: Target Indicator Performance

**Update frequency:** Every frame (~60fps) for local player

**Performance concern:** Recalculating target every frame may lag

**Mitigation:**
- NNTree queries fast (< 0.5ms)
- Only local player updates (remote entities don't need indicators)
- Dirty flag optimization if profiling shows issue

**Measurement:** Profile in Phase 3, optimize if < 60fps

---

### Implementation Note: Client-Server Target Tolerance

**Problem:** Latency causes client and server to select different targets

**MVP approach:** Strict match (client and server must select same entity)

**If issues arise:** Add tolerance (server accepts if client's target within 1 hex of server's selection)

**Reasoning:** Wild Dogs stationary (low mismatch risk), can relax later if needed

---

## Acceptance Review

**Status:** Pending implementation

---

## Conclusion

The ability system implementation enables directional combat gameplay through heading-based automatic targeting.

**Key Achievements (planned):**
- Reuses existing Heading component (no new infrastructure)
- Deterministic shared logic (client/server identical)
- Keyboard-only combat (controller-ready)
- Fast performance (< 1ms target selection)

**Architectural Impact:** Enables combat testing, damage pipeline (ADR-005), and AI behavior (ADR-006).

**The implementation achieves RFC-004's core goal: enabling directional combat with keyboard-only targeting.**

---

## Sign-Off

**Status:** Awaiting implementation
