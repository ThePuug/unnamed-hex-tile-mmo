# SOW-006: AI Behavior and Ability Integration

## Status

**Ready for Implementation** - 2025-10-30

## References

- **RFC-006:** [AI Behavior and Ability Integration](../01-rfc/006-ai-behavior-and-ability-integration.md)
- **ADR-011:** [GCD Component for Cooldown Tracking](../02-adr/011-gcd-component-cooldown-tracking.md)
- **ADR-012:** [AI TargetLock and Behavior Tree Integration](../02-adr/012-ai-targetlock-behavior-tree-integration.md)
- **Branch:** (proposed)
- **Implementation Time:** 9-10 days

---

## MVP Critical Path: Three Must-Have Components

**The following three components are MANDATORY for functional AI combat:**

1. **TargetLock Component (CRITICAL)** - Sticky target acquisition (prevents mid-chase abandonment)
2. **GCD Component (CRITICAL)** - Cooldown enforcement (prevents ability spam)
3. **FindOrKeepTarget Node (CRITICAL)** - Target persistence across behavior tree loops

**Without all three, Wild Dogs will behave unreliably and break combat pressure testing.**

---

## Implementation Plan

### Phase 1: GCD Component (1 day)

**Goal:** Add cooldown tracking to all actors

**Deliverables:**
- `common/components/gcd.rs` module
- `Gcd` struct with `gcd_type`, `expires_at`
- Methods: `is_active()`, `activate()`, `clear()`
- Unit tests for GCD timing logic
- Insert `Gcd::new()` in spawn flows (NPCs and players)
- Update `execute_ability` to validate GCD

**Architectural Constraints:**
- Component (not resource) for per-entity state
- Uses `Time::elapsed()` (not `Instant::now()`)
- `expires_at` field (not duration, simpler checks)
- Shared by players and NPCs (DRY principle)

**Success Criteria:** All actors spawn with `Gcd`, server validates before abilities, GCD prevents spam (<0.5s)

---

### Phase 2: TargetLock Component (1 day)

**Goal:** Add sticky target acquisition (MANDATORY for MVP)

**Deliverables:**
- `server/components/target_lock.rs` module
- `TargetLock` struct with `locked_target`, `max_chase_distance`
- Method: `is_target_valid(target_loc, npc_loc) -> bool`
- Unit tests for validation logic
- Register component (server-only, no sync)

