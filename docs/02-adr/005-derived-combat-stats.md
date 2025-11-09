# ADR-005: Derived Combat Stats (On-Demand Calculation)

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-002: Combat Foundation](../01-rfc/002-combat-foundation.md)

Combat stats (armor, resistance) derive mathematically from attributes. The fundamental architectural question: **Should we calculate these stats on-demand or store them in components?**

### Formulas (from combat spec)

```
armor = base_armor + (vitality / 200.0)  // Max 75% cap
resistance = base_resistance + (focus / 200.0)  // Max 75% cap
```

### Requirements

1. **Single source of truth:** Attributes are already stored and synchronized
2. **Network efficiency:** Minimize redundant state synchronization
3. **Consistency:** Derived values must always match current attributes
4. **Performance:** Calculations must be fast enough for combat frequency

### Options Considered

#### Option 1: Store in Components

Pre-calculate and store armor/resistance in components:

```rust
#[derive(Component)]
pub struct CombatStats {
    pub armor: f32,
    pub resistance: f32,
}

// On attribute change
fn update_combat_stats(
    mut query: Query<(&ActorAttributes, &mut CombatStats)>,
) {
    for (attrs, mut stats) in &mut query {
        stats.armor = calculate_armor(attrs);
        stats.resistance = calculate_resistance(attrs);
    }
}

// On damage
fn apply_damage(
    query: Query<(&CombatStats, &mut Health)>,
) {
    for (stats, mut health) in &mut query {
        let reduced = damage * (1.0 - stats.armor);  // Use stored value
    }
}
```

**Pros:**
- No calculation during combat (stored value ready)
- Single query fetches combat stats

**Cons:**
- **Duplicate state:** Armor/resistance AND attributes both stored
- **Sync overhead:** Must update CombatStats whenever attributes change
- **Network traffic:** Send CombatStats updates + attribute updates
- **Desync risk:** If calculation changes, stored values become stale
- **Memory overhead:** 8 bytes per entity (2 floats)

---

#### Option 2: Calculate On-Demand

Calculate armor/resistance from attributes when needed:

```rust
// Pure functions in common/systems/resources.rs
pub fn calculate_armor(attrs: &ActorAttributes) -> f32 {
    let armor = (attrs.vitality() / 200.0).min(0.75);
    armor
}

pub fn calculate_resistance(attrs: &ActorAttributes) -> f32 {
    let resistance = (attrs.focus() / 200.0).min(0.75);
    resistance
}

// On damage - calculate from attributes
fn apply_damage(
    attrs_query: Query<&ActorAttributes>,
    mut health_query: Query<&mut Health>,
) {
    let attrs = attrs_query.get(target).unwrap();
    let armor = calculate_armor(attrs);  // Calculate on-demand
    let reduced = damage * (1.0 - armor);
}
```

**Pros:**
- **No duplicate state:** Only attributes stored
- **No sync overhead:** Attributes already synchronized
- **Always consistent:** Derived values always match current attributes
- **Easy to tune:** Change formula, immediately affects all entities
- **Zero network traffic:** Armor/resistance not sent (derived from attributes)

**Cons:**
- Calculation cost: Division + clamping per damage event
- Two queries in damage system (attributes + health)

## Decision

**We will calculate armor and resistance on-demand from attributes (Option 2).**

### Core Principle

**Derived values should not be stored if:**
1. Source data (attributes) is already stored
2. Calculation is cheap (< 100 CPU cycles)
3. Source data changes infrequently
4. Derived value is not needed every frame

**Armor/resistance meets all criteria:**
1. ✅ Attributes already stored and synchronized
2. ✅ Calculation is cheap: `vitality / 200.0` (one division, one min)
3. ✅ Attributes change rarely (level-up, gear) in MVP
4. ✅ Armor/resistance only needed on damage events (infrequent)

---

## Rationale

### 1. Avoid Duplicate State

**State Duplication Problem:**

```
ActorAttributes { vitality: 100, focus: 50 }
  ↓
CombatStats { armor: 0.5, resistance: 0.25 }
```

**Two representations of same information:**
- Vitality = 100 → Armor = 0.5 (stored twice)
- Focus = 50 → Resistance = 0.25 (stored twice)

**Consequences:**
- Memory: 8 bytes × 1,000 entities = 8 KB wasted
- Network: Must sync both attributes AND combat stats
- Desync risk: If attributes update but combat stats don't
- Maintenance: Change formula → must update stored values

