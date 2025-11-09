# ADR-003: Component-Based Resource Separation

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-002: Combat Foundation](../01-rfc/002-combat-foundation.md)

We need to represent combat resources (health, stamina, mana) in an ECS architecture. The fundamental architectural question: **Should we store all resources in a single component or separate them into distinct components?**

### Requirements

1. **Selective querying:** Systems should only access resources they need
2. **Entity flexibility:** Not all entities need all resources (decorators have no stamina/mana)
3. **Composition:** Enable adding/removing resource types per entity
4. **Consistency:** Match existing codebase component granularity

### Options Considered

#### Option 1: Monolithic Resources Component

Store all resources in one component:

```rust
#[derive(Component)]
pub struct Resources {
    // Health
    pub health: f32,
    pub health_step: f32,
    pub max_health: f32,

    // Stamina
    pub stamina: f32,
    pub stamina_step: f32,
    pub max_stamina: f32,
    pub stamina_regen_rate: f32,
    pub stamina_last_update: Duration,

    // Mana
    pub mana: f32,
    pub mana_step: f32,
    pub max_mana: f32,
    pub mana_regen_rate: f32,
    pub mana_last_update: Duration,
}
```

**Pros:**
- Single query fetches all resources
- All resource data co-located in memory
- One component to serialize/deserialize

