# SOW-021: Relative Meta-Attribute System Implementation

## Status

Planning

## Overview

Implement the relative meta-attribute opposition system from RFC-021 and ADR-031 in three phases:

1. **Phase 1**: Impact/Composure (recovery timeline) + crit removal
2. **Phase 2**: Finesse/Cunning (lockout vs window)
3. **Phase 3**: Dominance/Toughness (sustain ratio) + minimal healing

Each phase delivers a complete, testable, and playable relative opposition pair.

## Phase 1: Impact/Composure — Recovery Timeline

**Goal**: Implement recovery pushback (Impact) and recovery reduction (Composure), remove critical hit system.

### 1.1 Remove Critical Hit System

**Files to modify**:
- `src/common/systems/combat/damage.rs` - Remove `roll_critical()` function
- `src/server/systems/combat/abilities/*.rs` - Remove crit handling from all ability implementations
- `src/client/systems/combat_ui.rs` - Remove crit-related UI elements
- `src/client/systems/target_frame.rs` - Remove crit damage display

**Changes**:
- Delete `roll_critical()` function and tests
- Remove `precision_mod` parameter from all ability damage calculations
- Remove crit multiplier application in damage pipeline
- Remove any UI elements showing crit chance or crit indicators
- Update damage calculation tests to remove crit cases

**Tests**:
- ✅ Verify damage calculations are deterministic (no RNG)
- ✅ Verify no crit-related code paths remain
- ✅ Verify UI compiles without crit references

### 1.2 Implement Recovery Pushback (Impact)

**Files to modify**:
- `src/common/components/mod.rs` - Add `impact()` meta-attribute (already exists, verify mapping)
- `src/common/components/recovery.rs` - Add `pushback()` method to `GlobalRecovery`
- `src/common/systems/combat/damage.rs` - Add `apply_recovery_pushback()` function
- `src/server/systems/combat.rs` - Integrate pushback into damage resolution

**New functionality**:

```rust
// GlobalRecovery extension
impl GlobalRecovery {
    /// Apply recovery pushback based on Impact vs Composure contest
    /// Extends recovery timer by percentage of max duration
    pub fn apply_pushback(&mut self, pushback_amount: f32) {
        let extension = self.duration * pushback_amount;
        self.remaining = (self.remaining + extension).min(self.duration * 2.0); // cap at 2x duration
    }
}

// In damage.rs
pub fn calculate_recovery_pushback(
    attacker_impact: u16,
    defender_composure: u16,
) -> f32 {
    const BASE_PUSHBACK: f32 = 0.25; // 25% of max recovery
    let contest_mod = contest_modifier(attacker_impact, defender_composure);
    BASE_PUSHBACK * contest_mod
}
```

**Integration**:
- After damage is dealt and resolves, check if target has `GlobalRecovery`
- If yes, calculate pushback percentage via contest
- Apply pushback to target's recovery timer
- Cap pushback at 2x original duration (prevent infinite lockout)

**Tests**:
- ✅ `test_pushback_neutral_contest()` - Equal Impact/Composure → 25% extension
- ✅ `test_pushback_attacker_advantage()` - High Impact vs low Composure → ~37.5% extension (1.5x modifier)
- ✅ `test_pushback_defender_advantage()` - Low Impact vs high Composure → ~12.5% extension (0.5x modifier)
- ✅ `test_pushback_cap()` - Multiple pushbacks cap at 2x original duration
- ✅ `test_no_pushback_without_recovery()` - No effect if target has no active recovery

### 1.3 Implement Recovery Reduction (Composure)

**Files to modify**:
- `src/common/components/mod.rs` - Add `composure()` meta-attribute (already exists, verify mapping)
- `src/common/systems/combat/recovery.rs` - Modify `global_recovery_system()` to apply Composure reduction
- `src/common/components/recovery.rs` - Add reduction calculation

**New functionality**:

```rust
// In recovery system
pub fn global_recovery_system(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery, &ActorAttributes)>,
) {
    let delta = time.delta_secs();

    for (entity, mut recovery, attrs) in query.iter_mut() {
        if recovery.is_active() {
            // Apply Composure-based reduction
            let composure = attrs.composure() as f32;
            let reduction_factor = calculate_composure_reduction(composure);
            let effective_delta = delta * (1.0 + reduction_factor);

            recovery.tick(effective_delta);

            if !recovery.is_active() {
                commands.entity(entity).remove::<GlobalRecovery>();
            }
        }
    }
}

// In recovery.rs or damage.rs
pub fn calculate_composure_reduction(composure: u16) -> f32 {
    // Linear scaling: 0 composure = 0% faster, 200 composure = 20% faster
    const K: f32 = 0.001; // 0.1% per point
    (composure as f32 * K).min(0.30) // cap at 30% faster recovery
}
```

