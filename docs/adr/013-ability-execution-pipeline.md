# ADR-013: Ability Execution Pipeline (Observer Pattern)

## Status
Proposed

## Context

### Problem Statement

The current ability system has ~60 lines of duplicated validation and broadcasting logic per ability, making maintenance increasingly difficult as more abilities are added.

**Duplication Pattern:**

Looking at [lunge.rs:231](../../src/server/systems/combat/abilities/lunge.rs), [overpower.rs:222](../../src/server/systems/combat/abilities/overpower.rs), [knockback.rs:248](../../src/server/systems/combat/abilities/knockback.rs), [deflect.rs:129](../../src/server/systems/combat/abilities/deflect.rs):

**Duplicated (~60 lines/ability):**
- Death check, recovery/synergy check, GCD check
- Stamina consumption + broadcasting
- Recovery/synergy application, success/failure events

**Ability-specific (~20-40 lines/ability):**
- Target/range validation, ability effects, effect broadcasting

**Maintenance Problems:**

1. **Brittle:** Adding universal check (stun, silence) = change 7+ files
2. **Untestable:** Can't test ability logic without full ECS setup
3. **Inconsistent:** Error handling varies between abilities
4. **Error-prone:** Easy to forget recovery application, synergy checks

### Design Goals

1. **Single source of truth** for universal validation
2. **Pure function testability** for ability-specific logic
3. **Structured outcomes** (data) instead of imperative commands (events)
4. **Clear separation:** validation → execution → broadcasting

### Related Systems

- **ADR-012:** Recovery/synergy checks currently duplicated
- **ADR-005:** Similar pipeline pattern for damage processing
- **ADR-006:** AI will use same pipeline

---

## Decision

**Three-stage observer pipeline: universal validation → ability-specific execution → structured broadcasting**

Abilities become pure observers that receive validated inputs and return structured outcomes.

---

## Technical Design

### Architecture Overview

```
Try::UseAbility (client request)
    ↓
[1] Pipeline: Universal Validation
    - Death, recovery, GCD checks
    └─→ emit ValidatedAbilityUse OR write AbilityFailed
    ↓
[2] Observers: Ability-Specific Logic (parallel)
    - Filter by ability type
    - Call pure function: validate_X(params) → Result<AbilityData, FailReason>
    - Generate effects from data
    └─→ emit AbilityOutcome::Success/Failed
    ↓
[3] Broadcaster: Effect Processing
    - Apply ECS mutations (stamina, Loc, queue)
    - Broadcast network events (Incremental, UseAbility)
    - Apply recovery/synergy (ADR-012)
```

**Key Insight:** Extract ability-specific logic into **pure functions** that can be unit tested without ECS.

---

### Data Structures

```rust
// Events
ValidatedAbilityUse { caster, ability, target_loc }
AbilityOutcome::Success { caster, ability, target_loc, effects: Vec<AbilityEffect> }
AbilityOutcome::Failed { caster, reason }

// Effects (extensible enum)
AbilityEffect::ConsumeStamina { entity, amount }
AbilityEffect::Damage { target, base_damage, damage_type }
AbilityEffect::Teleport { entity, destination }
AbilityEffect::Push { entity, destination }
AbilityEffect::ClearQueue { entity, clear_type }
AbilityEffect::MutateQueue { entity, operation }
```

**Effect Examples:**
- **Lunge:** ConsumeStamina(20) + Teleport + Damage(40)
- **Overpower:** ConsumeStamina(40) + Damage(80)
- **Knockback:** ConsumeStamina(30) + Push + MutateQueue(PopBack)
- **Deflect:** ConsumeStamina(50) + ClearQueue(All)

---

### Pure Function Extraction Pattern

**Current (untestable):**
```rust
pub fn handle_lunge(
    commands, reader, entity_query, target_query, stamina_query, ...
) {
    // 80 lines of ECS queries + validation + mutations
}
```

**Proposed (testable):**
```rust
// Pure function - no ECS, just data
pub fn validate_lunge(
    caster_loc: Qrz,
    caster_stamina: f32,
    target_ent: Option<Entity>,
    target_loc: Qrz,
    tier_lock: Option<RangeTier>,
) -> Result<LungeData, AbilityFailReason> {
    // 20 lines of pure validation logic
    // Returns data struct, not effects
}

// System wrapper - thin ECS adapter
pub fn lunge_ability_system(
    reader: EventReader<ValidatedAbilityUse>,
    query: Query<...>,
) {
    // Extract ECS data → call pure function → convert to effects
}
```

**Benefits:**
- Test `validate_lunge()` with raw data (no Bevy setup)
- System becomes thin adapter (queries → pure function → effects)
- Pure function reusable (AI, scripting, replays)

---

### System Responsibilities