**Cons:**
- Forces all entities to have all resources (decorators don't need stamina/mana)
- Over-fetching: System needs stamina, gets health/mana too
- Breaks ECS composition principle (monolith vs granular)
- Harder to extend (adding new resource requires changing component)
- Violates existing codebase pattern (Loc, Offset, Heading are separate)

#### Option 2: Separate Components per Resource

Each resource is its own component:

```rust
#[derive(Component)]
pub struct Health {
    pub state: f32,
    pub step: f32,
    pub max: f32,
}

#[derive(Component)]
pub struct Stamina {
    pub state: f32,
    pub step: f32,
    pub max: f32,
    pub regen_rate: f32,
    pub last_update: Duration,
}

#[derive(Component)]
pub struct Mana {
    pub state: f32,
    pub step: f32,
    pub max: f32,
    pub regen_rate: f32,
    pub last_update: Duration,
}

#[derive(Component)]
pub struct CombatState {
    pub in_combat: bool,
    pub last_action: Duration,
}
```

**Pros:**
- Selective insertion: Decorators get Health only, actors get all
- Selective querying: `Query<&Stamina>` vs `Query<(&Health, &Stamina, &Mana)>`
- Composition over monolith (ECS best practice)
- Easy to extend (add Energy resource without changing Health/Stamina/Mana)
- Matches existing granularity (Loc, Offset, Heading are separate)

**Cons:**
- Multiple queries needed if system needs all resources
- More components to serialize (marginal overhead)
- Slightly more verbose entity construction

## Decision

**We will use separate components for each resource type (Option 2).**

### Rationale

#### 1. ECS Composition Principle

**Core ECS Philosophy:** Entities are composed of granular components, not monolithic structures.

**Examples from existing codebase:**
- Position: `Loc` (authoritative) + `Offset` (prediction) + `Heading` (separate)
- NOT: `PositionAndHeading { loc, offset, heading }`

**Applying same principle to resources:**
- `Health` + `Stamina` + `Mana` (granular)
- NOT: `Resources { health, stamina, mana }` (monolithic)

#### 2. Entity Flexibility

**Different entity types need different resources:**

| Entity Type | Health | Stamina | Mana | Combat State |
|-------------|--------|---------|------|--------------|
| Player | ✅ | ✅ | ✅ | ✅ |
| Wild Dog (NPC) | ✅ | ✅ | ✅ | ✅ |
| Decorator (terrain) | ✅ | ❌ | ❌ | ❌ |
| Chest (interactive) | ✅ | ❌ | ❌ | ❌ |

**With monolithic Resources:**
- Must insert full component for decorators (wastes memory)
- Or: Use `Option<f32>` for optional fields (awkward API)

**With separate components:**
- Insert only what's needed
- Clean, type-safe API

#### 3. Selective Querying

**System examples:**

```rust
// Regeneration system - only needs Stamina and Mana
fn regenerate_resources(
    query: Query<(&mut Stamina, &mut Mana)>,  // Clean
) { }

// vs monolithic
fn regenerate_resources(
    query: Query<&mut Resources>,  // Over-fetches health, combat state
) { }

// Death detection - only needs Health
fn check_death(
    query: Query<(Entity, &Health)>,  // Clean
) { }

// vs monolithic
fn check_death(
    query: Query<(Entity, &Resources)>,  // Over-fetches stamina, mana, combat state
) { }
```

**Performance consideration:**
- ECS query filtering is optimized (archetype-based)
- Over-fetching wastes cache bandwidth (fetching unused fields)
- Granular queries = better cache utilization

#### 4. Extension without Modification

**Future scenario: Add "Energy" resource for tech-based characters**

**With separate components:**
```rust
#[derive(Component)]
pub struct Energy {
    pub state: f32,
    pub step: f32,
    pub max: f32,
    pub regen_rate: f32,
    pub last_update: Duration,
}

// Insert on tech characters only
commands.entity(tech_char).insert(Energy { .. });

// Query only on systems that need it
fn tech_ability_system(
    query: Query<&mut Energy>,
) { }
```

**With monolithic Resources:**
```rust
// Must modify existing Resources struct
pub struct Resources {
    pub health: f32,
    // ... existing fields
    pub energy: Option<f32>,  // Awkward Option for optional resource
    pub energy_step: Option<f32>,
    // ... more Optional fields
}

// Every entity now carries Energy fields (even if None)
// Network serialization includes Optional energy fields
// Every query fetches energy fields even if unused
```

**Verdict:** Separate components enable extension without modifying existing code.

---

## Consequences

### Positive

**1. Clean Entity Construction**

```rust
// Combat entity (player/NPC)
commands.entity(ent)
    .insert(Health { state: max_hp, step: max_hp, max: max_hp })
    .insert(Stamina { state: max_stam, step: max_stam, max: max_stam, regen_rate: 10.0, last_update: time.elapsed() })
    .insert(Mana { state: max_mana, step: max_mana, max: max_mana, regen_rate: 8.0, last_update: time.elapsed() })
    .insert(CombatState { in_combat: false, last_action: time.elapsed() });

// Non-combat entity (decorator)
commands.entity(ent)
    .insert(Health { state: 100.0, step: 100.0, max: 100.0 });
    // No stamina/mana - clean and explicit
```

**2. Selective System Queries**

```rust
// Regeneration - only mutable access to regenerating resources
fn regenerate(query: Query<(&mut Stamina, &mut Mana)>) { }

// Death check - only read access to health
fn check_death(query: Query<(Entity, &Health)>) { }

// Combat state - only combat state component
fn update_combat_state(query: Query<&mut CombatState>) { }
```

**3. Matches Existing Patterns**

**Position/movement components:**
- `Loc` - authoritative position
- `Offset` - client prediction
- `Heading` - direction
- All separate, not `Movement { loc, offset, heading }`

**Resource components:**
- `Health` - health pool
- `Stamina` - stamina pool
- `Mana` - mana pool
- `CombatState` - combat state
- All separate, following same principle

**4. Network Serialization Efficiency**

```rust
// Only serialize what changed
Event::Health { ent, current: 75.0, max: 100.0 }  // 12 bytes
Event::Stamina { ent, current: 50.0, max: 120.0, regen_rate: 10.0 }  // 16 bytes

// vs monolithic (must serialize entire Resources struct even if only health changed)
Event::ResourcesUpdate { ent, resources: Resources { /* all fields */ } }  // 60+ bytes
```

### Negative

**1. Multiple Queries for Multi-Resource Systems**

```rust
// Damage system needs health and armor (from attributes)
fn apply_damage(
    health_query: Query<&mut Health>,
    attrs_query: Query<&ActorAttributes>,
) {
    // More verbose than single Resources query
}

// vs monolithic
fn apply_damage(
    query: Query<(&mut Resources, &ActorAttributes)>,
) { }
```

**Mitigation:** This is rare. Most systems need only 1-2 resources. The verbosity tradeoff is worth the flexibility.

**2. More Components to Track**

- 4 components instead of 1
- More serialization calls (marginal overhead)
- More component insertion on spawn

**Mitigation:** ECS engines are optimized for many small components. This is the intended usage pattern.

### Neutral

**Memory Layout:**

Separate components change memory layout (archetype-based storage):
- Entities with `(Health, Stamina, Mana)` in one archetype
- Entities with `(Health)` only in different archetype

**Effect:**
- Better cache locality for selective queries (query Health-only entities, no stamina/mana in cache)
- Marginal indirection cost (archetype lookup)
- Net: Performance neutral or slightly positive

---

## Alternatives Rejected

**Hybrid approach (Resources + Optional components):**
```rust
pub struct Resources {
    pub health: f32,
    pub max_health: f32,
}

pub struct Stamina { /* stamina-specific */ }  // Optional component
pub struct Mana { /* mana-specific */ }  // Optional component
```

**Rejected because:**
- Inconsistent (why Health in Resources but Stamina separate?)
- Still requires monolithic Resources for health
- Doesn't solve over-fetching for health systems
- Half-measure that gets worst of both approaches

---

## Implementation Notes

See [SOW-002](../03-sow/002-combat-foundation.md) for implementation details.

**Component locations:**
- `common/components/resources.rs` - All four components (Health, Stamina, Mana, CombatState)

**Network events:**
- `Event::Health { ent, current, max }`
- `Event::Stamina { ent, current, max, regen_rate }`
- `Event::Mana { ent, current, max, regen_rate }`
- `Event::CombatState { ent, in_combat }`

Each resource has its own event type (granular network sync).

---

## References

- **RFC-002:** Combat Foundation feature request
- **ADR-002:** Server-Authoritative Resource Management (authority model)
- **ADR-004:** Deterministic Resource Regeneration (how regeneration works)
- **Existing Pattern:** `Loc`, `Offset`, `Heading` as separate components

---

## Date

2025-10-29
