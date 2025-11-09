# SOW-015: Architectural Invariant Testing

## Status

**Proposed** - 2025-11-08

## References

- **RFC-015:** [Architectural Invariant Testing](../01-rfc/015-architectural-invariant-testing.md)
- **Branch:** (proposed)
- **Implementation Time:** 2.5 days

---

## Implementation Plan

### Priority 1: Critical Synchronization Tests

**Goal:** Prevent panics, visual glitches, and invulnerability exploits

**Deliverables:**
- INV-002: InputQueue never empty tests (2 tests)
- INV-003: World-space preservation tests (2 tests)
- INV-007: Armor/resistance cap tests (2 tests)
- INV-005: Damage two-phase timing tests (3 tests)

**Architectural Constraints:**
- Tests located with implementation (Rust convention: same file as code)
- Prefer pure function tests (no ECS setup when possible)
- Clear assertion messages (include expected/actual values)
- Test actual behavior (not standard library correctness)

**INV-002: InputQueue Never Empty**
- File: `src/common/systems/physics.rs`
- Test 1: `test_input_queue_never_empty_invariant()` - Verify queue always has ≥1 input
- Test 2: `test_physics_panics_on_empty_queue()` - Characterization test (expects panic)
- Constraints: Minimal ECS setup, test queue operations directly

**INV-003: World-Space Preservation**
- File: `src/common/systems/world.rs`
- Test 1: `test_world_space_preserved_on_smooth_tile_crossing()` - Adjacent hex (distance < 2)
- Test 2: `test_teleport_clears_offset_for_jumps_over_two_hexes()` - Distance ≥ 2
- Constraints: Pure function test, use Map directly (no ECS)

**INV-007: Armor/Resistance Cap**
- File: `src/common/systems/combat/resources.rs`
- Test 1: `test_armor_caps_at_75_percent()` - Extreme vitality still capped
- Test 2: `test_resistance_caps_at_75_percent()` - Extreme focus still capped
- Constraints: Test with extreme attributes (150 vitality/focus), verify 25% minimum damage

**INV-005: Damage Two-Phase Timing**
- File: `src/common/systems/combat/damage.rs`
- Test 1: `test_outgoing_damage_uses_attacker_attributes_at_attack_time()`
- Test 2: `test_passive_modifiers_use_defender_attributes_at_resolution_time()`
- Test 3: `test_two_phase_timing_handles_attribute_changes_mid_queue()`
- Constraints: Pure function tests, call `calculate_outgoing_damage()` and `apply_passive_modifiers()` directly

**Success Criteria:**
- All 9 tests pass
- Tests prevent regressions (fail if invariant violated)
- Clear failure messages (include formula, expected, actual)
- Fast execution (< 0.1s total for pure function tests)

**Duration:** 1 day

---

### Priority 2: Combat and Network Fairness

**Goal:** Validate fairness guarantees and protocol correctness

**Deliverables:**
- INV-008: Mutual destruction test (1 test)
- INV-004: Entity ID mapping tests (2 tests)
- INV-006: Crit roll timing tests (2 tests)
- INV-009: Heading offset magnitude test (1 test)

**Architectural Constraints:**
- Integration tests acceptable where needed (minimal ECS setup)
- Statistical tests use large samples (10,000) with generous tolerance
- Characterization tests document expected behavior

**INV-008: Mutual Destruction**
- File: `src/common/systems/combat/resources.rs`
- Test: `test_mutual_destruction_both_entities_die()` - Integration test
- Constraints: Minimal ECS setup (2 entities with low health), verify both die in same frame

**INV-004: Entity ID Mapping**
- File: `src/server/systems/renet.rs`
- Test 1: `test_lobby_maintains_bidirectional_client_entity_mapping()`
- Test 2: `test_lobby_overwrites_duplicate_client_id()`
- Constraints: Test BiHashMap directly (no ECS), verify bidirectional lookup

**INV-006: Crit Roll Timing**
- File: `src/common/systems/combat/damage.rs`
- Test 1: `test_crit_roll_probabilities_match_formula()` - Statistical (10,000 samples)
- Test 2: `test_crit_multiplier_scales_with_instinct()` - Formula verification
- Constraints: Generous tolerance (±1-2%), fixed seed optional for determinism

