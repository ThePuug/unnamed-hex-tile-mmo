# SOW-013: Ability Execution Pipeline

## Status

**Proposed** - 2025-11-07

## References

- **RFC-013:** [Ability Execution Pipeline (Reliable Ability System)](../01-rfc/013-ability-execution-pipeline.md)
- **ADR-018:** [Three-Stage Ability Execution Pipeline with Pure Function Extraction](../02-adr/018-ability-execution-pipeline-architecture.md)
- **Branch:** (proposed)
- **Implementation Time:** 3-4 days

---

## Implementation Plan

### Phase 1: Infrastructure Skeleton

**Goal:** Create pipeline infrastructure without breaking existing abilities

**Deliverables:**
- `ability_outcome.rs` in `common/components/` (data structures)
- `ability_pipeline.rs` in `server/systems/combat/` (pipeline systems)
- Event definitions: `ValidatedAbilityUse`, `AbilityOutcome`
- `AbilityEffect` enum with MVP effects
- Pipeline system skeleton (validator + broadcaster)
- System registration in CombatPlugin
- Integration tests for pipeline plumbing

**Architectural Constraints:**
- Events must be defined in `common/` (shared between client/server eventually)
- `AbilityEffect` enum must be extensible (easy to add new effects)
- MVP effects: `ConsumeStamina`, `Damage`, `Teleport`, `Push`, `ClearQueue`, `MutateQueue`
- Pipeline systems run AFTER ability input processing, BEFORE network broadcasting
- Pipeline systems idle initially (no abilities using them yet)
- Existing abilities continue working (parallel implementation, not replacement yet)
- System execution order: `ability_pipeline_system` → observers (parallel) → `outcome_broadcaster_system`

**Success Criteria:**
- Project compiles with new pipeline infrastructure
- Pipeline systems registered and running (idle, no events processed)
- Existing abilities still work (no regressions)
- Events flow correctly (send ValidatedAbilityUse → receive empty, send AbilityOutcome → broadcast empty)
- Integration test: Send ValidatedAbilityUse → verify broadcaster sees it

**Duration:** 1 day

---

### Phase 2: Pure Function Extraction

**Goal:** Extract ability logic into testable pure functions

**Deliverables:**
- `validate_lunge()` pure function in `abilities/lunge.rs`
- `validate_overpower()` pure function in `abilities/overpower.rs`
- `validate_knockback()` pure function in `abilities/knockback.rs`
- `validate_deflect()` pure function in `abilities/deflect.rs`
- Unit tests for each pure function (comprehensive edge cases)
- Data structures for ability-specific data (LungeData, OverpowerData, etc.)

**Architectural Constraints:**
- Pure functions take primitive data (no ECS queries)
- Function signature pattern:
  ```
  validate_{ability}(
      caster_loc: Qrz,
      caster_stamina: f32,
      target_ent: Option<Entity>,
      target_loc: Qrz,
      tier_lock: Option<RangeTier>,
      ...
  ) -> Result<{Ability}Data, AbilityFailReason>
  ```
- Return types: `LungeData { target_ent, target_loc, stamina_cost, damage }`, etc.
- Fail reasons: `OutOfRange`, `InsufficientResource`, `NoTargets`, `InvalidTarget`
- Pure functions must NOT mutate state (no side effects)
- Pure functions must be deterministic (same inputs = same output)
- Keep pure functions in same file as ability system (easy to find)
- Data structures implement `to_effects() -> Vec<AbilityEffect>` conversion

**Success Criteria:**
- All 4 pure functions compile and return correct data
- Unit tests cover success cases (valid inputs → Ok(Data))
- Unit tests cover all fail reasons (out of range, insufficient stamina, no targets)
- Unit tests run in < 0.01s each (no ECS overhead)
- Pure functions callable from any context (no ECS dependency)

**Duration:** 1 day

---

### Phase 3: Migrate Abilities to Pipeline

**Goal:** Refactor all 4 abilities to use pipeline pattern