**On-demand calculation eliminates duplication:**
- Only attributes stored (single source of truth)
- Armor/resistance calculated when needed
- Always consistent (no desync possible)

### 2. Network Efficiency

**Option 1 (Stored) - Network Traffic:**

Scenario: Player levels up, +10 vitality

```
1. Client: Level up
2. Server: Updates ActorAttributes { vitality: 100 → 110 }
3. Server: Calculates new armor (0.5 → 0.55)
4. Server: Updates CombatStats { armor: 0.55 }
5. Server → Client: Event::Attributes { vitality: 110 }  // 12 bytes
6. Server → Client: Event::CombatStats { armor: 0.55 }    // 12 bytes
Total: 24 bytes
```

**Option 2 (On-Demand) - Network Traffic:**

```
1. Client: Level up
2. Server: Updates ActorAttributes { vitality: 100 → 110 }
3. Server → Client: Event::Attributes { vitality: 110 }  // 12 bytes
4. Client: Calculates new armor (0.5 → 0.55) locally
Total: 12 bytes
```

**50% reduction in network traffic** for attribute changes.

**Frequency consideration:**
- MVP: No leveling, no gear → attributes never change → **0 bytes/sec**
- Future: Leveling system → attributes change ~1/hour → **negligible**

### 3. Calculation Performance

**Armor calculation:**
```rust
pub fn calculate_armor(attrs: &ActorAttributes) -> f32 {
    (attrs.vitality() / 200.0).min(0.75)
}
```

**CPU cost:**
- `vitality()` getter: 1 cycle (return field)
- Division: ~15 cycles
- `min()`: 2 cycles (comparison + select)
- **Total: ~20 cycles** (~10 nanoseconds on modern CPU)

**Damage event frequency:**
- 10 players in combat
- ~1 damage event/sec per player
- 10 calculations/sec × 20 cycles = **200 cycles/sec**
- **CPU usage: < 0.001%**

**Verdict:** Calculation is essentially free. No performance benefit to storing.

### 4. Matches Existing Pattern

**ActorAttributes already has derived stats:**

```rust
impl ActorAttributes {
    pub fn max_health(&self) -> f32 {
        100.0 + (self.vitality() as f32 * 0.5) + (self.might() as f32 * 0.3)
    }

    pub fn movement_speed(&self) -> f32 {
        1.0 + (self.grace() as f32 / 100.0)
    }
}
```

**These are calculated on-demand, NOT stored:**
- `max_health()` calculated when needed
- `movement_speed()` calculated when needed
- No `MaxHealth` or `MovementSpeed` components

**Armor/resistance follow same pattern:**
```rust
pub fn calculate_armor(attrs: &ActorAttributes) -> f32;
pub fn calculate_resistance(attrs: &ActorAttributes) -> f32;
```

**Consistency:** All derived stats use same approach.

### 5. Formula Tuning Flexibility

**Option 1 (Stored) - Formula Change:**

Change armor formula: `vitality / 200.0` → `vitality / 150.0`

```
1. Update calculate_armor() function
2. Update stored CombatStats for ALL entities
3. Send network updates to clients (1,000 messages)
4. Hope no entities missed (desync)
```

**Option 2 (On-Demand) - Formula Change:**

```
1. Update calculate_armor() function
2. Done. All future calculations use new formula automatically.
```

**On-demand enables instant formula tuning:**
- Change code, next damage calculation uses new formula
- No entity migration needed
- No network updates needed
- No stale values possible

---

## Consequences

### Positive

**1. Single Source of Truth**

Attributes are the authoritative source for:
- Max health (calculated)
- Movement speed (calculated)
- Armor (calculated)
- Resistance (calculated)

**No duplicate state, no desync possible.**

**2. Network Efficiency**

Attribute changes:
- Option 1 (stored): 24 bytes (attributes + combat stats)
- Option 2 (on-demand): 12 bytes (attributes only)

**50% reduction** in attribute change traffic.

**3. Formula Flexibility**

Balance tuning:
- Change armor formula → affects all entities immediately
- No migration, no stored value updates
- Easy to iterate during development

**4. Memory Efficiency**

Per entity savings:
- Option 1: 8 bytes (armor + resistance floats)
- Option 2: 0 bytes

1,000 entities: **8 KB saved** (marginal but clean)

**5. Zero Desync Risk**

Derived values always calculated from current attributes:
- Impossible to have stale armor value
- Impossible to forget to update CombatStats
- Impossible for client-server combat stats to diverge

