# SOW-017: Combat Balance Implementation

## Status

**Merged** - 2026-02-09

## References

- **RFC-017:** [Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md)
- **ADR-020:** [Super-Linear Level Multiplier](../02-adr/020-super-linear-level-multiplier.md)
- **ADR-021:** [Commitment-Ratio Queue Capacity](../02-adr/021-commitment-ratio-queue-capacity.md)
- **ADR-022:** [Dismiss Mechanic](../02-adr/022-dismiss-mechanic.md)
- **Spec:** [Combat Balance Design Doc](../00-spec/combat-balance.md)
- **Branch:** main (merged)
- **Implementation Time:** 8-12 hours

---

## Implementation Plan

### Phase 1: Super-Linear Level Multiplier

**Goal:** Replace linear stat derivation with polynomial level-multiplied stats for HP, damage, and reaction-related attributes

**Deliverables:**
- `level_multiplier(level, k, p)` pure function in `resources.rs`
- Named constants for each stat category (HP_K, HP_P, DAMAGE_K, DAMAGE_P, REACTION_K, REACTION_P)
- Integration into existing stat derivation pipeline (applied after linear formula)
- Updated tests with multiplied expected values
- New tests for multiplier edge cases (level 0 = 1.0, high levels)

**Architectural Constraints:**
- Multiplier applied AFTER existing linear formula (linear formula unchanged)
- Level 0 multiplier must equal exactly 1.0 for all stat categories (backward compatibility)
- Constants isolated as named `const` values (not inline magic numbers)
- Pure function: `fn level_multiplier(level: u8, k: f32, p: f32) -> f32`
- HP constants: k=0.10, p=1.5
- Damage constants: k=0.15, p=2.0
- Reaction constants: k=0.10, p=1.2

**Success Criteria:**
- Level 0 entities have unchanged stats (multiplier = 1.0)
- Level 10 HP multiplier = 2.83 (within f32 tolerance)
- Level 10 damage multiplier = 6.25 (within f32 tolerance)
- Level 5 HP multiplier = 1.84 (within f32 tolerance)
- Existing stat derivation tests updated to account for multiplier
- New unit tests for `level_multiplier` function across level range 0-10
- `cargo test` passes

**Duration:** 2-3 hours

**Dependencies:** None (standalone)

---

### Phase 2: Commitment-Ratio Queue Capacity

**Goal:** Replace raw-Focus queue capacity with investment ratio threshold system

**Deliverables:**
- Updated `calculate_queue_capacity` function in `queue.rs`
- Named threshold constants (QUEUE_THRESHOLD_1, QUEUE_THRESHOLD_2, QUEUE_THRESHOLD_3)
- Normalizing constant for attribute count
- Updated tests with ratio-based expected values
- New tests for threshold boundaries and edge cases

**Architectural Constraints:**
- Formula: `commitment_ratio = focus_reach / (total_level × 7)`
- Thresholds: <33% → 1, 33-49% → 2, 50-65% → 3, ≥66% → 4
- Level 0 special case: always returns 1 slot (division by zero guard)
- Function signature change: needs `focus_reach` and `total_level` as inputs
- Output unchanged: returns `u8` slot count
- Threshold constants isolated as named `const` values

**Success Criteria:**
- Level 0 entities always get 1 queue slot
- Focus 0 at any level → 1 slot
- Threshold boundaries tested: 32.9% → 1 slot, 33.0% → 2 slots
- Threshold boundaries tested: 49.9% → 2 slots, 50.0% → 3 slots
- Threshold boundaries tested: 65.9% → 3 slots, 66.0% → 4 slots
- NPC archetypes from RFC-014 get expected queue capacities
- Existing queue capacity tests updated
- `cargo test` passes

**Duration:** 2-3 hours

**Dependencies:** None (standalone)

---

### Phase 3: Reaction Window Level Gap Scaling

**Goal:** Apply level gap modifier to reaction window durations so higher-level defenders get more time against weaker threats

**Deliverables:**
- `gap_multiplier(defender_level, attacker_level)` pure function
- Named constants for scaling factor and max multiplier
- Integration into reaction timer insertion logic
- New tests for gap multiplier across level combinations

**Architectural Constraints:**
- Formula: `gap_multiplier = clamp(1.0 + max(0, defender_level - attacker_level) × 0.15, 1.0, 3.0)`
- Applied when creating QueueEntry timer duration (at threat insertion time)
- Defender below attacker: multiplier floors at 1.0 (no penalty)
- Maximum multiplier: 3.0 (cap prevents infinite windows)
- Scaling factor: 0.15 per level gap (15% more time per level advantage)
- Constants: WINDOW_SCALING_FACTOR = 0.15, WINDOW_MAX_MULTIPLIER = 3.0
- Requires access to both defender and attacker level at insertion time

