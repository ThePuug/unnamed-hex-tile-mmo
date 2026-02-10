# SOW-020: Attribute System Rework

## Status

**In Progress** - 2026-02-10 (Phases 1–4 complete)

## References

- **RFC-020:** [Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- **ADR-026:** [Three Scaling Modes](../02-adr/026-three-scaling-modes.md)
- **ADR-027:** [Commitment Tiers](../02-adr/027-commitment-tiers.md)
- **ADR-028:** [Attribute-Triumvirate Decoupling](../02-adr/028-attribute-triumvirate-decoupling.md)
- **ADR-029:** [Relative Stat Contests](../02-adr/029-relative-stat-contests.md)
- **ADR-020:** [Super-Linear Level Multiplier](../02-adr/020-super-linear-level-multiplier.md)
- **Spec:** [Attribute System Design Doc](../00-spec/attribute-system.md) (v2.0)
- **Branch:** (proposed)
- **Implementation Time:** 16–25 hours

---

## Implementation Plan

### Phase 1: Scaling Mode Foundation

**Goal:** Add CommitmentTier enum and total_budget() on top of the existing ActorAttributes — no struct changes, no changes to Axis/Spectrum/Shift

**Deliverables:**
- `CommitmentTier` enum (T0/T1/T2/T3) with calculation from percentage thresholds
- `total_budget()` method on ActorAttributes (sum of all six derived values)
- `commitment_tier_for()` helper on ActorAttributes
- Tests for CommitmentTier thresholds, total_budget, and helper

**Architectural Constraints:**
- `CommitmentTier` enum: T0 (<30%), T1 (≥30%), T2 (≥45%), T3 (≥60%)
- `total_budget()` = `might() + grace() + vitality() + focus() + instinct() + presence()` (sum of derived values)
- Existing ActorAttributes struct is **unchanged** — 9 `i8` fields (axis/spectrum/shift per pair)
- All existing methods preserved: `.might()`, `.grace()`, `.might_reach()`, `.set_might_grace_shift()`, etc.
- Character panel unchanged
- NPC generation unchanged
- Level multiplier methods use `total_budget()` for level input (same concept as `total_level()`)
- `total_level()` kept as alias for `total_budget()`

**Success Criteria:**
- `commitment_tier(30, 100)` returns T1 (30% threshold)
- `commitment_tier(29, 100)` returns T0
- `commitment_tier(45, 100)` returns T2
- `commitment_tier(60, 100)` returns T3
- `total_budget()` correctly sums all six derived attribute values
- All existing tests pass unchanged
- Character panel works unchanged
- `cargo test` passes

**Duration:** 2–3 hours

**Dependencies:** None (foundation phase)

---

### Phase 2: Absolute Stat Derivation

**Goal:** Wire absolute stats (Force/damage, Constitution/HP) through the existing super-linear level multiplier, using derived attribute values

**Deliverables:**
- Updated `max_health()` to use `vitality()` × `hp_level_multiplier()`
- Updated damage calculation to use `might()` × `damage_level_multiplier()`
- Updated `max_stamina()` formula
- Updated `max_mana()` formula
- Updated `movement_speed()` — Grace-derived
- Updated `calculate_timer_duration()` — Instinct-derived base window
- Preserved level multiplier constants (HP_K=0.10, HP_P=1.5, DAMAGE_K=0.15, DAMAGE_P=2.0, REACTION_K=0.10, REACTION_P=1.2)
- Updated tests for new derivation formulas

**Architectural Constraints:**
- Absolute derivation: `effective_stat = linear_formula(derived_attribute) × level_multiplier(total_budget, k, p)`
- Level multiplier reuses existing ADR-020 constants and `level_multiplier()` method
- HP formula: `base + vitality() × scaling_factor` × hp_multiplier
- Damage formula: `base + might() × scaling_factor` × damage_multiplier
- Movement speed: function of `grace()` derived value
- Timer duration: function of `instinct()` derived value × reaction_multiplier
- All absolute stats should produce reasonable values for level 0 (multiplier = 1.0)

**Success Criteria:**
- Level 0 entity stats unchanged from multiplier perspective (multiplier = 1.0)
- HP scales with Vitality investment and level multiplier
- Damage scales with Might investment and level multiplier
- Movement speed scales with Grace investment
- Timer duration scales with Instinct investment and reaction multiplier
- `cargo test` passes

**Duration:** 3–5 hours

**Dependencies:** Phase 1 (total_budget for level multiplier input)

---

### Phase 3: Commitment-Driven Stats

**Goal:** Wire concrete commitment stats (Concentration → queue capacity, Intensity → cadence, Poise → evasion) to commitment tier calculations

**Deliverables:**
- Updated `calculate_queue_capacity()` to use Focus commitment tier instead of `focus_reach / (total_level × 7)`
- Cadence system: auto-attack interval derived from Presence commitment tier
- Evasion framework: threat avoidance chance derived from Grace commitment tier
- Commitment tier lookup table per concrete stat (T0/T1/T2/T3 → specific values)
- Updated tests for tier-driven queue capacity
- Tests for cadence tier mapping
- Tests for evasion tier mapping

**Architectural Constraints:**
- Queue capacity: T0→1 slot, T1→2, T2→3, T3→4 (same output as ADR-021, different input)
- Cadence (auto-attack interval): specific values per tier are tuning knobs (e.g., T0=2.0s, T1=1.5s, T2=1.0s, T3=0.75s — adjust through playtesting)
- Evasion (dodge chance): specific values per tier are tuning knobs (e.g., T0=0%, T1=10%, T2=20%, T3=30% — adjust through playtesting)
- `calculate_queue_capacity` input changes from `ActorAttributes` (using focus_reach/total_level) to `CommitmentTier` (Focus commitment tier)
- Cadence replaces fixed auto-attack cooldown in `LastAutoAttack` system
- Evasion check runs at threat insertion time (before adding to queue)

**Success Criteria:**
- Focus T0 entity gets 1 queue slot
- Focus T1 entity gets 2 queue slots
- Focus T2 entity gets 3 queue slots
- Focus T3 entity gets 4 queue slots
- Auto-attack interval varies by Presence commitment tier
- Evasion chance varies by Grace commitment tier (may be 0/0/0/0 for MVP — values TBD)
- Boundary tests: 29.9% → T0, 30.0% → T1 for all commitment-driven stats
- `cargo test` passes

**Duration:** 4–6 hours

**Dependencies:** Phase 1 (commitment tier calculation)

---

### Phase 4: Relative Stat Contest Framework

**Goal:** Implement the three relative stat contest pairs for combat resolution

**Deliverables:**
- Precision vs Toughness contest: `grace()` vs `vitality()` affecting crit chance and mitigation on unmitigated threats
- Dominance vs Cunning contest: `presence()` vs `instinct()` affecting recovery pushback and reaction window duration
- Composure effect: `focus()` relative stat reduces recovery duration (contests Dominance pushback)
- Contest resolution function: `f(attacker_stat - defender_stat) -> modifier`
- Integration with reaction queue timer insertion (Cunning affects window)
- Integration with recovery/lockout system (Dominance pushback, Composure reduction)
- Integration with damage resolution (Precision vs Toughness on dismissed/expired threats)
- Tests for each contest pair across stat ranges

**Architectural Constraints:**
- Relative stats use derived attribute values (no level multiplier applied)
- Contest function: monotonic function of `attacker_stat - defender_stat` — exact shape (linear, sigmoid, clamped) is a tuning knob
- Precision vs Toughness: modifies crit chance and damage mitigation percentage
- Dominance vs Cunning: modifies reaction window duration at threat insertion; modifies recovery pushback on hit
- Composure: reduces effective recovery duration (directly contests Dominance pushback)
- Impact (Might relative) is intentionally left open — no mechanic assigned yet
- All contest modifiers default to 1.0 (neutral) when attacker stat equals defender stat
- Server-authoritative — contests resolved on server, results broadcast

**Success Criteria:**
- Equal raw stats produce neutral modifier (1.0 or equivalent)
- Higher attacker stat produces advantage (modifier > 1.0)
- Higher defender stat produces defense (modifier < 1.0 for attacker)
- Reaction window visibly affected by Cunning vs Dominance contest
- Recovery duration visibly affected by Composure vs Dominance
- Dismissed threats subject to Precision vs Toughness for crit/mitigation
- Impact (Might relative) has no mechanical effect (documented as open)
- `cargo test` passes

**Duration:** 4–6 hours

**Dependencies:** Phase 1 (derived attribute values), Phase 2 (absolute stat context)

---

### Phase 5: Decoupling and Integration

**Goal:** Remove archetype-attribute coupling, migrate NPC configurations to data-driven spreads, comprehensive integration tests

**Deliverables:**
- NPC attribute spreads as direct data configuration (not derived from Triumvirate type)
- Updated engagement spawner NPC attribute assignment
- Updated spatial difficulty NPC attribute scaling
- Removal of archetype-to-attribute mapping tables (if any exist in code)
- Comprehensive integration tests verifying end-to-end attribute flow
- Updated or removed tests that assert archetype-attribute coupling

**Architectural Constraints:**
- NPC attribute values are data-driven constants, not derived from Triumvirate Approach/Resilience
- Existing NPC combat behavior should be preserved (same effective stats, different configuration approach)
- Spatial difficulty: NPC level determines total budget, attribute spread is per-archetype data
- Axis/Spectrum/Shift model, character panel, and all existing methods remain unchanged
- No regressions in combat flow, movement, or resource management

**Success Criteria:**
- NPCs spawn with correct attribute values (matching pre-migration effective stats where possible)
- NPC attribute spreads not derived from Triumvirate type (data-configured)
- Full `cargo test` passes
- `cargo build` compiles cleanly
- Character panel works unchanged

**Duration:** 3–5 hours

**Dependencies:** Phases 1–4 (all other phases complete)

---

## Acceptance Criteria

**Functional:**
- Three scaling modes produce correct values (absolute × level multiplier, relative raw comparison, commitment tier from percentage)
- Commitment tiers at 30/45/60% thresholds
- Queue capacity driven by Focus commitment tier
- Cadence driven by Presence commitment tier
- Reaction window affected by Cunning vs Dominance relative contest
- Recovery pushback/reduction via Dominance/Composure relative contest
- Crit/mitigation via Precision/Toughness relative contest
- Attributes fully decoupled from Triumvirate
- Open stats (Ferocity, Grit, Flow, Impact, Technique, Discipline, Intuition, Gravitas) documented but have no mechanical effect

**UX:**
- Level progression feels meaningful through absolute stat scaling
- Build identity visible through commitment tier effects (queue slots, attack speed, evasion)
- Relative matchups create perceivable combat differences
- No regressions in existing combat flow
- Character panel unchanged — bipolar bars, shift drag, spectrum/reach all preserved

**Performance:**
- Commitment tier: computed at spawn/level-up, not per frame
- Absolute derivation: computed at spawn/level-up, not per frame
- Relative contests: one subtraction per combat event per pair — negligible
- No data model change — existing struct preserved

**Code Quality:**
- CommitmentTier enum with clear tier boundaries
- Pure functions for tier calculation, absolute derivation, and contest resolution
- All commitment tier thresholds as named constants
- Open stats documented as empty in code comments
- Comprehensive tests for tier boundaries, absolute formulas, and relative contests

---

## Risks

### 1. NPC Balance Regression

**Risk:** Decoupling archetype-attribute assignment may shift NPC effective stats, altering combat balance.

**Mitigation:** Phase 5 explicitly maps pre-migration effective stats to new model. Integration tests verify stat equivalence. Playtesting validates combat feel.

**Impact:** Medium — requires careful migration math.

### 2. Open Stats Create Incomplete System

**Risk:** Many sub-attributes have no concrete mechanic. System may feel incomplete.

**Mitigation:** Explicitly documented as open design space. System is fully functional with the concrete stats (Force, Constitution, Precision, Toughness, Dominance, Cunning, Composure, Poise, Concentration, Intensity). Open stats fill in through playtesting.

**Impact:** Low — design intent, not technical risk.

### 3. Commitment Tier Cliff Effects

**Risk:** 29% vs 30% investment creates binary identity unlock.

**Mitigation:** Intentional design — discrete tiers are clearer to players than continuous scaling. 30/45/60 thresholds (not 33/50/66) provide buffer.

**Impact:** None — by design.

---

## Discussion

*This section will be populated during implementation with questions, decisions, and deviations.*

---

## Acceptance Review

*This section will be populated after implementation is complete.*

---

## Sign-Off

**Reviewed By:** —
**Date:** —
**Decision:** —
