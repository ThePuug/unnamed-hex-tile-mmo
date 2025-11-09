# ADR-018: Three-Stage Ability Execution Pipeline with Pure Function Extraction

## Status

**Proposed** - 2025-11-07

## Context

**Related RFC:** [RFC-013: Ability Execution Pipeline (Reliable Ability System)](../01-rfc/013-ability-execution-pipeline.md)

Current ability implementations have ~60 lines of duplicated validation and broadcasting logic per ability. Adding universal mechanics (stun, silence) requires changing 7+ files. Ability logic requires full ECS setup to test.

### Requirements

- Single source of truth for universal validation (death, recovery, GCD)
- Pure function testability for ability-specific logic
- Structured outcomes (data) instead of imperative commands (events)
- Clear separation: validation → execution → broadcasting
- Extensible for future abilities (minimal boilerplate)

### Options Considered

**Option 1: Three-Stage Observer Pipeline** ✅ **SELECTED**
- Universal validation → Ability observers → Effect broadcasting
- Pure functions extracted for testability
- Structured outcomes (AbilityEffect enum)

**Option 2: Trait-Based Abstraction**
- `Ability` trait with validate/execute methods
- ❌ Different query signatures, loses Bevy parallelism

**Option 3: Macro-Generated Boilerplate**
- Macros generate validation/broadcasting code
- ❌ Hides control flow, limited flexibility

**Option 4: Do Nothing**
- Keep current duplication
- ❌ Worsens with each new ability (420 duplicated lines for 7 abilities)

## Decision

**Use three-stage observer pipeline: universal validation → ability-specific execution → structured broadcasting. Extract ability logic into pure functions testable without ECS.**

### Core Mechanism

**Pipeline Flow:**
```
1. Client: Try::UseAbility { ability, ... }
   ↓
2. Pipeline: Universal Validation
   - Check death, recovery, GCD
   - Emit ValidatedAbilityUse OR broadcast AbilityFailed
   ↓
3. Observers: Ability-Specific Logic (parallel)
   - Filter by ability type
   - Call pure function: validate_X(params) → Result<Data, Reason>
   - Convert to effects: Data::to_effects() → Vec<AbilityEffect>
   - Emit AbilityOutcome::Success OR Failed
   ↓
4. Broadcaster: Effect Processing
   - Apply ECS mutations (stamina, Loc, queue)
   - Broadcast network events (Incremental, UseAbility)
   - Apply recovery/synergy (GlobalRecovery, SynergyUnlock)
```

**Pure Function Pattern:**

```rust
// Pure function - no ECS, just data
pub fn validate_lunge(
    caster_loc: Qrz,
    caster_stamina: f32,
    target_ent: Option<Entity>,
    target_loc: Qrz,
    tier_lock: Option<RangeTier>,
) -> Result<LungeData, AbilityFailReason>
```

**Observer System Pattern:**

```rust
pub fn lunge_ability_system(
    reader: EventReader<ValidatedAbilityUse>,
    outcome_writer: EventWriter<AbilityOutcome>,
    queries: Query<...>,
) {
    for event in reader.read() {
        if event.ability != AbilityType::Lunge { continue; }

        // Extract ECS data
        let data = extract_from_queries(&queries, event.caster);

        // Call pure function
        let result = validate_lunge(data);

        // Convert to effects
        match result {
            Ok(ability_data) => {
                let effects = ability_data.to_effects();
                outcome_writer.write(AbilityOutcome::Success { effects, ... });
            }
            Err(reason) => {
                outcome_writer.write(AbilityOutcome::Failed { reason, ... });
            }
        }
    }
}
```

**Structured Effects:**

```rust
pub enum AbilityEffect {
    ConsumeStamina { entity: Entity, amount: f32 },
    Damage { target: Entity, base_damage: u32, damage_type: DamageType },
    Teleport { entity: Entity, destination: Qrz },
    Push { entity: Entity, destination: Qrz },
    ClearQueue { entity: Entity, clear_type: ClearType },
    MutateQueue { entity: Entity, operation: QueueOperation },
}
```

**Effect Examples:**
- Lunge: `[ConsumeStamina(20), Teleport(adjacent), Damage(40)]`
- Overpower: `[ConsumeStamina(40), Damage(80)]`
- Knockback: `[ConsumeStamina(30), Push(1 hex), MutateQueue(PopBack)]`
- Deflect: `[ConsumeStamina(50), ClearQueue(All)]`

---

## Rationale

### 1. Universal Validation Eliminates Duplication

**Before (Current):**
- Every ability checks: death, recovery, GCD (60 lines duplicated)
- Adding stun check = modify 7 files
- Inconsistent error handling between abilities

**After (Pipeline):**
- Universal checks in single system (ability_pipeline_system)
- Adding stun check = modify 1 file
- Consistent error handling (all abilities use same path)

**Impact:** ~350 lines eliminated across 7 abilities (60 lines × 7 - 70 lines pipeline overhead).

### 2. Pure Functions Enable Comprehensive Testing

**Before (Current):**
- Test requires full ECS setup (spawn entities, components, systems)
- Slow iteration (~1-2s per test)
- Hard to test edge cases (complex ECS state setup)

**After (Pure Functions):**
- Test with raw data (no ECS setup)
- Fast iteration (~0.01s per test)
- Easy to test edge cases (just data permutations)

**Example:**
```rust
#[test]
fn test_validate_lunge_out_of_range() {
    let result = validate_lunge(
        caster_loc: Qrz::new(0, 0),
        caster_stamina: 50.0,
        target_ent: Some(Entity::from_raw(1)),
        target_loc: Qrz::new(0, 5), // 5 hexes (too far)
        tier_lock: None,
    );

    assert_eq!(result, Err(AbilityFailReason::OutOfRange));
}
```