#### [1] AbilityPipeline System

**Location:** `src/server/systems/combat/ability_pipeline.rs`

**Responsibility:** Universal validation (death, recovery, GCD)

**Logic:**
- Read `Try::UseAbility` events
- Check death (RespawnTimer), recovery (GlobalRecovery + SynergyUnlock), GCD
- Emit `ValidatedAbilityUse` or write `Do::AbilityFailed`

---

#### [2] Ability Observer Systems

**Location:** `src/server/systems/combat/abilities/{ability}.rs`

**Responsibility:** Ability-specific validation + effect generation

**Pattern:**
```rust
pub fn {ability}_ability_system(
    reader: EventReader<ValidatedAbilityUse>,
    outcome_writer: EventWriter<AbilityOutcome>,
    queries: Query<...>,
) {
    for event in reader.read() {
        if event.ability != AbilityType::{Ability} { continue; }

        // Extract ECS data
        let data = extract_from_queries(&queries, event.caster);

        // Call pure function
        let result = validate_{ability}(data);

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

**Pure Functions:**
- `validate_lunge(...)` → `Result<LungeData, FailReason>`
- `validate_overpower(...)` → `Result<OverpowerData, FailReason>`
- `validate_knockback(...)` → `Result<KnockbackData, FailReason>`
- `validate_deflect(...)` → `Result<DeflectData, FailReason>`

---

#### [3] OutcomeBroadcaster System

**Location:** `src/server/systems/combat/ability_pipeline.rs`

**Responsibility:** Effect processing (ECS mutations + network broadcasting)

**Logic:**
- Read `AbilityOutcome` events
- For each effect:
  - `ConsumeStamina`: Mutate Stamina component, broadcast Incremental
  - `Teleport/Push`: Insert Loc component, broadcast Incremental
  - `Damage`: Trigger DealDamage event (ADR-005 pipeline)
  - `ClearQueue/MutateQueue`: Mutate ReactionQueue, broadcast ClearQueue
- On success:
  - Broadcast `Do::UseAbility`
  - Insert GlobalRecovery component (ADR-012)
  - Apply synergies (ADR-012)
  - Activate legacy GCD (temporary, will remove)

---

### Helper Functions

**Location:** `src/server/systems/combat/ability_pipeline.rs`

```rust
// Shared tier lock validation (used by Lunge, Overpower)
fn validate_tier_locked_target(...) -> Option<Entity>

// Shared range checking
fn validate_range(caster: Qrz, target: Qrz, min: u32, max: u32) -> bool

// Shared target viability (alive, exists)
fn is_target_valid(target: Entity, respawn_query: &Query<...>) -> bool
```

---

## Implementation Plan

### Holistic Migration (All-at-Once)

**Phase 1: Add Infrastructure (1 day)**
1. Create `ability_outcome.rs` (data structures)
2. Create `ability_pipeline.rs` (pipeline + broadcaster skeleton)
3. Wire into CombatPlugin (events, systems)
4. Verify compilation (systems idle, old abilities still work)

**Phase 2: Extract Pure Functions (1 day)**
1. For each ability, extract validation logic:
   - `validate_lunge()`, `validate_overpower()`, etc.
2. Write unit tests for pure functions (no ECS)
3. Keep in same file as system (easy to find)

**Phase 3: Refactor All Abilities (1 day)**
1. Refactor all 4 abilities simultaneously:
   - Convert to observer pattern (listen for ValidatedAbilityUse)
   - Call pure function
   - Convert result to AbilityOutcome
2. Remove old systems
3. Run test suite (verify equivalence)

**Phase 4: Cleanup (half day)**
1. Remove legacy GCD (ADR-012 Phase 1)
2. Extract shared helpers (tier lock, range checking)
3. Performance profiling

**Timeline:** 3.5 days total

---

## Testing Strategy

### Unit Tests: Pure Functions (Primary)

**Location:** `src/server/systems/combat/abilities/{ability}.rs`

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_validate_lunge_success() {
        let result = validate_lunge(
            caster_loc: Qrz::new(0, 0),
            caster_stamina: 50.0,
            target_ent: Some(Entity::from_raw(1)),
            target_loc: Qrz::new(0, 3), // 3 hexes away
            tier_lock: None,
        );

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.stamina_cost, 20.0);
        assert_eq!(data.damage, 40.0);
    }

    #[test]
    fn test_validate_lunge_out_of_range() {
        let result = validate_lunge(
            // ...
            target_loc: Qrz::new(0, 5), // 5 hexes away (too far)
            // ...
        );

        assert_eq!(result, Err(AbilityFailReason::OutOfRange));
    }

    #[test]
    fn test_validate_lunge_tier_locked_wrong_tier() {
        let result = validate_lunge(
            // ...
            tier_lock: Some(RangeTier::Close), // Locked to close (1-2)
            target_loc: Qrz::new(0, 4), // But target is 4 hexes (mid tier)
        );

        assert_eq!(result, Err(AbilityFailReason::NoTargets));
    }
}
```

