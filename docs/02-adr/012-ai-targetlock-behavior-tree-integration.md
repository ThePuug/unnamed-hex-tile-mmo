# ADR-012: AI TargetLock and Behavior Tree Integration

## Status

**Ready for Implementation** - 2025-10-30

## Context

**Related RFC:** [RFC-006: AI Behavior and Ability Integration](../01-rfc/006-ai-behavior-and-ability-integration.md)

NPCs must attack players using directional targeting. Current behavior trees pathfind but never attack, and behavior failures cause target switching (breaks sustained pressure).

### Requirements

- NPCs use abilities when in range (BasicAttack)
- NPCs maintain target commitment (no mid-chase abandonment)
- Sustained pressure fills reaction queue (validates ADR-006/007/008)
- Integration with ADR-009 directional targeting

### Options Considered

**Option 1: Behavior Tree Nodes** ✅ **SELECTED**
- Add combat nodes to existing bevy_behave system
- TargetLock component for sticky targeting
- Fastest to implement, consistent with existing architecture

**Option 2: Separate ECS System**
- Dedicated AI system checks conditions, emits abilities
- ❌ Splits behavior between tree and system
- ❌ More complex state management

**Option 3: Time-Based TargetLock**
- Lock expires after 10 seconds
- ❌ Adds complexity (time tracking)
- ❌ Worse gameplay (arbitrary abandonment)

## Decision

**Use behavior tree nodes with TargetLock component for sticky targeting.**

### Core Mechanism

**TargetLock Component (MANDATORY):**
```rust
pub struct TargetLock {
    pub locked_target: Entity,
    pub max_chase_distance: u32,  // Leash (0 = infinite)
}
```
- Sticky target acquisition (persists until invalid)
- Prevents mid-chase abandonment
- Releases only on: death, despawn, or exceeding leash distance
- **CRITICAL**: Without this, behavior tree failures cause target switching

**FindOrKeepTarget Behavior Node:**
- Checks existing TargetLock first
- Only searches for new target if lock invalid
- Inserts TargetLock on new acquisition
- Idempotent (multiple calls don't change locked target)

**FaceTarget Behavior Node:**
- Updates NPC heading to face Target entity
- Runs twice per sequence (before and after pathfinding)
- Prevents facing-cone failures

**UseAbilityIfAdjacent Behavior Node:**
- Checks: GCD ready (ADR-011), adjacent (distance=1), facing (60° cone)
- Emits `Try::UseAbility` if all conditions met
- Integrates with ADR-009 directional targeting

**Wild Dog Behavior Tree:**
```rust
Behave::Forever => {
    Behave::Sequence => {
        FindOrKeepTarget { dist: 20, leash_distance: 30 },
        FaceTarget,                                         // Initial
        Nearby { min: 1, max: 1, origin: Target },
        PathTo::default(),
        FaceTarget,                                         // Re-face
        UseAbilityIfAdjacent { ability: BasicAttack },
        Wait(1.0),                                          // Attack cooldown
    }
}
```

---

## Rationale

### 1. TargetLock Prevents Cascade Failures

**Problem:** Behavior tree sequence failures cause `FindSomethingInterestingWithin` to re-run, selecting different targets.

**Solution:** TargetLock makes acquisition sticky, persisting across behavior tree loops.

**Impact:**
- Enables sustained pressure (threats accumulate in queue)
- Dogs commit to targets (no mid-chase abandonment)
- Validates reaction queue mechanics (ADR-006/007/008)
- **MANDATORY for functional combat**

### 2. Dual FaceTarget Pattern

**Problem:** Pathfinding may update heading, breaking facing cone check.

**Solution:** Run FaceTarget twice - before pathfinding (initial) and after (correction).

**Impact:** Prevents UseAbilityIfAdjacent failures due to heading mismatch.

### 3. Behavior Tree Integration

- Consistent with existing NPC architecture
- Per-NPC configurability (different templates)
- Declarative behavior definition
- Fastest to implement

### 4. No Time-Based Expiry

**Simpler approach:** Lock persists until target becomes invalid (not time-based).

**Benefits:**
- No time tracking needed
- Better gameplay (commit to target until invalid)
- Self-healing (auto-releases dead/despawned targets)

---

## Consequences

### Positive

- **Target commitment:** No mid-chase abandonment (better gameplay)
- **Sustained pressure:** Dogs attack every ~1s (validates queue)
- **Behavior tree consistency:** Reuses existing architecture
- **Extensible:** Add new nodes for complex AI patterns

### Negative

- **Behavior tree coupling:** Tight dependency on bevy_behave
- **Sequence complexity:** 7-step sequence (target >80% completion)
- **Testing burden:** Behavior trees harder to unit test
- **Heading timing:** Pathfinding may override (mitigated by dual FaceTarget)

### Mitigations

- TargetLock prevents most sequence failures
- Dual FaceTarget corrects heading issues
- Wait(1.0) provides GCD recovery buffer
- Integration tests validate full behavior tree

---

## Implementation Notes

**Component location:** `server/components/target_lock.rs`

**Behavior nodes:**
- `server/systems/behaviour/find_target.rs` - FindOrKeepTarget
- `server/systems/behaviour/face_target.rs` - FaceTarget
- `server/systems/behaviour/use_ability.rs` - UseAbilityIfAdjacent

**Integration points:**
- Uses `is_in_facing_cone` from ADR-009 (directional targeting)
- Uses `Gcd` component from ADR-011 (cooldown tracking)
- Emits `Try::UseAbility` → processed by ADR-009 `execute_ability`

**Network:** TargetLock not synced (server-authoritative)

---

## Validation Criteria

**Functional:**
- TargetLock persists until target invalid
- FindOrKeepTarget maintains lock (no target switching)
- FaceTarget updates heading correctly (twice per sequence)
- UseAbilityIfAdjacent emits abilities when conditions met
- Full combat loop: Dog → attack → queue fills → Dodge required

**Critical tests:**
- **Target commitment:** Dog locks to Player A, ignores closer Player B
- **Sustained pressure:** 2 Dogs fill queue within 3 seconds
- **Behavior tree success:** >80% sequence completion rate

**Performance:**
- 100 Wild Dogs attacking: < 10% CPU
- Behavior tree overhead: negligible

---

## References

- **RFC-006:** AI Behavior and Ability Integration
- **ADR-009:** Directional Targeting (facing cone, select_target)
- **ADR-011:** GCD Component (cooldown tracking)
- **ADR-006/007/008:** Reaction Queue (validates sustained pressure)

## Date

2025-10-30
