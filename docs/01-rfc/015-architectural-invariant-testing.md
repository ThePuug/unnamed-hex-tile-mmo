# RFC-015: Architectural Invariant Testing

## Status

**Approved** - 2025-11-08

## Feature Request

### Player Need

From player perspective: **Reliable game systems that don't break during updates** - Core mechanics should work consistently without regressions when new features are added.

**Current Problem:**
Without comprehensive invariant testing:
- Critical architectural guarantees enforced by assertions but not tested (runtime checks, no regression prevention)
- Systems can break during refactoring without early detection (silent failures possible)
- New developers risk breaking invariants they don't know exist (undocumented assumptions)
- Regression bugs slip into production (no safety net for core mechanics)

**We need a system that:**
- Tests critical architectural invariants (17 identified during review)
- Prevents regressions during refactoring (catch breaking changes early)
- Documents system correctness guarantees (executable documentation)
- Enables confident refactoring (developers know what must stay true)

### Desired Experience

Players should experience:
- **Reliability:** Core systems work consistently (no surprise breakage)
- **Stability:** Updates don't introduce regressions in combat/movement/networking
- **Polish:** Edge cases handled correctly (no falling through terrain, invulnerability exploits)
- **Fairness:** Combat mechanics work as specified (damage timing, mutual destruction)

### Specification Requirements

**Critical Invariants to Test (17 total):**

**Category 1: Client-Server Synchronization (4 invariants)**
1. **Chunk Eviction Parity:** Client and server use identical `FOV_CHUNK_RADIUS + 1` for eviction
2. **InputQueue Never Empty:** All InputQueues contain ≥1 input (physics asserts this)
3. **World-Space Preservation:** Tile crossings preserve world position, teleports clear offset
4. **Entity ID Mapping:** Lobby bimap maintains 1:1 `ClientId ↔ Entity` mapping

**Category 2: Combat System Fairness (4 invariants)**
5. **Damage Two-Phase Timing:** Outgoing at attack time, incoming at resolution time
6. **Critical Hit Roll:** Crit determined at attack time, can't change after
7. **Armor/Resistance Cap:** Hard cap at 75% damage reduction (no invulnerability)
8. **Mutual Destruction:** Death checks after all damage in same frame (both can die)

**Category 3: Spatial and Movement (3 invariants)**
9. **Heading Offset Magnitude:** Non-default heading offsets by 0.33 units toward neighbor
10. **Chunk/FOV Relationship:** `CHUNK_SIZE = 8`, `FOV_CHUNK_RADIUS = 2` (25 chunks, 1600 tiles)
11. **Spatial Difficulty Scaling:** Enemy level = `⌊distance / 100⌋`, clamped [0, 10]

**Category 4: Network Protocol (4 invariants)**
12. **Spawn Before Incremental:** Client receives Spawn before Incremental for entity
13. **Proximity Filtering Radii:** Different event types broadcast at tuned radii

**Test Quality Criteria:**
- Test system behavior (not library correctness)
- Would fail if architecture changed incorrectly
- Test integration points (not pure logic)
- Test what could actually break (not standard library)

### MVP Scope

**Priority 1 (Critical - 1 day):**
- INV-002: InputQueue never empty (prevents movement crashes)
- INV-003: World-space preservation (prevents terrain glitches)
- INV-007: Armor/resistance cap (prevents invulnerability exploits)
- INV-005: Damage two-phase timing (ensures combat fairness)

**Priority 2 (Important - 1 day):**
- INV-008: Mutual destruction (spec compliance)
- INV-004: Entity ID mapping (prevents server panics)
- INV-006: Crit roll timing (enhances existing coverage)
- INV-009: Heading offset magnitude (movement regression prevention)

**Priority 3 (Documentation - 0.5 day):**
- INV-010: Chunk/FOV relationship (characterization)
- INV-011: Spatial difficulty edge cases (boundary testing)
- INV-013: Proximity radii (documentation)
- INV-012: Spawn before Incremental (integration test)

### Priority Justification

**MEDIUM PRIORITY** - Quality improvement that enables confident refactoring.

**Why medium priority:**
- Not a player-facing feature (no new content)
- Prevents regressions (enables future work safely)
- Documents correctness guarantees (onboarding, maintenance)
- Some invariants already have assertions (partial coverage exists)

**Benefits:**
- Regression prevention (tests catch breaking changes)
- Confident refactoring (safety net for changes)
- Executable documentation (tests show how systems must work)
- Faster debugging (failing test pinpoints issue)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Unit Tests for Critical Invariants**

#### Core Mechanism

**Test Categories:**
1. **Pure function tests:** No ECS setup, test logic directly (fast)
2. **Integration tests:** Minimal ECS setup, test system interactions (slower)
3. **Characterization tests:** Document current behavior (regression detection)