**Benefits:**
- Fast (no ECS setup)
- Exhaustive (test all edge cases)
- Clear (pure input/output)
- Maintainable (no mock queries)

### System Tests: Pipeline Integration (Secondary)

**Location:** `src/server/systems/combat/ability_pipeline.rs`

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_pipeline_blocks_dead_caster() {
        // Minimal ECS setup: spawn entity with RespawnTimer
        // Send Try::UseAbility
        // Assert: AbilityFailed broadcast, no ValidatedAbilityUse
    }

    #[test]
    fn test_broadcaster_applies_stamina() {
        // Minimal ECS setup: spawn entity with Stamina
        // Send AbilityOutcome::Success with ConsumeStamina effect
        // Assert: Stamina component mutated, Incremental broadcast
    }
}
```

**Purpose:** Verify plumbing (events flow correctly, ECS mutations applied)

---

## File Structure

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
    │   ├── ability_pipeline_system
    │   ├── outcome_broadcaster_system
    │   └── helpers (validate_tier_locked_target, etc.)
    │
    └── abilities/
        ├── lunge.rs [REFACTOR]
        │   ├── validate_lunge() [NEW pure function]
        │   └── lunge_ability_system [REFACTOR observer]
        ├── overpower.rs [REFACTOR]
        ├── knockback.rs [REFACTOR]
        └── deflect.rs [REFACTOR]
```

---

## Consequences

### Positive

✅ **Massive code reduction:** ~60 lines eliminated per ability (38-65% smaller)

✅ **Single source of truth:** Universal validation in 1 place (not 7)

✅ **Testable:** Pure functions tested without ECS (fast, comprehensive)

✅ **Extensible:** New abilities = pure function + thin system wrapper

✅ **Consistent:** All abilities use same validation/broadcasting pattern

✅ **Maintainable:** Adding universal check = modify pipeline only

✅ **Reusable:** Pure functions work for AI, scripting, replays

### Negative

⚠️ **Event overhead:** 2 extra event passes vs direct execution

⚠️ **Indirection:** Ability execution spans 3 systems (harder to trace)

⚠️ **Migration effort:** Refactor 4 abilities + extract pure functions (3.5 days)

⚠️ **Learning curve:** New pattern for developers to understand

### Neutral

🔹 **Commits to event-driven architecture** (not direct function calls)

🔹 **Pure functions separate from systems** (could extract to separate module later)

🔹 **Effect-based outcomes** (enables future replay/rollback)

---

## Open Questions

**Performance:**
- Is event overhead acceptable? (Profile before/after)
- Should outcomes be batched? (Multiple abilities in one frame)

**Testing:**
- Should integration tests be added? (Or trust unit tests + system tests?)
- How to test AI using same pipeline?

**Architecture:**
- Should pure functions live in separate module? (abilities/logic/)
- Do we need ability priorities? (Simultaneous abilities)
- How do channeled abilities fit? (Multi-frame execution)

---

## Alternatives Considered

### Alt 1: Trait-Based Abstraction

```rust
trait Ability {
    fn validate(&self, world: &World) -> Result<...>;
    fn execute(&self, world: &mut World) -> Vec<Effect>;
}
```

**Rejected:** Different query signatures per ability, loses Bevy parallelism

### Alt 2: Macro-Generated Boilerplate

```rust
define_ability! { Lunge { cost: 20, range: 4, ... } }
```

**Rejected:** Hides control flow, limited flexibility for complex abilities

### Alt 3: Do Nothing

**Rejected:** Duplication will worsen (7 abilities = 420 duplicated lines)

---

## References

**Related ADRs:**
- [ADR-004: Ability System](004-ability-system-and-targeting.md)
- [ADR-005: Damage Pipeline](005-damage-pipeline.md)
- [ADR-006: AI Behavior](006-ai-behavior-and-ability-integration.md)
- [ADR-009: MVP Ability Set](009-mvp-ability-set.md)
- [ADR-012: Recovery and Synergies](012-ability-recovery-and-synergies.md)

**Current Implementations:**
- [lunge.rs](../../src/server/systems/combat/abilities/lunge.rs)
- [overpower.rs](../../src/server/systems/combat/abilities/overpower.rs)
- [knockback.rs](../../src/server/systems/combat/abilities/knockback.rs)
- [deflect.rs](../../src/server/systems/combat/abilities/deflect.rs)

---

**Document Version:** 1.0
**Created:** 2025-11-07
**Author:** ARCHITECT