**Success Criteria:**
- Equal-level combat: multiplier = 1.0 (unchanged behavior)
- Defender 10 levels above: multiplier = 2.5
- Defender 5 levels above: multiplier = 1.75
- Defender below attacker: multiplier = 1.0 (no penalty)
- Multiplier capped at 3.0 (even for extreme gaps)
- Reaction window with gap matches worked examples in design doc
- `cargo test` passes

**Duration:** 1-2 hours

**Dependencies:** Phase 1 (multiplier infrastructure and constant patterns established)

---

### Phase 4: Dismiss Mechanic

**Goal:** Add dismiss verb for queue management — skip front threat at full unmitigated damage with no lockout

**Deliverables:**
- `Try::Dismiss { ent: Entity }` message variant in `message.rs`
- Server-side dismiss handler in reaction queue system
- Client-side input handling and key binding
- Basic UI feedback (threat removal visual)
- Tests for dismiss behavior and edge cases

**Architectural Constraints:**
- Dismiss resolves FRONT threat only (FIFO consistent with ADR-006)
- Damage applied WITHOUT armor/resistance mitigation (full unmitigated)
- No GlobalRecovery component created (does not trigger lockout)
- No GCD interaction (always available regardless of ability state)
- No resource cost (stamina/mana)
- Server validates: entity has ReactionQueue with ≥1 entry
- Empty queue dismiss: fail silently or show feedback (no error state)
- Network message: `Try::Dismiss { ent: Entity }` where ent is the dismissing entity
- Server broadcasts queue update and damage event after dismiss

**Success Criteria:**
- Dismiss removes front threat from queue
- Dismissed threat deals full unmitigated damage to entity
- Dismiss does not create GlobalRecovery (no lockout)
- Dismiss usable during GlobalRecovery lockout (from other abilities)
- Dismiss usable immediately after another dismiss (no cooldown)
- Empty queue dismiss fails gracefully (no crash, no error state)
- Client sends correct message on key press
- Server broadcasts queue update after dismiss
- Damage event visible in UI after dismiss
- `cargo test` passes

**Duration:** 3-4 hours

**Dependencies:** Phase 2 (understanding of queue mechanics and capacity system)

---

## Acceptance Criteria

**Functional:**
- Super-linear multiplier applied to HP, damage, and reaction stats
- Level 0 behavior unchanged (multiplier = 1.0)
- Queue capacity based on commitment ratio with correct thresholds
- Reaction windows widened by level gap (defender advantage)
- Dismiss mechanic resolves front threat at full unmitigated damage
- Dismiss has no lockout, no GCD, no resource cost

**UX:**
- Level progression feels meaningful (visible stat growth)
- Queue capacity reflects build investment (Focus specialists get more slots)
- Fighting weaker enemies feels manageable (wider reaction windows)
- Dismiss is intuitive and provides visible feedback
- No regressions in existing combat flow

**Performance:**
- Level multiplier: negligible overhead (one multiply per stat, runs on spawn/level-up)
- Queue capacity: negligible overhead (one division + compare per capacity check)
- Gap multiplier: negligible overhead (one subtract + multiply per threat insertion)
- Dismiss: same cost as timer expiry (one queue pop)
- No per-frame overhead added by any system

**Code Quality:**
- All constants named and isolated (no magic numbers)
- Pure functions for multiplier, gap multiplier, and capacity calculation
- Comprehensive unit tests for each formula with edge cases
- Worked example values from design doc verified in tests
- Existing tests updated to account for new formulas

---

## Discussion

*This section will be populated during implementation with questions, decisions, and deviations.*

### Implementation Note: Phase Dependencies

Phases 1 and 2 are fully independent and can be implemented in any order or in parallel. Phase 3 depends on Phase 1 for the multiplier infrastructure and constant naming patterns. Phase 4 depends on Phase 2 for queue mechanics understanding but is architecturally independent.

Recommended order: Phase 1 → Phase 2 → Phase 3 → Phase 4 (linear), or Phase 1 + Phase 2 (parallel) → Phase 3 → Phase 4.

---

## Acceptance Review

### Scope Completion: 100%

**Phases Complete:**
- ✅ Phase 1: Super-Linear Level Multiplier
- ✅ Phase 2: Commitment-Ratio Queue Capacity
- ✅ Phase 3: Reaction Window Level Gap Scaling
- ✅ Phase 4: Dismiss Mechanic

### Architectural Compliance

All four systems implemented per ADR-020, ADR-021, ADR-022 specifications:
- Level multiplier pure function with named constants per stat category
- Commitment-ratio threshold formula with division-by-zero guard
- Gap multiplier applied at threat insertion time with floor and cap
- Dismiss message variant, server handler, client input binding — no lockout, no GCD

### Code Quality

- Constants isolated as named `const` values
- Pure functions for level_multiplier, gap_multiplier, calculate_queue_capacity
- Existing tests updated for new formulas

---

## Sign-Off

**Reviewed By:** ARCHITECT
**Date:** 2026-02-09
**Decision:** ✅ **ACCEPTED** — Merged to main