**Integration**:
- Modify tick rate in `global_recovery_system()` based on entity's Composure
- Higher Composure → faster recovery tick → shorter lockout
- Apply to all recovery timers (both ability lockout and any future recovery types)

**Tests**:
- ✅ `test_composure_reduction_zero()` - 0 Composure → no reduction
- ✅ `test_composure_reduction_linear()` - Composure increases → recovery time decreases linearly
- ✅ `test_composure_reduction_capped()` - Very high Composure caps at 30% reduction
- ✅ `test_composure_vs_pushback()` - Composure reduction counteracts Impact pushback

### 1.4 Integration Tests

**Tests**:
- ✅ `test_impact_composure_tempo_battle()` - Simulate fight between high-Impact vs high-Composure builds
- ✅ `test_neutral_recovery_unchanged()` - Equal stats → recovery works as before
- ✅ `test_pushback_survives_composure()` - Impact pushback is visible even with Composure reduction

**Acceptance Criteria**:
- Critical hit system fully removed (no crit code, no crit UI)
- Impact extends enemy recovery on hit (observable in game)
- Composure reduces own recovery timers (observable in game)
- All tests pass
- Combat tempo feels different based on Impact/Composure investment

---

## Phase 2: Finesse/Cunning — Lockout vs Window

**Goal**: Implement synergy recovery reduction (Finesse) and reaction window extension (Cunning).

### 2.1 Add Finesse Meta-Attribute

**Files to modify**:
- `src/common/components/mod.rs` - Replace `precision()` with `finesse()`
- `src/common/systems/combat/damage.rs` - Remove all `precision_mod` parameters (already done in Phase 1)
- `src/client/systems/character_panel.rs` - Update UI to show "Finesse" instead of "Precision"

**Changes**:
```rust
// In ActorAttributes
// Remove:
// pub fn precision(&self) -> u16 { self.grace() }

// Add:
/// Finesse: Synergy chain compression from grace (relative meta-attribute)
/// Used in contest vs Cunning (affects synergy recovery reduction)
pub fn finesse(&self) -> u16 { self.grace() }
```

**Tests**:
- ✅ `test_finesse_maps_to_grace()` - Verify finesse() returns grace() value
- ✅ Verify no `precision()` references remain in codebase

### 2.2 Implement Synergy Recovery Reduction (Finesse)

**Files to modify**:
- `src/common/systems/combat/synergies.rs` - Modify `apply_synergies()` to accept attacker/defender attributes
- `src/server/systems/combat/abilities/*.rs` - Pass attributes when calling `apply_synergies()`

**New functionality**:

```rust
// Modified synergies.rs
pub fn apply_synergies(
    entity: Entity,
    used_ability: AbilityType,
    recovery: &GlobalRecovery,
    attacker_attrs: &ActorAttributes,
    defender_attrs: &ActorAttributes,
    commands: &mut Commands,
) {
    let Some(trigger_type) = get_synergy_trigger(used_ability) else {
        return;
    };

    // Calculate Finesse vs Cunning contest modifier
    let finesse = attacker_attrs.finesse();
    let cunning = defender_attrs.cunning();
    let synergy_mod = contest_modifier(finesse, cunning);

    for rule in MVP_SYNERGIES {
        if rule.trigger == trigger_type {
            // Apply Finesse modifier to unlock reduction
            let effective_reduction = rule.unlock_reduction * synergy_mod;
            let unlock_at = (recovery.remaining - effective_reduction).max(0.0);

            let synergy = SynergyUnlock::new(rule.target, unlock_at, used_ability);
            if let Ok(mut entity_cmd) = commands.get_entity(entity) {
                entity_cmd.insert(synergy);
            }
        }
    }
}
```

**Integration**:
- When ability creates synergy, calculate contest between attacker Finesse and self Cunning
- High Finesse → tighter burst windows (more reduction)
- High Cunning → longer gaps between attacker's synergy chains (less reduction)
- Affects both attacker perspective (tighter chains) and defender perspective (more time to react)

**Tests**:
- ✅ `test_finesse_neutral()` - Equal Finesse/Cunning → base synergy reduction (0.5s for Lunge→Overpower)
- ✅ `test_finesse_advantage()` - High Finesse vs low Cunning → ~0.75s reduction (1.5x modifier)
- ✅ `test_cunning_advantage()` - Low Finesse vs high Cunning → ~0.25s reduction (0.5x modifier)
- ✅ `test_finesse_enables_instant_chains()` - Very high Finesse can unlock synergies at full recovery duration
- ✅ `test_synergy_window_calculation()` - Verify unlock_at is correct with modifier

### 2.3 Implement Reaction Window Extension (Cunning)