**Impact:** 10x faster test iteration, enables exhaustive edge case coverage.

### 3. Structured Outcomes Enable Future Features

**Data-Driven Effects:**
- Effects describe what happens (not how to do it)
- Broadcaster interprets effects (centralized)
- Easy to add new effects (extend enum)

**Future Capabilities:**
- **Replay:** Record effects, replay later
- **Rollback:** Undo effects if prediction wrong
- **Simulation:** Test ability interactions without ECS
- **Scripting:** AI/scripts generate effects directly

**Impact:** Architecture ready for advanced features (replay, rollback, scripting).

### 4. Observer Pattern Maintains Bevy Parallelism

**Parallel Execution:**
- Stage 2 (observers) runs in parallel (Bevy schedules systems concurrently)
- Each ability system filters by type (no contention)
- Queries scoped to ability needs (optimal parallelism)

**Alternative (Trait) Loses Parallelism:**
- Trait requires generic `World` access (all abilities share)
- Can't parallelize (serialized execution)
- Performance degradation with many abilities

**Impact:** Maintains Bevy's parallelism benefits as ability count grows.

### 5. Thin Observer Wrapper Minimizes Boilerplate

**System Responsibilities:**
- Extract ECS data (queries)
- Call pure function
- Convert result to effects
- Emit outcome

**Code Reduction:**
- Current: ~80 lines per ability
- Proposed: ~30 lines per ability (20 unique logic + 10 wrapper)
- Savings: ~50 lines per ability

**Impact:** New abilities faster to implement, less copy-paste errors.

---

## Consequences

### Positive

**1. Massive Code Reduction**
- ~350 lines eliminated across 7 abilities (60 duplicated × 7 - 70 pipeline)
- Single source of truth for universal validation

**2. Testable Ability Logic**
- Pure functions tested without ECS (10x faster)
- Comprehensive edge case coverage
- Fast iteration (no Bevy setup overhead)

**3. Consistent Behavior**
- All abilities use same validation/broadcasting path
- Uniform error handling
- Easier to reason about ability execution

**4. Extensible Architecture**
- New abilities = pure function + thin wrapper
- Universal mechanics added once (stun, silence)
- Structured effects enable future features (replay, rollback)

**5. Reusable Logic**
- Pure functions work for AI, scripting, replays
- Not tied to player input

### Negative

**1. Event Overhead**
- 2 extra event passes (Try → Validated → Outcome)
- ~0.1ms per ability use
- Acceptable for turn-based combat

**2. Indirection**
- Ability execution spans 3 systems (harder to trace)
- Logging/debugging requires understanding pipeline flow

**3. Migration Effort**
- Refactor 4 abilities (extract pure functions, convert to observers)
- 3-4 days effort
- Risk: Regression if not tested thoroughly

**4. Learning Curve**
- New pattern for developers (observer + pure functions)
- Requires understanding pipeline stages

### Neutral

**1. Event-Driven Architecture**
- Commits to event pattern (not direct function calls)
- Consistent with existing damage pipeline (ADR-005)

**2. Effect-Based Outcomes**
- Abilities return effects (not mutate directly)
- Enables future features (replay, rollback)

**3. Pure Functions Separate from Systems**
- Could extract to separate module later (abilities/logic/)
- MVP: Keep in same file (easy to find)

---

## Implementation Notes

**System Execution Order:**
```
1. ability_pipeline_system (universal validation)
2. lunge_ability_system, overpower_ability_system, ... (observers, parallel)
3. outcome_broadcaster_system (effect application)
```

**File Structure:**
```
src/
├── common/components/
│   └── ability_outcome.rs [NEW]
│       ├── ValidatedAbilityUse
│       ├── AbilityOutcome
│       └── AbilityEffect
│
└── server/systems/combat/
    ├── ability_pipeline.rs [NEW]
    │   ├── ability_pipeline_system (validator)
    │   ├── outcome_broadcaster_system (broadcaster)
    │   └── helpers (shared validation logic)
    │
    └── abilities/
        ├── lunge.rs [REFACTOR]
        │   ├── validate_lunge() [NEW pure function]
        │   └── lunge_ability_system [REFACTOR observer]
        ├── overpower.rs [REFACTOR]
        ├── knockback.rs [REFACTOR]
        └── deflect.rs [REFACTOR]
```

**Integration Points:**
- Universal validation: death (RespawnTimer), recovery (GlobalRecovery + SynergyUnlock), GCD
- Effect broadcasting: Damage (ADR-005 pipeline), Recovery (ADR-012), Reaction queue (ADR-003)
- Network: Incremental updates, UseAbility events

---

## Validation Criteria

**Functional:**
- Universal validation blocks dead/recovering/GCD-locked casters
- Pure functions return correct data for all edge cases
- Broadcaster applies effects correctly (stamina, Loc, queue, recovery)
- All 4 abilities migrated and working

**UX:**
- Abilities behave identically to before (no regressions)
- Failure reasons clear (out of range, insufficient stamina)
- Consistent error handling across all abilities

**Performance:**
- Event overhead < 0.1ms per ability use
- Pure function tests run in < 0.01s each
- Bevy parallelism maintained (observers run concurrently)

**Code Quality:**
- ~350 lines eliminated (duplication removed)
- Pure functions have comprehensive unit tests
- Systems thin and focused (clear responsibilities)

---

## References

- **RFC-013:** Ability Execution Pipeline (Reliable Ability System)
- **ADR-004:** Ability System (Try::UseAbility events)
- **ADR-005:** Damage Pipeline (similar pattern, Damage effect integration)
- **ADR-009:** MVP Ability Set (4 abilities to migrate)
- **ADR-012:** Recovery and Synergies (broadcaster applies GlobalRecovery)

## Date

2025-11-07