**Example (World-Space Preservation):**
```rust
#[test]
fn test_world_space_preserved_on_smooth_tile_crossing() {
    let map = Map::new(qrz::Map::new(1.0, 0.8));
    let old_loc = Qrz::new(5, 5);
    let new_loc = Qrz::new(6, 5);  // Adjacent (distance = 1)
    let old_offset = Vec3::new(0.5, 0.0, 0.3);

    let world_pos_before = map.convert(old_loc) + old_offset;

    // Apply preservation formula
    let state_world = map.convert(old_loc) + old_offset;
    let new_offset = state_world - map.convert(new_loc);

    let world_pos_after = map.convert(new_loc) + new_offset;

    assert!((world_pos_before - world_pos_after).length() < 0.001);
}
```

**Test Quality Guidelines:**
- Focus on timing (attack vs resolution)
- Focus on boundaries (coordinate transforms, caps)
- Focus on integration (how systems interact)
- Avoid testing standard library (VecDeque is FIFO)
- Prefer pure functions (avoid heavy ECS setup)

#### Performance Projections

**Test Execution:**
- Pure function tests: < 0.01s each
- Integration tests: < 0.1s each
- Total suite: < 2s for all 19 new tests

**Development Time:**
- Priority 1: 1 day (4 invariants, 9 tests)
- Priority 2: 1 day (4 invariants, 6 tests)
- Priority 3: 0.5 day (4 invariants, 4 tests)
- **Total: 2.5 days**

#### Technical Risks

**1. Test Maintenance Burden**
- *Risk:* Tests must be updated when invariants change
- *Mitigation:* Good test names/comments make intent clear
- *Impact:* Minor ongoing cost, worth it for regression prevention

**2. False Security**
- *Risk:* Tests document current invariants, new ones may emerge
- *Mitigation:* Regular architectural reviews to identify new invariants
- *Impact:* Acceptable - tests prevent known regressions

**3. Statistical Test Flakiness**
- *Risk:* Probability tests (crit roll) might have random failures
- *Mitigation:* Use generous tolerance, large sample sizes
- *Impact:* Low - proper statistical testing handles variance

### System Integration

**Affected Systems:**
- Physics (InputQueue, world-space preservation, heading offset)
- Combat (damage timing, crit roll, armor cap, mutual destruction)
- Networking (entity mapping, spawn ordering, proximity radii)
- Spatial (chunk/FOV, enemy level scaling)

**Compatibility:**
- ✅ Tests use existing systems (no new code needed)
- ✅ Tests located with implementation (Rust convention)
- ✅ Minimal ECS setup (prefer pure function tests)

### Alternatives Considered

#### Alternative 1: Integration Tests Only

Test full systems end-to-end.

**Rejected because:**
- Slow (full ECS setup for every test)
- Hard to isolate failures (which invariant broke?)
- Over-testing (standard library, not our logic)

#### Alternative 2: Property-Based Testing

Use QuickCheck/proptest for generative testing.

**Rejected for MVP because:**
- Overkill for deterministic invariants
- Adds dependency and complexity
- Can add later for specific systems (RNG, formulas)

#### Alternative 3: Do Nothing

Trust assertions and manual testing.

**Rejected because:**
- Assertions catch failures in production (too late)
- Manual testing doesn't scale (17 invariants × edge cases)
- No regression prevention (refactors break silently)

---

## Discussion

### ARCHITECT Notes

**Key Insight:** Testing invariants is different from testing features. Invariant tests document "what must always be true" about system architecture.

**Quality over quantity:** 19 well-designed invariant tests more valuable than 100 feature tests. These tests protect the architectural foundation.

**Regression prevention is primary goal:** Not finding new bugs (assertions do that), but preventing old bugs from returning during refactors.

### PLAYER Validation

**Player-Facing Benefits (Indirect):**
- Fewer regression bugs in updates (stability)
- Core systems work reliably (combat, movement, networking)
- Edge cases handled correctly (no exploits, glitches)

**Development Velocity:**
- Confident refactoring (safety net)
- Faster debugging (failing test pinpoints issue)
- Better onboarding (tests document how systems work)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Comprehensive coverage of critical invariants, good quality criteria
- DEVELOPER: ✅ Reasonable effort (2.5 days), valuable regression prevention

**Scope Constraint:** Fits in one SOW (2.5 days for 3 priority tiers)

**Dependencies:**
- Existing systems (all invariants already implemented)
- Rust test framework (built-in)

**Next Steps:**
1. ARCHITECT creates SOW-015 with 3-priority implementation plan
2. DEVELOPER begins Priority 1 (critical synchronization)

**Date:** 2025-11-08
