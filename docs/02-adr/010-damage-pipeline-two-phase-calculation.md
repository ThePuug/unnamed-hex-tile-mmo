# ADR-010: Damage Pipeline with Two-Phase Calculation

## Status

**Accepted** - 2025-10-31

## Context

**Related RFC:** [RFC-005: Damage Pipeline and Combat Resolution](../01-rfc/005-damage-pipeline.md)

Combat system requires complete damage flow: **abilities → queue → health → death**.

### Requirements

- Attribute-scaled damage (Might/Focus increase damage)
- Passive mitigation (Vitality/Focus reduce damage via armor/resistance)
- Critical hits (Instinct affects crit chance and multiplier)
- Fair damage calculation (handle mid-queue attribute changes)
- Server authoritative (prevent cheating)

### Options Considered

**Option 1: Single-Phase Calculation** - Calculate final damage at insertion
- ❌ Unfair (uses defender's attributes at insertion, not resolution)
- ❌ Breaks if attributes change mid-queue

**Option 2: Two-Phase Calculation** - Outgoing at insertion, mitigation at resolution
- ✅ Fair (attacker scaling at attack time, defender mitigation at resolution time)
- ✅ Handles attribute changes correctly
- ⚠️ More complex (two calculation steps)

**Option 3: Calculate at Resolution Only** - Store base damage, calculate everything at resolution
- ❌ Unfair (uses attacker's attributes at resolution, not attack time)
- ❌ Breaks if attacker dies before resolution

## Decision

**Use two-phase damage calculation (Option 2).**

### Core Mechanism

**Phase 1: Outgoing Damage (at insertion time)**
```rust
// Calculate with attacker's attributes at attack time
let scaling = match damage_type {
    DamageType::Physical => 1.0 + (attrs.might() as f32 / 100.0),
    DamageType::Magic => 1.0 + (attrs.focus() as f32 / 100.0),
};

let (was_crit, crit_mult) = roll_critical(attrs);
let outgoing_damage = base_damage * scaling * crit_mult;

// Store in QueuedThreat
threat.damage = outgoing_damage;
```

**Phase 2: Incoming Damage (at resolution time)**
```rust
// Apply with defender's attributes at resolution time
let mitigation = match damage_type {
    DamageType::Physical => (attrs.vitality() as f32 / 200.0).min(0.75),
    DamageType::Magic => (attrs.focus() as f32 / 200.0).min(0.75),
};

let final_damage = threat.damage * (1.0 - mitigation);
health.state -= final_damage;
```

**Critical Hit Formula:**
```rust
let instinct = attrs.instinct_presence() as f32;
let crit_chance = 0.05 + (instinct / 200.0);  // 5% base + Instinct
let crit_mult = if crit { 1.5 + (instinct / 200.0) } else { 1.0 };
```

### Pipeline Flow

**Server systems:**
1. `abilities::execute_ability` → Emit `Try::DealDamage { source, target, base, type }`
2. `damage::process_deal_damage` → Calculate outgoing, roll crit, insert into queue → Emit `Do::InsertThreat`
3. `queue::process_expired_threats` → Timer expires → Emit `Try::ResolveThreat`
4. `damage::process_resolve_threat` → Apply mitigation, apply to Health → Emit `Do::ApplyDamage`
5. `resources::check_death` → Health <= 0 → Emit `Try::Death`

**Client response:**
- Receive `Do::InsertThreat` → Show in queue UI
- Receive `Do::ApplyDamage` → Update health bar, show damage number
- Receive `Do::Despawn` → Remove entity

**Prediction (local player only):**
- Predict Health decrease at resolution time (not insertion)
- Use same `apply_passive_modifiers` function as server
- Rollback if threat cleared by Dodge

---

## Rationale

### 1. Fair Damage Calculation

**Attacker fairness:** Scaling reflects attacker's state at attack time
- Might=50 at attack → 1.5x damage, even if Might changes before resolution
- Crit rolled at attack time (attacker's Instinct)

**Defender fairness:** Mitigation reflects defender's state at resolution time
- Vitality=100 at resolution → 50% reduction, even if changed after attack
- Allows buffs/debuffs to affect incoming damage

### 2. Handles Attribute Changes

**Mid-queue scenarios:**
- Attacker gains buff after attack → damage already calculated (fair)
- Defender gains armor before resolution → mitigation applied correctly
- Attacker dies before resolution → outgoing damage preserved in threat

### 3. Server Authoritative

- Server calculates all damage (prevents cheating)
- Client predicts local player only (minimal rollback)
- Server validates attribute states at both phases

### 4. Extensible Design

Foundation supports future features:
- Damage over time (multiple threats from single ability)
- Shields (absorb before Health)
- Damage modifiers (buffs increase threat.damage before resolution)
- Reflect damage (create new threat in reverse direction)

---

## Consequences

### Positive

- **Fair damage:** Attacker/defender attributes at correct times
- **Handles buffs/debuffs:** Attribute changes mid-queue work correctly
- **Server authoritative:** Prevents cheating
- **Minimal client prediction:** Local player only, at resolution (rare rollback)
- **Extensible:** Clean hooks for future damage modifiers

### Negative

- **Two-phase complexity:** Must track damage through pipeline
- **Storage overhead:** QueuedThreat stores outgoing damage (not base)
- **Prediction errors:** Client predicts resolution, threat cleared by Dodge (rare)
- **RNG trust:** Client can't verify crit rolls (server-only)

### Mitigations

- Clear function names (`calculate_outgoing_damage`, `apply_passive_modifiers`)
- Comprehensive tests for both phases
- Comments in QueuedThreat struct explain stored damage
- Visual feedback on rollback (health restore effect)
- Future: Deterministic RNG seed for verifiable crits

---

## Implementation Notes

**Shared code:** `common/systems/damage.rs`
- `calculate_outgoing_damage(base, attrs, type) -> f32`
- `roll_critical(attrs) -> (bool, f32)`
- `apply_passive_modifiers(damage, attrs, type) -> f32`

**Server systems:** `server/systems/damage.rs`
- `process_deal_damage` - Phase 1, emit InsertThreat
- `process_resolve_threat` - Phase 2, emit ApplyDamage

**Client systems:** `client/systems/damage.rs`
- `predict_threat_resolution` - Predict local player Health
- `spawn_damage_numbers` - Floating text on ApplyDamage

**Network events:** `common/message.rs`
- `Try::DealDamage` - Server-internal
- `Try::ResolveThreat` - Server-internal
- `Do::ApplyDamage` - Broadcast to clients

**System schedule:**
```rust
FixedUpdate: (
    queue::process_expired_threats,   // Emit ResolveThreat
    damage::process_resolve_threat,   // Apply damage
    resources::check_death,            // Emit Death
).chain()  // Sequential, all damage applies before death checks
```

---

## Validation Criteria

**Functional:**
- Outgoing: base=20, Might=50 → 30
- Mitigation: damage=30, Vitality=100 → 15 (50% armor)
- Critical: Instinct=100 → ~55% crit chance, 2.0x multiplier
- Pipeline: Ability → InsertThreat → (timer) → ApplyDamage → Death

**Performance:**
- Damage processing: < 1ms per event
- 100 damage events/sec: < 10ms total
- Damage numbers: 60fps with 100 simultaneous

**Mutual Destruction:**
- Both entities have lethal damage queued
- Both die same frame (order-independent)

---

## References

- **RFC-005:** Damage Pipeline and Combat Resolution
- **Spec:** `docs/00-spec/combat-system.md` (damage formulas)
- **Related:** ADR-002 (Health), ADR-006/007/008 (queue), ADR-009 (abilities)

## Date

2025-10-31