**INV-009: Heading Offset Magnitude**
- File: `src/common/systems/physics.rs`
- Test: `test_heading_based_position_offset_magnitude()` - Verify 0.33 × direction
- Constraints: Pure function test, use Map directly

**Success Criteria:**
- All 6 tests pass
- Statistical tests within tolerance (crit probability ±1-2%)
- Integration tests run fast (< 0.1s each with minimal ECS)
- Bidirectional mapping verified (both directions)

**Duration:** 1 day

---

### Priority 3: Documentation and Edge Cases

**Goal:** Document relationships and boundary conditions

**Deliverables:**
- INV-010: Chunk/FOV relationship test (1 test)
- INV-011: Spatial difficulty edge cases test (1 test)
- INV-013: Proximity radii documentation test (1 test)
- INV-012: Spawn before Incremental test (1 test)

**Architectural Constraints:**
- Characterization tests document expected constants
- Edge case tests verify boundaries (0, 99, 100, 1000+)
- Constants documented in assertions (fail message explains relationship)

**INV-010: Chunk/FOV Relationship**
- File: `src/common/chunk.rs`
- Test: `test_chunk_size_and_fov_relationship()` - Characterization
- Constraints: Document `CHUNK_SIZE = 8`, `FOV_CHUNK_RADIUS = 2`, derive 25 chunks / 1600 tiles

**INV-011: Spatial Difficulty Edge Cases**
- File: `src/common/spatial_difficulty.rs`
- Test: `test_enemy_level_calculation_edge_cases()` - Boundary testing
- Constraints: Test at 0, 99, 100, 500, 1000, 5000 tiles (level thresholds)

**INV-013: Proximity Radii**
- File: `src/server/systems/renet.rs`
- Test: `test_proximity_filtering_radii_documented()` - Characterization
- Constraints: Document Spawn(55), Combat(20), MovementIntent(30), Despawn(70) radii

**INV-012: Spawn Before Incremental**
- File: `src/client/systems/renet.rs`
- Test: `test_client_requests_spawn_for_unknown_entity_on_incremental()` - Integration
- Constraints: Minimal setup, verify self-healing behavior (client requests spawn)

**Success Criteria:**
- All 4 tests pass
- Constants documented in assertions (fail message explains)
- Edge cases covered (boundaries, thresholds)
- Integration test verifies protocol self-healing

**Duration:** 0.5 days

---

## Acceptance Criteria

**Functional:**
- All 19 tests implemented and passing
- Tests prevent regressions (fail if invariants violated)
- Tests located with implementation (same file as code)
- Clear failure messages (include formula, expected, actual)

**Performance:**
- Pure function tests: < 0.01s each
- Integration tests: < 0.1s each
- Total suite: < 2s for all 19 tests

**Code Quality:**
- Tests follow naming convention: `test_{invariant}_{scenario}()`
- Assertions include helpful messages
- Statistical tests use appropriate sample sizes and tolerance
- Characterization tests document constants/relationships

**Documentation:**
- Each test has comment explaining invariant being tested
- Failure messages reference ADR/spec where invariant defined
- Edge cases documented in test names

---

## Discussion

*This section will be populated during implementation with questions, decisions, and deviations.*

### Implementation Note: Test Organization

All tests located in same file as implementation (Rust convention):
- `src/common/systems/physics.rs` - INV-002, INV-009
- `src/common/systems/world.rs` - INV-003
- `src/common/systems/combat/damage.rs` - INV-005, INV-006
- `src/common/systems/combat/resources.rs` - INV-007, INV-008
- `src/server/systems/renet.rs` - INV-004, INV-013
- `src/client/systems/renet.rs` - INV-012
- `src/common/chunk.rs` - INV-010
- `src/common/spatial_difficulty.rs` - INV-011

### Implementation Note: What Makes a Good Invariant Test

Test **system behavior**, not library correctness:
- ❌ BAD: `assert_eq!(vec.pop_front(), first_item)` // Testing VecDeque
- ✅ GOOD: `assert!(world_pos_before == world_pos_after)` // Testing our transform

Test what would **fail if architecture changed**:
- ❌ BAD: Test passes if VecDeque swapped for Vec
- ✅ GOOD: Test fails if Loc update changes coordinate transform

---

## Acceptance Review

*This section will be populated after implementation is complete.*

---

## Sign-Off

**Reviewed By:** (pending)
**Date:** (pending)
**Decision:** (pending)