**Architectural Constraints:**
- Server-only (clients don't need target lock state)
- Leash distance enforcement (0 = infinite)
- No time-based expiry (persists until invalid)
- Validation: Health > 0, entity exists, within leash

**Success Criteria:** TargetLock compiles, passes unit tests, handles despawn/distance edge cases

---

### Phase 3: FindOrKeepTarget Behavior Node (2 days)

**Goal:** Sticky targeting (prevents mid-sequence target switching)

**Deliverables:**
- Modify `server/systems/behaviour/find_target.rs`
- `FindOrKeepTarget` component with `dist`, `leash_distance`
- `find_or_keep_target` system:
  - Check existing TargetLock first
  - Validate locked target (Health > 0, within leash)
  - Remove lock if invalid
  - Find new target if no lock or invalid
  - Insert TargetLock on new acquisition
- Unit tests for sticky behavior
- Update Wild Dog behavior tree to use FindOrKeepTarget

**Architectural Constraints:**
- Idempotent (multiple calls don't change locked target while valid)
- Self-healing (auto-releases invalid targets)
- Per-NPC configuration (Wild Dog: dist=20, leash=30)

**Success Criteria:** NPC locks to target, maintains lock until invalid, ignores closer targets while locked

---

### Phase 4: FaceTarget Behavior Node (1 day)

**Goal:** NPCs update heading to face Target

**Deliverables:**
- `server/systems/behaviour/face_target.rs` module
- `FaceTarget` component
- `face_target` system:
  - Query Target entity
  - Calculate heading from NPC to Target
  - Update NPC's Heading component
- Register with behavior plugin
- Update Wild Dog behavior tree (insert FaceTarget twice: before and after pathfinding)

**Architectural Constraints:**
- Uses `Heading::new(direction_qrz)` (6 cardinals)
- Discretizes continuous direction → nearest cardinal
- Runs twice per sequence (before Nearby, after PathTo)

**Success Criteria:** Wild Dog faces player after finding them, re-faces after pathfinding, heading visible to clients

---

### Phase 5: UseAbilityIfAdjacent Behavior Node (2 days)

**Goal:** NPCs emit abilities when conditions met

**Deliverables:**
- `server/systems/behaviour/use_ability.rs` module
- `UseAbilityIfAdjacent` component with `ability` field
- `use_ability_if_adjacent` system:
  - Check GCD ready
  - Check adjacent (distance == 1)
  - Check facing (60° cone via `is_in_facing_cone`)
  - Emit `Try::UseAbility` if all conditions met
- Helper: `is_in_facing_cone` (from ADR-009)
- Register with behavior plugin
- Update Wild Dog behavior tree

**Architectural Constraints:**
- Uses ADR-009 directional targeting (facing cone check)
- GCD check optimizes network (don't send if GCD active)
- Server validates again (authoritative)
- Failure returns Success=false (node retries next loop)

**Success Criteria:** Wild Dog emits `Try::UseAbility` when adjacent + facing + GCD ready

---

### Phase 6: Wild Dog Behavior Tree Integration (1 day)

**Goal:** Complete attack pattern with sustained pressure

**Deliverables:**
- Update `spawner.rs::spawn_npc` behavior tree:
  - FindOrKeepTarget { dist: 20, leash: 30 }
  - FaceTarget (initial)
  - Nearby { min: 1, max: 1, origin: Target }
  - PathTo::default()
  - FaceTarget (re-face after pathfinding)
  - UseAbilityIfAdjacent { ability: BasicAttack }
  - Wait(1.0) - 1 second cooldown (enables queue pressure)
- Test full sequence

**Architectural Constraints:**
- 7-step sequence (FindOrKeepTarget → FaceTarget → Nearby → PathTo → FaceTarget → UseAbility → Wait)
- Wait(1.0) creates queue pressure (attacks faster than queue timers)
- Target >80% sequence completion rate

**Success Criteria:** Wild Dog pursues, faces, attacks player every ~1 second

---

### Phase 7: Integration Testing (2 days)

**Goal:** Validate full combat loop with sustained pressure (CRITICAL for ADR-006/007/008 validation)

**Deliverables:**
- Test target lock behavior:
  - Dog locks to Player A
  - Player B runs past (closer)
  - Dog IGNORES Player B, continues chasing Player A
  - Player A moves >30 hexes (leash), Dog releases lock
  - Player A dies, Dog releases lock and finds new target
- Test sustained pressure (MOST IMPORTANT):
  - Player with Focus=0 (3 slots), Instinct=0 (1.0s timers)
  - 2 Wild Dogs attack adjacent
  - Dogs attack every 1 second
  - Queue fills to capacity within 3 seconds
  - Player forced to Dodge or take overflow damage
  - **This validates ADR-006/007/008 reaction queue mechanics**
- Measure behavior tree success rate:
  - Log sequence failures
  - Count: sequences completed vs restarted
  - **Target: >80% completion rate**
- Test edge cases:
  - Player moves away mid-attack (Dog pursues, maintains lock)
  - Multiple Dogs attack same player (threats accumulate)
  - Player kills Dog (Dog despawns)
  - GCD active (UseAbilityIfAdjacent fails gracefully, retries)

**Architectural Constraints:**
- Dogs must maintain target lock (no abandonment)
- Attack speed must be ~1 second (queue pressure)
- Behavior tree success rate >80%

**Success Criteria:**
- ✅ Target switching eliminated (measured via logs)
- ✅ Queue accumulation works (2-3 threats under sustained pressure)
- ✅ Sequence success rate >80%
- ✅ Player must use Dodge to survive
- ✅ Combat feels intense and consistent

---

## Acceptance Criteria

**Functionality:**
- ✅ GCD component tracks cooldowns (<0.5s between abilities)
- ✅ TargetLock persists until invalid (death/despawn/leash)
- ✅ FindOrKeepTarget maintains lock (no target switching)
- ✅ FaceTarget updates heading twice per sequence
- ✅ UseAbilityIfAdjacent emits abilities when conditions met
- ✅ Wild Dog attacks every ~1 second
- ✅ 2 Dogs fill queue within 3 seconds (sustained pressure)

**Performance:**
- ✅ 100 Wild Dogs attacking: < 10% CPU
- ✅ 1000 actors checking GCD: < 1ms
- ✅ 10 Dogs attacking: ~500 bytes/sec network

**Code Quality:**
- ✅ Behavior nodes registered with plugin
- ✅ Unit tests for components and helpers
- ✅ Integration tests for full combat loop
- ✅ Behavior tree success rate >80%

---

## Discussion

### Design Decision: TargetLock is MANDATORY

**Context:** Original design considered TargetLock optional for MVP.

**Decision:** TargetLock is MANDATORY.

**Rationale:**
- Without it, behavior tree failures cause target switching
- Target switching breaks sustained pressure (no queue accumulation)
- Queue accumulation required to validate ADR-006/007/008
- Combat pressure testing blocked without reliable targeting

**Impact:** MVP implementation time increased by 1 day (TargetLock component + FindOrKeepTarget node)

---

### Design Decision: Dual FaceTarget Pattern

**Context:** Should FaceTarget run once or twice?

**Decision:** Run twice - before Nearby, after PathTo.

**Rationale:**
- Pathfinding may update heading during movement
- Without re-facing, UseAbilityIfAdjacent fails (not in facing cone)
- Second FaceTarget corrects heading after pathfinding
- Prevents sequence failures

**Impact:** Slight behavior tree complexity increase, but prevents facing-cone failures

---

### Design Decision: Wait(1.0) for Queue Pressure

**Context:** Attack speed affects queue accumulation.

**Decision:** Wait(1.0) - 1 second between attacks.

**Rationale:**
- Queue timers: 0.5-1.5s (Instinct-scaled)
- Attacks every 1s → threats accumulate faster than expiry
- Enables sustained pressure (validates queue mechanics)
- Original spec said 2-3s, but that's too slow for queue pressure

**Impact:** Attack speed faster than spec, but necessary for testing

---

### Implementation Note: Behavior Tree Success Rate

**Target:** >80% sequence completion rate

**Measurement:**
- Log each sequence failure
- Count: sequences completed vs restarted
- Failing nodes: Note which step fails most

**Concern:** 7-step sequence has many failure points

**Mitigation:**
- TargetLock prevents most failures (target switching eliminated)
- Dual FaceTarget prevents facing failures
- Wait(1.0) provides GCD recovery
- If <80%: Structural problem (needs redesign)

---

## Acceptance Review

**Status:** Pending implementation

---

## Conclusion

The AI behavior integration enables NPCs to attack players using the ability system.

**Key Achievements (planned):**
- TargetLock prevents mid-chase abandonment
- Sustained attack pressure validates reaction queue
- Behavior tree integration reuses existing architecture
- GCD component shared by players and NPCs

**Architectural Impact:** Completes combat loop, enables reaction queue validation, unblocks combat playtesting.

**The implementation achieves RFC-006's core goal: Wild Dogs reliably attack players every ~1 second, creating sustained combat pressure.**

---

## Sign-Off

**Status:** Awaiting implementation
