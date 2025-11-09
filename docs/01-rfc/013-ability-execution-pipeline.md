# RFC-013: Ability Execution Pipeline (Reliable Ability System)

## Status

**Proposed** - 2025-11-07

## Feature Request

### Player Need

From player perspective: **Reliable and consistent ability behavior** - Abilities should work predictably every time, and new abilities should ship quickly without introducing bugs.

**Current Problem:**
Without a centralized execution pipeline:
- Ability bugs vary by ability (inconsistent error handling between Lunge/Overpower/Knockback/Deflect)
- Adding universal mechanics (stun, silence) requires changing 7+ files (error-prone, easy to miss one)
- Testing requires full ECS setup (slow iteration, hard to verify edge cases)
- Code duplication (~60 lines per ability) increases maintenance burden and bug risk
- New abilities take longer to implement (copy-paste boilerplate, risk introducing bugs)

**We need a system that:**
- Ensures abilities behave consistently (same validation logic for all)
- Enables comprehensive testing (verify edge cases without ECS overhead)
- Accelerates ability development (less boilerplate, focus on unique mechanics)
- Makes bugs easier to fix (single source of truth for validation)
- Supports future mechanics (stun, silence, interrupt) cleanly

### Desired Experience

Players should experience:
- **Reliability:** Abilities work the same way every time (no edge case bugs)
- **Consistency:** All abilities respect same rules (death check, recovery, resource costs)
- **Predictability:** Clear feedback when ability fails (always get reason)
- **Polish:** New abilities ship without regression bugs (thorough testing)
- **Variety:** New abilities arrive faster (streamlined implementation)

### Specification Requirements

**MVP Pipeline Architecture:**

**1. Universal Validation Stage:**
- Single system checks universal rules (death, recovery, GCD)
- Runs before ability-specific logic
- Emits `ValidatedAbilityUse` OR broadcasts `AbilityFailed` with reason
- All abilities guaranteed to respect universal rules (no per-ability duplication)

**2. Ability Observer Stage:**
- Each ability listens for `ValidatedAbilityUse` (filtered by ability type)
- Calls pure validation function (no ECS queries)
- Returns structured outcome: `Result<AbilityData, FailReason>`
- Converts outcome to effects: `Vec<AbilityEffect>`
- Emits `AbilityOutcome::Success` OR `AbilityOutcome::Failed`

**3. Broadcast Stage:**
- Single system processes outcomes (ECS mutations + network broadcasting)
- Applies effects: ConsumeStamina, Damage, Teleport, Push, ClearQueue
- Broadcasts success events: `Do::UseAbility`, `Incremental` updates
- Applies recovery/synergy (ADR-012 integration)

**Pure Function Extraction:**
- Ability logic extracted into testable functions:
  - `validate_lunge(caster_loc, stamina, target_loc, ...) -> Result<LungeData, FailReason>`
  - `validate_overpower(...)`, `validate_knockback(...)`, `validate_deflect(...)`
- Unit testable without ECS (fast iteration, comprehensive coverage)
- Reusable for AI, scripting, replays

**Structured Effects:**
- `AbilityEffect` enum represents all possible ability outcomes:
  - `ConsumeStamina`, `Damage`, `Teleport`, `Push`, `ClearQueue`, `MutateQueue`
- Extensible for future effects (Shield, Heal, Buff, Debuff)
- Data-driven (effects describe what happens, broadcaster applies them)

### MVP Scope

**Phase 1 includes:**
- Three-stage pipeline infrastructure (validation → execution → broadcasting)
- Pure function extraction for all 4 MVP abilities (Lunge, Overpower, Knockback, Deflect)
- `AbilityEffect` enum with MVP effects (ConsumeStamina, Damage, Teleport, Push, ClearQueue)
- Unit tests for pure functions (comprehensive edge case coverage)
- Migration of all 4 abilities to observer pattern

**Phase 1 excludes:**
- Advanced effects (Shield, Heal, Buff, Debuff - future abilities)
- Effect batching optimization (single ability per frame acceptable for MVP)
- Channeled ability support (multi-frame execution - Phase 2)
- AI integration testing (AI uses same pipeline, but separate validation)

### Priority Justification

**MEDIUM PRIORITY** - Improves code quality and development velocity, but no immediate player-facing feature.