**Files to modify**:
- `src/common/components/reaction_queue.rs` - Add window calculation field to `QueuedThreat`
- `src/server/systems/combat/queue.rs` - Modify threat insertion to apply Cunning
- `src/client/systems/combat_ui.rs` - Display extended reaction window

**New functionality**:

```rust
// In reaction_queue.rs
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct QueuedThreat {
    // ... existing fields ...

    /// Extended window from Cunning (added to base timer_duration)
    pub cunning_extension: Duration,
}

// In queue insertion
pub fn calculate_reaction_window(
    base_duration: Duration,
    defender_cunning: u16,
) -> Duration {
    // Linear scaling: 0 cunning = 0ms extension, 200 cunning = 400ms extension
    const MS_PER_POINT: f32 = 2.0; // 2ms per cunning point
    let extension_ms = (defender_cunning as f32 * MS_PER_POINT).min(600.0); // cap at 600ms
    base_duration + Duration::from_millis(extension_ms as u64)
}
```

**Integration**:
- When threat is inserted into `ReactionQueue`, calculate window extension from target's Cunning
- Add extension to `timer_duration` for that specific threat
- Display extended window in UI (green portion of threat timer?)

**Tests**:
- ✅ `test_cunning_window_zero()` - 0 Cunning → no extension
- ✅ `test_cunning_window_linear()` - Cunning increases → window extends linearly
- ✅ `test_cunning_window_capped()` - Very high Cunning caps at 600ms extension
- ✅ `test_finesse_vs_cunning_integration()` - Fast synergy chain vs long reaction window

### 2.4 Integration Tests

**Tests**:
- ✅ `test_finesse_cunning_burst_battle()` - High-Finesse attacker vs high-Cunning defender
- ✅ `test_lockout_equation()` - Verify `chain_gap + reaction_window > lockout` works correctly
- ✅ `test_synergy_predictability_tradeoff()` - Confirm synergies are telegraphed but faster

**Acceptance Criteria**:
- Precision removed, Finesse added (all code and UI updated)
- Finesse tightens synergy burst sequences (observable in game)
- Cunning extends reaction windows (observable in UI and gameplay)
- All tests pass
- Synergy chains feel responsive with high Finesse, but can be reacted to with high Cunning

---

## Phase 3: Dominance/Toughness — Sustain Ratio

**Goal**: Implement healing reduction aura (Dominance) and verify Toughness mitigation, add minimal healing system.

### 3.1 Minimal Healing System

**Files to create**:
- `src/common/components/heal.rs` - Heal event component
- `src/common/systems/combat/heal.rs` - Healing application system

**New functionality**:

```rust
// heal.rs component
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents a healing event that needs to be applied
#[derive(Component, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PendingHeal {
    pub source: Entity,      // Who cast the heal
    pub amount: f32,          // Base healing amount (before Dominance reduction)
    pub heal_type: HealType,  // Physical/Magic for future expansion
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HealType {
    Physical, // Vitality-based healing (bandages, potions)
    Magic,    // Focus-based healing (spells, regeneration)
}

// heal.rs system
pub fn apply_healing_system(
    mut commands: Commands,
    mut heal_query: Query<(Entity, &PendingHeal, &Loc)>,
    mut health_query: Query<(&mut Health, &ActorAttributes, &Loc)>,
    dominance_query: Query<(&Loc, &ActorAttributes), With<Dominance>>,
) {
    for (heal_entity, pending_heal, heal_loc) in heal_query.iter() {
        if let Ok((mut health, attrs, loc)) = health_query.get_mut(heal_entity) {
            // Find strongest Dominance aura within 5 hexes
            let dominance_reduction = calculate_dominance_reduction(
                loc,
                attrs.toughness(),
                &dominance_query,
            );

            // Apply healing with Dominance reduction
            let effective_healing = pending_heal.amount * (1.0 - dominance_reduction);
            health.current = (health.current + effective_healing).min(attrs.max_health());

            // Remove pending heal
            commands.entity(heal_entity).remove::<PendingHeal>();
        }
    }
}
```

**Tests**:
- ✅ `test_healing_no_dominance()` - Base healing works without any Dominance auras
- ✅ `test_healing_applies_to_current_health()` - Healing increases current HP
- ✅ `test_healing_capped_at_max()` - Healing cannot exceed max_health()

### 3.2 Implement Dominance Aura

**Files to modify**:
- `src/common/components/mod.rs` - Add `Dominance` marker component
- `src/common/systems/combat/heal.rs` - Add aura calculation

**New functionality**:

```rust
// In components/mod.rs
/// Marker component for entities with Dominance aura
/// Presence-based entities that reduce healing effectiveness in area
#[derive(Component, Clone, Copy, Debug)]
pub struct Dominance;

// In heal.rs
pub fn calculate_dominance_reduction(
    target_loc: &Loc,
    target_toughness: u16,
    dominance_query: &Query<(&Loc, &ActorAttributes), With<Dominance>>,
) -> f32 {
    const AURA_RADIUS: u16 = 5; // hex distance

    // Find all Dominance sources within range
    let mut strongest_dominance = 0u16;

    for (dom_loc, dom_attrs) in dominance_query.iter() {
        let distance = target_loc.flat_distance(dom_loc);
        if distance <= AURA_RADIUS {
            let dominance_value = dom_attrs.dominance();
            strongest_dominance = strongest_dominance.max(dominance_value);
        }
    }

    if strongest_dominance == 0 {
        return 0.0; // No aura affecting target
    }

    // Contest: Dominance vs Toughness
    let contest_mod = contest_modifier(strongest_dominance, target_toughness);

    // Reduction factor: neutral = 50%, attacker advantage = 75%, defender advantage = 25%
    const BASE_REDUCTION: f32 = 0.50;
    let reduction = BASE_REDUCTION * contest_mod;
    reduction.clamp(0.0, 0.90) // cap at 90% reduction (always allow some healing)
}
```

**Integration**:
- Add `Dominance` component to high-Presence entities (players/NPCs with Presence commitment)
- Trigger component added when `intensity()` tier is T1+ (similar to how cadence works)
- Query all Dominance entities within 5 hex radius when healing is applied
- Apply only the strongest (worst-effect-wins)

**Tests**:
- ✅ `test_dominance_single_aura()` - One Dominance source reduces healing via contest
- ✅ `test_dominance_worst_effect_wins()` - Multiple auras → only strongest applies
- ✅ `test_dominance_range_limit()` - Dominance only affects targets within 5 hexes
- ✅ `test_dominance_vs_toughness()` - Contest modifier affects reduction percentage
- ✅ `test_dominance_stacking_prevented()` - Two 50% auras = 50%, not 75%

### 3.3 Verify Toughness Mitigation

**Files to verify**:
- `src/common/systems/combat/damage.rs` - `apply_passive_modifiers()` already implements Toughness mitigation

**Verification**:
- Toughness already maps to `vitality()` and is used in damage mitigation
- No changes needed, just verify tests cover the Vitality→Toughness mapping

**Tests**:
- ✅ `test_toughness_mitigation_exists()` - Verify `apply_passive_modifiers()` uses Vitality
- ✅ `test_toughness_vs_dominance_symmetry()` - Verify sustain ratio math is symmetric (see RFC example)

### 3.4 Integration Tests

**Tests**:
- ✅ `test_dominance_toughness_sustain_battle()` - High-Dominance vs high-Toughness builds
- ✅ `test_sustain_ratio_symmetry()` - RFC worked example: 60 dmg, 180 heal, 33% Dominance/Toughness → 3 hits per heal both ways
- ✅ `test_dominance_priority_target()` - Verify high-Dominance entities are rational priority targets

**Acceptance Criteria**:
- Minimal healing system implemented (PendingHeal, apply_healing_system)
- Dominance aura reduces healing within 5 hex radius (observable in game)
- Worst-effect-wins logic works (multiple auras don't stack)
- Toughness contests Dominance (sustain ratio affected by both stats)
- All tests pass
- Presence builds create sustain pressure (strategic value observable)

---

## Acceptance Criteria (All Phases)

### Functionality
- ✅ All three relative opposition pairs implemented and functional
- ✅ Critical hit system fully removed
- ✅ Contest formulas work consistently across all pairs
- ✅ Minimal healing system supports Dominance mechanics

### Code Quality
- ✅ All new code has unit tests
- ✅ Integration tests cover pair interactions
- ✅ No dead code (Precision, crit system cleaned up)
- ✅ Documentation updated (ADR-031, this SOW)

### Game Feel
- ✅ Impact/Composure creates observable tempo differences
- ✅ Finesse/Cunning creates burst vs reaction gameplay
- ✅ Dominance/Toughness creates sustain pressure gameplay
- ✅ Build diversity increases (opposing stats on different layers)

### UI/UX
- ✅ Character panel shows Finesse (not Precision)
- ✅ Combat UI shows recovery pushback/reduction
- ✅ Reaction queue UI shows Cunning window extension
- ✅ Healing UI shows Dominance reduction effect

## References

- RFC-021: Relative Meta-Attribute Opposition System
- ADR-031: Relative Meta-Attribute Opposition System (this implementation's foundation)
- ADR-029: Relative Stat Contests (contest_modifier formula)
- ADR-030: Reaction Queue Window Mechanic
- ADR-012: Universal Lockout (recovery system)
- ADR-003: Reaction Queue
- SOW-020: Attribute System Rework (Phase 4 provided contest framework)