**Deliverables:**
- `lunge_ability_system` refactored to observer pattern
- `overpower_ability_system` refactored to observer pattern
- `knockback_ability_system` refactored to observer pattern
- `deflect_ability_system` refactored to observer pattern
- Remove old ability systems (direct Try::UseAbility handlers)
- Update ability_pipeline_system to validate universal rules
- Update outcome_broadcaster_system to apply effects
- System tests for pipeline integration

**Architectural Constraints:**
- Observer systems filter `ValidatedAbilityUse` by ability type
- Observer pattern:
  1. Extract ECS data via queries
  2. Call pure function with extracted data
  3. Convert result to effects (Ok → Success, Err → Failed)
  4. Emit `AbilityOutcome`
- Broadcaster applies effects in order:
  - `ConsumeStamina`: Mutate Stamina component, broadcast Incremental
  - `Damage`: Emit DealDamage event (ADR-005 pipeline)
  - `Teleport/Push`: Insert Loc component, broadcast Incremental
  - `ClearQueue/MutateQueue`: Mutate ReactionQueue, broadcast event
- On success: Broadcast `Do::UseAbility`, insert GlobalRecovery, apply synergies (ADR-012)
- Universal validation checks: death (RespawnTimer), recovery (GlobalRecovery + SynergyUnlock), GCD
- Migration order: Lunge → Overpower → Knockback → Deflect (simple to complex)
- Run test suite after each ability migration (detect regressions early)

**Success Criteria:**
- All 4 abilities work identically to before (no behavior changes)
- Old ability systems removed (no duplicates)
- Universal validation blocks dead/recovering/GCD-locked casters
- Effects applied correctly (stamina consumed, damage dealt, Loc updated, queue mutated)
- Network broadcasting works (Incremental updates, UseAbility events)
- Recovery/synergy applied correctly (GlobalRecovery inserted, synergies triggered)
- Test suite passes (no regressions)

**Duration:** 1 day

---

### Phase 4: Cleanup and Optimization

**Goal:** Refine implementation and extract shared logic

**Deliverables:**
- Shared helper functions extracted to `ability_pipeline.rs`
- Legacy GCD system removed (if not already done by ADR-012)
- Performance profiling (before/after comparison)
- Documentation updates (code comments, system flow diagram)
- Final integration test suite

**Architectural Constraints:**
- Shared helpers:
  - `validate_tier_locked_target(caster, tier_lock, nntree, query) -> Option<Entity>`
  - `validate_range(caster: Qrz, target: Qrz, min: u32, max: u32) -> bool`
  - `is_target_valid(target: Entity, respawn_query: &Query<...>) -> bool`
- Helpers defined in `ability_pipeline.rs` (shared module)
- Pure functions call helpers (keep logic DRY)
- Performance target: Event overhead < 0.1ms per ability use
- Documentation: System flow diagram, event lifecycle, pure function testing guide

**Success Criteria:**
- Shared helpers used by multiple abilities (no duplication)
- Legacy GCD system removed (if applicable)
- Performance profiling shows acceptable overhead (< 0.1ms per ability)
- Code comments explain pipeline stages
- Integration tests cover full pipeline flow (Try → Validated → Outcome → Effects)
- Final test suite passes (all abilities working)

**Duration:** 0.5 days

---

## Acceptance Criteria

**Functional:**
- Universal validation blocks dead/recovering/GCD-locked casters
- All 4 abilities work identically to before (no regressions)
- Pure functions return correct data for all edge cases
- Broadcaster applies effects correctly (stamina, Loc, queue, recovery)
- Network broadcasting works (Incremental updates, UseAbility events)

**UX:**
- Abilities behave consistently (same validation logic for all)
- Clear failure reasons (out of range, insufficient stamina)
- No perceived latency increase (event overhead < 0.1ms)

**Performance:**
- Event overhead < 0.1ms per ability use
- Pure function tests run in < 0.01s each
- Bevy parallelism maintained (observers run concurrently)
- No performance regression from current implementation

**Code Quality:**
- ~350 lines eliminated (duplication removed)
- Pure functions have comprehensive unit tests (all edge cases)
- Systems thin and focused (< 50 lines each)
- Shared helpers extracted (no duplication)
- Clear separation: validation → execution → broadcasting

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