**Why medium priority:**
- Reliability: Centralized validation reduces ability bugs (player-facing benefit)
- Development velocity: New abilities ship faster with less boilerplate (unlocks future content)
- Maintainability: Single source of truth for universal mechanics (technical debt reduction)
- Testability: Pure functions enable comprehensive edge case testing (higher quality)

**Benefits:**
- Consistent ability behavior (fewer edge case bugs)
- Faster ability development (less duplication)
- Foundation for advanced mechanics (stun, silence, interrupt)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Three-Stage Observer Pipeline**

#### Core Mechanism

**Stage 1 - Universal Validation:**
- Listen for `Try::UseAbility` events (client requests)
- Check universal rules: death (RespawnTimer), recovery (GlobalRecovery + SynergyUnlock), GCD
- Emit `ValidatedAbilityUse` if passed OR broadcast `Do::AbilityFailed` with reason
- Runs in single system (ability_pipeline_system)

**Stage 2 - Ability Observers:**
- Each ability system listens for `ValidatedAbilityUse` (filters by ability type)
- Extract ECS data (queries for stamina, loc, target)
- Call pure function: `validate_{ability}(data) -> Result<AbilityData, FailReason>`
- Convert result to effects: `AbilityData::to_effects() -> Vec<AbilityEffect>`
- Emit `AbilityOutcome::Success` OR `AbilityOutcome::Failed`
- Runs in parallel (Bevy schedules ability systems concurrently)

**Stage 3 - Effect Broadcasting:**
- Listen for `AbilityOutcome` events
- Apply effects to ECS:
  - `ConsumeStamina`: Mutate Stamina component, broadcast Incremental
  - `Damage`: Trigger DealDamage event (ADR-005 pipeline)
  - `Teleport/Push`: Insert Loc component, broadcast Incremental
  - `ClearQueue/MutateQueue`: Mutate ReactionQueue, broadcast event
- On success: Broadcast `Do::UseAbility`, insert GlobalRecovery, apply synergies
- Runs in single system (outcome_broadcaster_system)

**Pure Function Example (Lunge):**
```rust
// Pure function - no ECS, just data
pub fn validate_lunge(
    caster_loc: Qrz,
    caster_stamina: f32,
    target_ent: Option<Entity>,
    target_loc: Qrz,
    tier_lock: Option<RangeTier>,
) -> Result<LungeData, AbilityFailReason> {
    // Check stamina
    if caster_stamina < 20.0 {
        return Err(AbilityFailReason::InsufficientResource);
    }

    // Check range (4 hexes)
    let distance = caster_loc.distance(target_loc);
    if distance > 4 {
        return Err(AbilityFailReason::OutOfRange);
    }

    // Check tier lock
    if let Some(tier) = tier_lock {
        if !tier.contains(distance) {
            return Err(AbilityFailReason::NoTargets);
        }
    }

    // Success: Return data
    Ok(LungeData {
        target_ent,
        target_loc,
        stamina_cost: 20.0,
        damage: 40.0,
    })
}
```

**Unit Test Example:**
```rust
#[test]
fn test_validate_lunge_out_of_range() {
    let result = validate_lunge(
        caster_loc: Qrz::new(0, 0),
        caster_stamina: 50.0,
        target_ent: Some(Entity::from_raw(1)),
        target_loc: Qrz::new(0, 5), // 5 hexes away (too far)
        tier_lock: None,
    );

    assert_eq!(result, Err(AbilityFailReason::OutOfRange));
}
```

#### Performance Projections

**Event Overhead:**
- 2 extra event passes vs. direct execution (Try → Validated → Outcome)
- Event processing: ~0.1ms per ability use
- Acceptable for turn-based combat (not real-time action game)

**Code Reduction:**
- Current: ~80 lines per ability (60 duplicated + 20 unique)
- Proposed: ~30 lines per ability (20 unique + 10 observer wrapper)
- Savings: ~50 lines per ability × 7 abilities = ~350 lines eliminated

**Testing Velocity:**
- Current: Integration tests only (full ECS setup, slow)
- Proposed: Unit tests for pure functions (fast, comprehensive)
- Speedup: ~10x faster test iteration (no ECS overhead)

**Development Time:**
- Phase 1 (MVP Refactor): 3-4 days (infrastructure + migrate 4 abilities)

#### Technical Risks

