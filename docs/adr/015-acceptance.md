# ADR-015 Acceptance Review

**Date:** 2025-11-08
**Reviewer:** ARCHITECT
**Status:** ⚠️ **PARTIAL ACCEPT** - Priority 1 accepted, Priority 2 requires revision

---

## Executive Summary

Priority 1 implementation demonstrates solid architectural understanding with tests that verify **actual system invariants**. Priority 2 implementation has fundamental issues: tests validate library behavior and formulas rather than architectural properties.

**Recommendation:** Accept Priority 1, reject Priority 2, revise ADR-015 to clarify what "architectural invariants" means.

---

## Priority 1 Assessment: ✅ ACCEPT

### Strengths

**INV-002: InputQueue Never Empty** ([physics.rs:301-326](../../src/common/systems/physics.rs#L301-L326))
- ✅ **Tests actual invariant**: Verifies system panics when violated
- ✅ **Architectural property**: Empty queue causes production code to fail
- ✅ **Prevents regression**: Will catch if someone adds empty queue handling

**INV-003: World-Space Preservation** ([world.rs:243-320](../../src/common/systems/world.rs#L243-L320))
- ✅ **Tests coordinate transform invariant**: Validates `world_pos_before == world_pos_after`
- ✅ **Architectural boundary condition**: Teleport vs smooth crossing distinction
- ✅ **Mathematical proof**: Tests geometric properties, not implementation details
- ✅ **Comprehensive coverage**: 4 tests cover edge cases (adjacent=1, teleport≥2, boundaries)

**INV-005: Two-Phase Damage Timing** ([damage.rs:152-313](../../src/common/systems/combat/damage.rs#L152-L313))
- ✅ **Tests timing separation**: Attack-time attrs vs resolution-time attrs
- ✅ **Architectural pattern**: Verifies snapshot behavior (attributes frozen)
- ✅ **Critical for fairness**: Mid-queue buff/debuff handling
- ✅ **8 tests**: Good coverage of both phases and both damage types

**INV-007: Damage Reduction Caps** ([resources.rs:295-376](../../src/common/systems/combat/resources.rs#L295-L376))
- ✅ **Tests game-critical constraint**: 75% cap prevents invulnerability
- ✅ **Balance enforcement**: Formula can change, cap cannot
- ✅ **Clear boundary**: Tests both below-cap and at-cap scenarios

**INV-009: Heading Offset Magnitude** ([physics.rs:386-408](../../src/common/systems/physics.rs#L386-L408))
- ✅ **Tests positional constant**: HERE = 0.33 relationship
- ✅ **Spatial invariant**: Heading-based positioning consistency

**INV-001: Client-Side Prediction Determinism** ([physics.rs:328-384](../../src/common/systems/physics.rs#L328-L384))
- ✅ **Critical multiplayer invariant**: Client must match server exactly
- ✅ **Tests pure function**: `apply()` determinism
- ✅ **2 tests**: Determinism + movement prediction match

### Priority 1 Code Quality

- **Clear test names**: All tests explain what they verify
- **Good documentation**: INV-XXX headers explain why invariant matters
- **Minimal brittleness**: Tests logic, not implementation details
- **Pure function tests preferred**: Avoid ECS integration where possible

---

## Priority 2 Assessment: ❌ REJECT

### Critical Issues

#### Issue 1: Testing Standard Library Behavior

**INV-004: ReactionQueue FIFO** ([reaction_queue.rs:68-111](../../src/common/components/reaction_queue.rs#L68-L111))

❌ **Problem:** Tests that `VecDeque::push_back()` and `pop_front()` work like FIFO
```rust
queue.threats.push_back(threat1);
queue.threats.push_back(threat2);
let first = queue.threats.pop_front().unwrap();
assert_eq!(first.damage, 10.0); // Of course it's 10.0, that's how VecDeque works
```

**Real invariant not tested:**
- Does `process_expired_threats()` actually process in FIFO order?
- What happens if threats are inserted out-of-order by timestamp?
- Does the system maintain FIFO when threats have different timers?

**Architectural value:** ⚠️ **ZERO** - This is testing Rust standard library correctness

---

**INV-004: Lobby Bidirectional Mapping** ([renet.rs:470-538](../../src/server/systems/renet.rs#L470-L538))

❌ **Problem:** Tests that `BiHashMap::insert()` and `get_by_left()` work
```rust
lobby.insert(client1, entity1);
assert_eq!(lobby.get_by_left(&client1), Some(&entity1)); // BiHashMap does this
```

**Real invariant not tested:**
- Does `handle_client_connected()` actually maintain 1:1 mapping?
- What happens on client reconnect with same ClientId?
- Is orphaned entity cleanup actually invoked?

**Architectural value:** ⚠️ **ZERO** - This is testing `bimap` crate correctness

---

#### Issue 2: Testing Formulas Instead of Timing

**INV-006: Crit Roll at Attack Time** ([damage.rs:315-406](../../src/common/systems/combat/damage.rs#L315-L406))

❌ **Problem:** Tests formula correctness, not attack-time timing
```rust
for _ in 0..10000 {
    if roll_critical(&instinct_100).0 { crits += 1; }
}
assert!((crit_rate - 0.55).abs() < 0.02); // Testing math, not architecture
```

**Real invariant not tested:**
- Is crit rolled at attack time or resolution time?
- Are crit results stored in QueuedThreat.damage?
- Can target change fate after attack lands?

**What ADR says:** "Crit roll and multiplier determined **at attack time**" (emphasis on timing)
**What test does:** Validates formula produces correct probability distribution

**Architectural value:** ⚠️ **LOW** - This is a unit test for `roll_critical()`, not an invariant test

**Better test would be:**
```rust
// Setup: Attacker with 100 instinct attacks
let (outgoing, was_crit) = calculate_attack_damage(...); // Phase 1: Attack time

// Target gains debuff that reduces their instinct to 0
// This should NOT affect whether attack was crit

// Phase 2: Resolution
let final_damage = apply_passive_modifiers(outgoing, ...);

// Verify crit status unchanged (determined at attack time)
assert_eq!(was_crit, original_crit_status);
```

---

#### Issue 3: Not Testing System Schedule Ordering

**INV-008: Mutual Destruction** ([resources.rs:467-565](../../src/common/systems/combat/resources.rs#L467-L565))

❌ **Problem:** Manually sets health to 0, doesn't test damage→death ordering
```rust
// Manually set health
health_a.state = 0.0;
health_b.state = 0.0;

// Run check_death
app.add_systems(Update, check_death);
app.update();
```

**Real invariant not tested:**
- Does `run-server.rs` schedule damage before death checks?
- If schedule order changes, does test fail?
- Are damage systems actually chained with death checks?

**What ADR says:** "Death checks run AFTER all damage application in same frame"
**What test does:** Verifies `check_death` can emit two events (obviously it can)

**Better test would be:**
```rust
// Setup full damage pipeline
app.add_systems(FixedUpdate, (
    process_expired_threats,  // Emit ResolveThreat
    process_resolve_threat,   // Apply damage
    check_death,              // Check HP <= 0
).chain());

// Queue threats to both entities
reaction_queue_a.push_threat(...);
reaction_queue_b.push_threat(...);

// Run full pipeline
app.update();

// Verify both died (tests actual schedule ordering)
```

**Current test architectural value:** ⚠️ **VERY LOW** - Doesn't test schedule invariant at all

---

**INV-008: Resource Regeneration** ([resources.rs:378-405](../../src/common/systems/combat/resources.rs#L378-L405))

✅ **ACCEPT** - Actually tests architectural invariant
- Tests regen rate functions (actual game rule)
- Simple, clear, tests what matters
- Not testing library behavior

---

### Priority 2 Summary

| Test | Testing | Should Test | Verdict |
|------|---------|-------------|---------|
| INV-004 FIFO | VecDeque behavior | `process_expired_threats` order | ❌ Reject |
| INV-004 Lobby | BiHashMap behavior | Connection handler mapping | ❌ Reject |
| INV-006 Crit | Formula correctness | Attack-time vs resolution-time | ❌ Reject |
| INV-008 Mutual | Event emission | System schedule ordering | ❌ Reject |
| INV-008 Regen | Regen rates | ✅ (correct) | ✅ Accept |

**4 of 5 tests provide minimal architectural value**

---

## Root Cause Analysis

### Why Did This Happen?

1. **ADR-015 Prescribes Wrong Tests**
   - ADR itself shows example tests that validate library behavior
   - Developer followed ADR recommendations exactly
   - Problem is with ADR specification, not implementation

2. **"Invariant" Definition Too Broad**
   - ADR treats "invariants" as "things that should be true"
   - Should be "architectural properties that could break if code changes"
   - Current tests would pass even if architecture violated

3. **Missing "What Could Go Wrong" Thinking**
   - Good test: "If I change X, this test should fail"
   - Current tests: "This library works correctly"
   - Priority 1 tests are good because they'd catch real problems

---

## Recommendations

### 1. Accept Priority 1 As-Is ✅

Priority 1 tests demonstrate correct understanding of architectural testing. Merge these.

### 2. Reject Priority 2 Implementation ❌

Remove tests that validate library behavior:
- `test_reaction_queue_fifo_ordering`
- `test_reaction_queue_preserves_insertion_order`
- `test_lobby_maintains_bidirectional_client_entity_mapping`
- `test_lobby_overwrites_duplicate_client_id`
- `test_crit_roll_probabilities_match_formula`
- `test_crit_multiplier_scales_with_instinct`
- `test_mutual_destruction_both_entities_die`

**Keep only:**
- `test_stamina_regenerates_in_combat`
- `test_mana_regenerates_in_combat`
- `test_health_does_not_regenerate_in_combat`
- `test_health_regenerates_out_of_combat`

### 3. Revise ADR-015 📝

Add section: **"What Makes a Good Invariant Test?"**

```markdown
## Invariant Test Quality Criteria

An architectural invariant test must:

1. **Test System Behavior, Not Libraries**
   ❌ BAD: `assert_eq!(vec.pop_front(), first_item)` // Testing Vec
   ✅ GOOD: `assert!(world_pos_before == world_pos_after)` // Testing our code

2. **Would Fail If Architecture Changed**
   ❌ BAD: Test passes if VecDeque swapped for Vec (not architectural)
   ✅ GOOD: Test fails if Loc update changes coordinate transform (architectural)

3. **Test Integration Points, Not Pure Logic**
   ❌ BAD: `assert_eq!(crit_chance, 0.05)` // Formula correctness
   ✅ GOOD: `assert!(damage_stored_at_attack_time)` // Timing property

4. **Test What Could Actually Break**
   ❌ BAD: "Does standard library work?" (no)
   ✅ GOOD: "Does system maintain invariant under edge case?" (yes)
```

### 4. Replace Priority 2 with Better Tests

**INV-004 (FIFO):** Test `process_expired_threats` processes oldest first
```rust
#[test]
fn test_threat_processing_order_matches_expiry_time() {
    // Insert threats with t=100ms, t=50ms, t=150ms
    // Process at t=200ms
    // Verify t=50ms processed first (oldest expired)
}
```

**INV-004 (Lobby):** Test actual connection handler
```rust
#[test]
fn test_client_disconnect_removes_entity_from_lobby() {
    // Simulate client disconnect event
    // Verify lobby.get_by_left(client_id) returns None
    // Verify lobby.get_by_right(entity) returns None
}
```

**INV-006:** Test timing, not formula
```rust
#[test]
fn test_crit_determined_at_attack_time_not_resolution() {
    // Record crit status at attack time
    // Change attacker instinct before resolution
    // Verify crit status unchanged
}
```

**INV-008:** Test schedule order
```rust
#[test]
fn test_death_check_runs_after_damage_application() {
    // Setup full FixedUpdate schedule
    // Queue lethal damage to both entities
    // Verify both died (schedule order correct)
}
```

---

## Test Coverage Summary

### Current State (After Priority 1+2)
- Total tests added: 27 (Priority 1: 18, Priority 2: 9)
- Architecturally valuable: 19 (Priority 1: 18, Priority 2 regen: 4)
- Library behavior tests: 4 (VecDeque, BiHashMap)
- Formula tests: 2 (crit probability)
- Ineffective integration test: 1 (mutual destruction)
- **Architectural value: 70%** (19/27)

### After Recommended Changes
- Remove 7 low-value tests
- Keep 20 high-value tests
- **Architectural value: 100%** (20/20)
- **Test suite stays fast** (removes 11,000 statistical samples)

---

## Lessons for Future ADRs

1. **Show Counter-Examples in ADRs**
   - "This is NOT an invariant: testing library behavior"
   - Helps developers distinguish architecture from implementation

2. **Test-Driven ADR Writing**
   - Write test first, then decide if it's architectural
   - If test would pass with wrong architecture, it's not an invariant test

3. **Focus on "What Could Break"**
   - Invariant tests should fail when architecture violated
   - Unit tests should fail when logic violated
   - These are different

4. **Prioritize Integration Over Isolation**
   - Test system interactions (schedule order, coordinate transforms)
   - Not isolated functions (formulas, library calls)

---

## Final Verdict

**Priority 1:** ✅ **ACCEPT** - Excellent architectural testing
**Priority 2:** ❌ **REJECT** - Validates libraries, not architecture (except regen tests)

**Recommendation:**
1. Merge Priority 1 tests immediately
2. Keep only INV-008 regen tests from Priority 2
3. Revise ADR-015 with quality criteria
4. Re-implement Priority 2 with proper architectural focus

**Estimated effort to fix:** 4 hours (remove bad tests, write 4 good tests, update ADR)

---

## Approval Status

- [ ] Priority 1 merged to main
- [ ] Priority 2 low-value tests removed
- [ ] ADR-015 revised with test quality criteria
- [ ] Priority 2 re-implemented with architectural focus

**Next steps:** Await user decision on whether to proceed with fixes or accept partial implementation.