### Negative

**1. Calculation Overhead**

Every damage event triggers calculation:
- Division + clamp = ~20 CPU cycles
- **Negligible** (< 0.001% CPU for 10 damage events/sec)

**2. Multiple Queries in Damage System**

```rust
fn apply_damage(
    attrs_query: Query<&ActorAttributes>,
    health_query: Query<&mut Health>,
) {
    let attrs = attrs_query.get(target)?;
    let armor = calculate_armor(attrs);

    let health = health_query.get_mut(target)?;
    // Apply damage
}
```

**vs stored:**
```rust
fn apply_damage(
    query: Query<(&CombatStats, &mut Health)>,
) {
    for (stats, mut health) in &mut query {
        let armor = stats.armor;  // Slightly cleaner
    }
}
```

**Mitigation:** This is a minor verbosity increase. The benefits (no duplication, network efficiency) outweigh.

### Neutral

**Calculation Location:**

Where to call `calculate_armor()`?

**Option A:** In damage calculation system
```rust
fn apply_damage() {
    let armor = calculate_armor(attrs);
    let reduced = damage * (1.0 - armor);
}
```

**Option B:** As ActorAttributes method
```rust
impl ActorAttributes {
    pub fn armor(&self) -> f32 {
        (self.vitality() / 200.0).min(0.75)
    }
}

fn apply_damage() {
    let armor = attrs.armor();
    let reduced = damage * (1.0 - armor);
}
```

**Decision:** Use standalone functions in `common/systems/resources.rs`

**Rationale:**
- Matches existing pattern (`calculate_max_stamina`, `calculate_max_mana`)
- Keeps ActorAttributes clean (pure attribute storage)
- Combat formulas separate from attribute definition
- Easier to test (pure functions)

---

## Implementation Details

**Function signatures:**

```rust
// common/systems/resources.rs

/// Calculate armor from vitality (75% cap)
pub fn calculate_armor(attrs: &ActorAttributes) -> f32 {
    (attrs.vitality() as f32 / 200.0).min(0.75)
}

/// Calculate resistance from focus (75% cap)
pub fn calculate_resistance(attrs: &ActorAttributes) -> f32 {
    (attrs.focus() as f32 / 200.0).min(0.75)
}
```

**Usage in damage system (future ADR-005):**

```rust
fn apply_damage(
    attrs_query: Query<&ActorAttributes>,
    mut health_query: Query<&mut Health>,
    damage_events: EventReader<DamageEvent>,
) {
    for event in damage_events.read() {
        let attrs = attrs_query.get(event.target)?;
        let armor = calculate_armor(attrs);
        let reduced_damage = event.amount * (1.0 - armor);

        let mut health = health_query.get_mut(event.target)?;
        health.state -= reduced_damage;
    }
}
```

**Testing:**

```rust
#[test]
fn test_armor_baseline() {
    let attrs = ActorAttributes::default();  // vitality = 0
    assert_eq!(calculate_armor(&attrs), 0.0);
}

#[test]
fn test_armor_with_vitality() {
    let mut attrs = ActorAttributes::default();
    attrs.set_vitality(150);
    assert_eq!(calculate_armor(&attrs), 0.75);  // 150/200 = 0.75 (at cap)
}

#[test]
fn test_armor_cap() {
    let mut attrs = ActorAttributes::default();
    attrs.set_vitality(200);
    assert_eq!(calculate_armor(&attrs), 0.75);  // Capped at 75%
}
```

---

## Alternatives Rejected

**Hybrid: Store in component, recalculate on attribute change:**

```rust
fn update_combat_stats(
    mut query: Query<(&ActorAttributes, &mut CombatStats), Changed<ActorAttributes>>,
) {
    for (attrs, mut stats) in &mut query {
        stats.armor = calculate_armor(attrs);
    }
}
```

**Rejected because:**
- Still has duplicate state (attributes + combat stats)
- Still requires network sync (attributes + combat stats)
- `Changed<ActorAttributes>` filter adds complexity
- Doesn't solve core problem (state duplication)
- On-demand calculation is simpler and equally fast

---

## References

- **RFC-002:** Combat Foundation feature request
- **ADR-002:** Server-Authoritative Resource Management
- **SOW-002:** Combat Foundation implementation (Phase 1 includes armor/resistance tests)
- **Existing Pattern:** `ActorAttributes::max_health()` and `movement_speed()` (calculated on-demand)

---

## Date

2025-10-29