**1. Event Overhead**
- *Risk:* 2 extra event passes might add latency (Try → Validated → Outcome → Effects)
- *Mitigation:* Profile before/after, event processing is fast (~0.1ms per ability)
- *Impact:* Acceptable for turn-based combat, not performance-critical

**2. Migration Effort**
- *Risk:* Refactoring 4 abilities simultaneously increases regression risk
- *Mitigation:* Run test suite after each ability migration, migrate in order (Lunge → Overpower → Knockback → Deflect)
- *Impact:* 1 day migration effort, low risk if systematic

**3. Learning Curve**
- *Risk:* New pattern requires developer training (observer + pure functions)
- *Mitigation:* Document pattern clearly, provide examples, code review
- *Impact:* One-time learning cost, benefits compound with each new ability

**4. Indirection**
- *Risk:* Ability execution spans 3 systems (harder to trace)
- *Mitigation:* Clear naming, documentation, logging at each stage
- *Impact:* Tradeoff for maintainability (easier to change validation logic)

### System Integration

**Affected Systems:**
- Ability execution (all 4 MVP abilities refactored)
- Damage pipeline (Damage effect triggers ADR-005 pipeline)
- Recovery/synergy (broadcaster applies GlobalRecovery, triggers synergies)
- Reaction queue (ClearQueue/MutateQueue effects)
- Network broadcasting (Incremental updates, UseAbility events)

**Compatibility:**
- ✅ Uses existing ADR-005 damage pipeline (Damage effect)
- ✅ Uses existing ADR-012 recovery/synergy (broadcaster applies)
- ✅ Uses existing reaction queue (ClearQueue/MutateQueue effects)
- ✅ Uses existing network messaging (Incremental, UseAbility events)

### Alternatives Considered

#### Alternative 1: Trait-Based Abstraction

Define `Ability` trait with validate/execute methods.

**Rejected because:**
- Different query signatures per ability (loses Bevy parallelism)
- Can't share queries across abilities (performance hit)
- Trait objects limit compile-time optimization

#### Alternative 2: Macro-Generated Boilerplate

Use macros to generate validation/broadcasting code per ability.

**Rejected because:**
- Hides control flow (debugging harder)
- Limited flexibility for complex abilities (channeled, multi-target)
- Doesn't address testability (still requires ECS for tests)

#### Alternative 3: Do Nothing

Keep current duplication.

**Rejected because:**
- Duplication will worsen (7 abilities = 420 duplicated lines)
- Maintenance burden increases with each new ability
- Testing remains slow (full ECS setup required)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Observer pattern + pure function extraction separates ability logic (testable, reusable) from ECS plumbing (queries, mutations).

**Testability is the primary benefit:** Pure functions can be unit tested comprehensively without ECS overhead. This enables thorough edge case testing for all abilities.

**Extensibility:** Future abilities only require pure validation function + thin observer wrapper. Universal mechanics (stun, silence) added to pipeline once, all abilities benefit.

### PLAYER Validation

**Player-Facing Benefits:**
- **Reliability:** Fewer ability bugs (centralized validation)
- **Consistency:** All abilities respect same rules (no "why did this work differently?")
- **Predictability:** Clear failure reasons (out of range, insufficient stamina)
- **Polish:** New abilities ship with comprehensive testing (fewer regressions)

**Development Velocity:**
- Current: ~80 lines per ability, slow testing (full ECS setup)
- Proposed: ~30 lines per ability, fast testing (pure functions)
- Impact: New abilities ship faster, higher quality

---

## Approval

**Status:** Proposed for implementation

**Approvers:**
- ARCHITECT: ✅ Pipeline pattern solid, pure function extraction enables testability, event overhead acceptable
- PLAYER: ✅ Improves reliability and development velocity (faster ability content)

**Scope Constraint:** Fits in one SOW (3-4 days for refactor)

**Dependencies:**
- ADR-004: Ability System (Try::UseAbility events)
- ADR-005: Damage Pipeline (Damage effect integration)
- ADR-012: Recovery and Synergies (broadcaster applies GlobalRecovery)
- ADR-009: MVP Ability Set (4 abilities to migrate)

**Next Steps:**
1. ARCHITECT creates ADR-018 documenting pipeline architecture
2. ARCHITECT creates SOW-013 with 4-phase implementation plan
3. DEVELOPER begins Phase 1 (infrastructure skeleton)

**Date:** 2025-11-07
