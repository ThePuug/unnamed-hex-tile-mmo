# ADR-004: Ability System and Targeting

## Status

Proposed

## Context

### Combat System Requirements

From `docs/spec/combat-system.md`, abilities must support:

1. **Hex-Based Targeting:**
   - Single hex (click target)
   - Line pattern (N hexes in line from caster)
   - Radius pattern (all hexes within distance R)
   - Adjacent (hexes directly adjacent to caster)

2. **Execution Patterns:**
   - **Instant:** Resolve immediately on cast (melee attacks)
   - **Projectile:** Visible projectile travels, hit on impact
   - **Ground Effect:** Telegraph appears, damage after delay
   - **Unavoidable:** Bypass reaction queue entirely

3. **Triumvirate Integration:**
   - Abilities tied to Approach/Resilience (signature skills)
   - Direct → Charge, Distant → Volley, Ambushing → Trap, etc.
   - Each build has 2 Approach + 2 Resilience abilities

4. **Resource Costs:**
   - Stamina for physical abilities
   - Mana for magic abilities
   - No cooldowns (except 0.5s GCD for reactions)

### Current Codebase State

**GCD System Exists:** `common/systems/gcd.rs` has `GcdType` enum and basic structure

**Triumvirate Complete:** `ActorImpl` in `common/components/entity_type/actor.rs` has Origin, Approach, Resilience

**No Ability System:** No ability definitions, no targeting system, no casting flow

### Architectural Challenges

#### Challenge 1: Ability Data Structure

**Problem:** How to define abilities (data-driven vs code-driven)?

**Options:**
- **Hardcoded Rust structs:** Fast, type-safe, requires recompile for changes
- **Data files (JSON/RON):** Flexible, moddable, slower parsing, less type safety
- **Hybrid:** Core abilities hardcoded, later support data-driven abilities

**Considerations:**
- MVP needs ~5 abilities (Basic Attack, Dodge, maybe 2 more)
- Balance tuning requires frequent cost/damage changes
- Future: 50+ abilities across all Triumvirate combinations

#### Challenge 2: Targeting Validation

**Problem:** Client and server must agree on valid targets.

- Client shows targeting indicator (which hexes will be hit)
- Server validates target location (in range, line-of-sight, entity present)
- Latency: Entity may move between client cast and server validation

**Validation concerns:**
- Range check (caster to target distance <= max_range)
- Line-of-sight (no obstacles blocking path)
- Entity occupancy (entity at target hex when ability resolves)
- Targeting patterns match (client highlights same hexes server hits)

#### Challenge 3: Projectile System

**Problem:** How to represent traveling projectiles (physics vs discrete)?

**Options:**
- **Physics-based:** Entities with velocity, collision detection (complex, smooth)
- **Discrete hex jumps:** Projectile teleports hex-by-hex (simple, janky)
- **Interpolated path:** Calculate path on cast, interpolate position (middle ground)

**Considerations:**
- Projectiles provide visual warning (see arrow coming)
- Travel time allows dodging by moving off targeted hex
- Must handle projectile despawn (hit target, max range, obstacle)

#### Challenge 4: Telegraph System

**Problem:** Ground indicators before damage (visual + timing).

- Server decides when to telegraph (0.5-2s before damage)
- Client renders ground effect (red hexes, expanding ring, etc.)
- Server triggers damage after delay (fixed duration)
- Entities can move off telegraphed hexes (dodging by positioning)

**Timing concerns:**
- Telegraph duration must match between client/server
- Latency may cause visual mismatch (client sees telegraph, server hasn't started)
- Multiple telegraphs overlapping (visual clarity)

#### Challenge 5: Ability → Triumvirate Mapping

**Problem:** How to grant abilities based on Approach/Resilience?

- Each `ActorImpl` has `Approach` and `Resilience` enums
- Each combo should grant 2-4 signature abilities
- Players may not have all abilities at once (progression system future)

**Mapping options:**
- **Component per ability:** `HasCharge`, `HasDodge` (explicit, queryable)
- **Ability list component:** `AvailableAbilities(Vec<AbilityType>)` (flexible)
- **Derive from ActorImpl:** Calculate abilities on-demand from Approach/Resilience (no storage)

### MVP Requirements

From combat spec MVP:

**Player Abilities:**
- Basic Attack (instant, adjacent hex, 0 cost, physical damage)
- Dodge (clear queue, 30 stamina, 0.5s GCD)

**Enemy Abilities:**
- Wild Dog: Basic melee attack (instant, adjacent, 15 damage, 2s cooldown)

**Targeting:**
- Single hex targeting only (for Basic Attack)
- No projectiles (instant abilities)
- No telegraphs (no ground effects yet)

**Scope Simplifications:**
- No Triumvirate ability grants yet (hardcode Basic Attack + Dodge for all)
- No complex targeting patterns (line, radius, adjacent)
- No projectile physics
- No ability progression/unlocking

## Decision

We will implement a **hybrid ability system with hardcoded core abilities and extensible targeting patterns**, prioritizing MVP simplicity while designing for future complexity.

### Core Architectural Principles

#### 1. Ability Definitions as Rust Enums

**Ability Data Structure:**

```rust
pub enum AbilityType {
    // Offensive
    BasicAttack,
    Charge,          // Direct signature (future)
    Volley,          // Distant signature (future)

    // Defensive (Reactions)
    Dodge,           // Evasive signature
    Counter,         // Patient signature (future)
    Ward,            // Shielded signature (future)
}

pub struct AbilityDefinition {
    pub ability_type: AbilityType,
    pub cost: AbilityCost,
    pub gcd_type: GcdType,
    pub targeting: TargetingPattern,
    pub execution: ExecutionType,
    pub effects: Vec<AbilityEffect>,
}

pub enum AbilityCost {
    None,
    Stamina(f32),
    Mana(f32),
}

pub enum TargetingPattern {
    Single { max_range: u8 },
    Line { length: u8 },
    Radius { range: u8, radius: u8 },
    Adjacent,
    SelfTarget,  // Dodge, Ward (no targeting required)
}

pub enum ExecutionType {
    Instant,
    Projectile { speed: f32, lifetime: Duration },
    GroundEffect { delay: Duration },
    Unavoidable,
}

pub enum AbilityEffect {
    Damage { amount: f32, damage_type: DamageType },
    ClearQueue { clear_type: ClearType },
    // Future: Heal, Buff, Debuff, Knockback, etc.
}
```

**Rationale:**
- **Hardcoded for MVP:** Fast, type-safe, easy to refactor
- **Enum-driven:** Match expressions exhaustive (compiler enforces handling)
- **Extensible:** Add new ability types without breaking existing code
- **Future data-driven:** Can load from JSON/RON later, deserialize into enums

**Ability Registry:** `common/systems/abilities.rs`

```rust
pub fn get_ability_definition(ability_type: AbilityType) -> AbilityDefinition {
    match ability_type {
        AbilityType::BasicAttack => AbilityDefinition {
            ability_type: AbilityType::BasicAttack,
            cost: AbilityCost::None,
            gcd_type: GcdType::Attack,
            targeting: TargetingPattern::Single { max_range: 1 },
            execution: ExecutionType::Instant,
            effects: vec![
                AbilityEffect::Damage {
                    amount: 20.0,  // Base damage, scaled by Might
                    damage_type: DamageType::Physical,
                }
            ],
        },
        AbilityType::Dodge => AbilityDefinition {
            ability_type: AbilityType::Dodge,
            cost: AbilityCost::Stamina(30.0),
            gcd_type: GcdType::Reaction,
            targeting: TargetingPattern::SelfTarget,
            execution: ExecutionType::Instant,
            effects: vec![
                AbilityEffect::ClearQueue {
                    clear_type: ClearType::All,
                }
            ],
        },
        // Future abilities...
    }
}
```

**Benefits:**
- Single source of truth for ability data
- Easy to tune balance (change `amount: 20.0` → `amount: 25.0`)
- Type system prevents invalid abilities (can't have projectile with SelfTarget)

---

#### 2. Shared Targeting Validation in `common/`

**Targeting Module:** `common/systems/targeting.rs`

Functions used by both client and server:

```rust
pub fn get_targeted_hexes(
    caster_loc: Loc,
    target_loc: Loc,
    pattern: &TargetingPattern,
    map: &Map,
) -> Vec<Qrz>;

pub fn is_valid_target(
    caster_loc: Loc,
    target_loc: Loc,
    pattern: &TargetingPattern,
    map: &Map,
) -> bool;

pub fn get_entities_at_hexes(
    hexes: &[Qrz],
    nntree: &NNTree,
) -> Vec<Entity>;
```

**`get_targeted_hexes` logic:**
- `Single { max_range }`: Return [target_loc] if distance <= max_range
- `Line { length }`: Return hexes in line from caster toward target (up to length)
- `Radius { range, radius }`: Return all hexes within radius of target (if target in range)
- `Adjacent`: Return 6 hexes adjacent to caster (qrz neighbors)
- `SelfTarget`: Return [caster_loc]

**`is_valid_target` checks:**
- Distance <= max_range
- Line-of-sight clear (no solid decorators blocking path) - future
- Target hex is valid terrain (exists in map, not void)

**`get_entities_at_hexes` query:**
- Use NNTree to find entities at exact Loc (distance == 0)
- Filter to EntityType::Actor (only actors can be hit)
- Return list of entities (may be multiple in AOE)

**Benefits:**
- Client and server use identical targeting logic (no desync)
- Easy to test (pure functions, no ECS queries in validation)
- Extensible (add new patterns without rewriting validation)

---

#### 3. Client-Predicted Ability Usage

**Prediction Flow:**

1. **Client initiates:**
   - Player presses attack key (e.g., Left Click on target hex)
   - Client validates targeting (is_valid_target)
   - Client predicts ability effects:
     - Spend resource (stamina/mana step -= cost)
     - Apply effects optimistically (damage prediction in ADR-005)
   - Client sends `Try::UseAbility { ent, ability_type, target_loc }`

2. **Server validates:**
   - Check resource cost available
   - Check GCD not active (from ADR-002 GCD system)
   - Check targeting valid (is_valid_target with server's map/entity state)
   - If valid: spend resource, execute ability, emit confirmations
   - If invalid: emit `Do::AbilityFailed { ent, ability_type, reason }`

3. **Server broadcasts:**
   - `Do::AbilityUsed { ent, ability_type, target_loc }`
   - `Do::Stamina/Mana { ent, current, max }` (updated resource)
   - `Do::Gcd { ent, typ, duration }`
   - Ability effects (damage, queue clear, etc. - separate events)

4. **Client receives confirmation:**
   - Local player: effects already applied (prediction correct)
   - Remote players: play ability animation, apply effects
   - If rollback: `Do::AbilityFailed` → undo predicted changes

**Rationale:**
- Instant feedback for local player (critical for combat feel)
- Server authority prevents cheating (range hacks, cost bypassing)
- Remote players see confirmed abilities only (no prediction needed)

---

#### 4. Instant Ability Execution (MVP)

**Server System:** `server/systems/abilities::execute_ability`

**Processes:** `Try::UseAbility` events

**Flow for BasicAttack:**
```
1. Get ability definition (BasicAttack)
2. Validate targeting (target hex in range 1)
3. Get entities at target hex (NNTree query)
4. For each entity:
   - Calculate damage (base * (1 + might/100))
   - Insert into ReactionQueue (ADR-003)
   - Emit Do::InsertThreat { ent: target, threat }
5. Emit Do::AbilityUsed { ent: caster, ability_type, target_loc }
6. Apply GCD (0.5s for Attack)
```

**Flow for Dodge:**
```
1. Get ability definition (Dodge)
2. Validate: queue not empty, stamina >= 30
3. Clear queue (ADR-003 clear_threats function)
4. Spend stamina
5. Emit Do::ClearQueue { ent, clear_type: All }
6. Emit Do::Stamina { ent, current, max }
7. Apply GCD (0.5s for Reaction)
```

**Client Response:**
- Receive `Do::AbilityUsed` → play attack animation
- Receive `Do::InsertThreat` → insert into target's visual queue (if visible)
- Receive `Do::ClearQueue` → clear local queue (already done if predicted)

**Benefits:**
- Instant abilities simple to implement (no projectile/telegraph complexity)
- Validates MVP combat loop (attack → queue → reaction)
- Foundation for projectile/ground effect abilities (Phase 2)

---

#### 5. GCD Integration

**GCD Types (from spec):**
- `Attack`: 0.5s (Basic Attack, offensive abilities)
- `Reaction`: 0.5s (Dodge, Counter, Ward)
- Other types future (Spawn, PlaceSpawner already defined in gcd.rs)

**GCD Component:** Already exists in `common/systems/gcd.rs`

**Integration Points:**
1. Server checks GCD before executing ability (validation)
2. Server applies GCD after successful ability use
3. Client displays GCD indicator (dimmed ability icons)
4. GCD duration from ability definition

**Shared GCD vs Per-Ability Cooldowns:**
- MVP: All reactions share 0.5s GCD (prevents spamming Dodge)
- Offensive abilities share separate 0.5s GCD
- Future: Individual ability cooldowns (Charge: 5s, Fireball: 3s)

**GCD System Enhancement:**
```rust
// Already exists, extend with ability validation
pub fn is_gcd_active(ent: Entity, gcd_type: GcdType, gcd_resource: &Res<Gcd>) -> bool;
pub fn apply_gcd(ent: Entity, gcd_type: GcdType, duration: Duration, gcd_resource: &mut ResMut<Gcd>);
```

---

#### 6. Ability Availability (Triumvirate Mapping)

**MVP Approach: Universal Abilities**

All actors have Basic Attack and Dodge (no Triumvirate filtering):

```rust
// Simple check for MVP
pub fn has_ability(entity: Entity, ability_type: AbilityType) -> bool {
    match ability_type {
        AbilityType::BasicAttack => true,  // Everyone has basic attack
        AbilityType::Dodge => true,        // Everyone has dodge
        _ => false,  // Future abilities require Triumvirate check
    }
}
```

**Future: Triumvirate-Based Grants**

Query `ActorImpl` for Approach/Resilience:

```rust
pub fn get_available_abilities(actor_impl: &ActorImpl) -> Vec<AbilityType> {
    let mut abilities = vec![AbilityType::BasicAttack];  // Everyone

    // Approach abilities (2 per Approach)
    match actor_impl.approach {
        Approach::Direct => {
            abilities.push(AbilityType::Charge);
            abilities.push(AbilityType::Slam);
        },
        Approach::Distant => {
            abilities.push(AbilityType::Volley);
            abilities.push(AbilityType::Snipe);
        },
        // ... other Approaches
    }

    // Resilience abilities (2 per Resilience)
    match actor_impl.resilience {
        Resilience::Vital => {
            abilities.push(AbilityType::Endure);
            abilities.push(AbilityType::Regenerate);
        },
        Resilience::Shielded => {
            abilities.push(AbilityType::Ward);
            abilities.push(AbilityType::Fortify);
        },
        // ... other Resiliences
    }

    abilities
}
```

**Component:** `AvailableAbilities(Vec<AbilityType>)` (optional, cache results)

**Benefits:**
- MVP defers complexity (all have same abilities)
- Future integration clear (just implement get_available_abilities)
- UI can query available abilities for hotbar display

---

#### 7. Targeting UI (Client-Side)

**Targeting Indicator System:** `client/systems/targeting_ui.rs`

**Functionality:**
- Hover over hex → highlight hex (white outline)
- Ability selected → show range indicator (circle around player)
- Valid target → green highlight
- Invalid target → red highlight
- Click → send UseAbility with target_loc

**Visual Feedback:**
- Range circle: Shader ring at max_range distance from player
- Hex highlight: Overlay sprite on hex
- Targeting line: Line from player to target (for Line pattern abilities)

**Update Logic:**
- Run in `Update` schedule (every frame)
- Raycast from camera to world (mouse position → hex coordinate)
- Validate targeting with `is_valid_target` function
- Update highlight color (green/red) and position

**MVP Simplification:**
- Only Single targeting (no line/radius/adjacent indicators yet)
- Basic Attack range 1 (adjacent hexes only)
- Simple hover highlight (no fancy visuals)

---

#### 8. Network Message Structure

**New Event Types (add to `common/message.rs`):**

```rust
pub enum Event {
    // Existing events...

    /// Client → Server: Attempt to use ability (Try event)
    UseAbility {
        ent: Entity,
        ability_type: AbilityType,
        target_loc: Loc,  // Target hex (None for SelfTarget abilities)
    },

    /// Server → Client: Ability successfully used (Do event)
    AbilityUsed {
        ent: Entity,
        ability_type: AbilityType,
        target_loc: Loc,
    },

    /// Server → Client: Ability usage failed (Do event)
    AbilityFailed {
        ent: Entity,
        ability_type: AbilityType,
        reason: String,
    },
}
```

**Event Classification:**
- **Try events:** `UseAbility`
- **Do events:** `AbilityUsed`, `AbilityFailed`

**Message Size:**
- `UseAbility`: ~20 bytes (entity + enum + loc)
- `AbilityUsed`: ~20 bytes
- Combat scenario: 10 abilities/sec = 200 bytes/sec (negligible)

---

## Consequences

### Positive

#### 1. Simple MVP, Extensible Design

- Hardcoded abilities easy to implement (no parsing, no files)
- Enum-driven extensible (add abilities by adding enum variants)
- Future data-driven support (deserialize JSON → enums)

#### 2. Shared Targeting Logic

- Client and server use identical validation (no desync)
- Easy to test (pure functions, no ECS dependencies)
- Targeting patterns reusable (Single, Line, Radius, Adjacent)

#### 3. Instant Feedback

- Client prediction for ability usage (local player sees immediate effects)
- Rollbacks rare (targeting validated client-side before sending)
- Remote players see confirmed abilities (no visual jank)

#### 4. Triumvirate Integration Path

- MVP: Universal abilities (no filtering)
- Future: `get_available_abilities` derives from ActorImpl
- No breaking changes (additive feature)

#### 5. GCD System Reused

- Existing `gcd.rs` infrastructure (no new system needed)
- Prevents ability spam (0.5s GCD for all reactions)
- Extensible for per-ability cooldowns (future)

### Negative

#### 1. Hardcoded Ability Data

- Balance changes require recompile (edit Rust code, rebuild)
- No runtime modding (abilities baked into binary)
- More verbose than data files (each ability = full definition struct)

**Mitigation:**
- MVP: Accept recompile overhead (few abilities, fast build times)
- Future: Migrate to data-driven (JSON/RON) once ability count grows (50+)
- Hybrid: Core abilities hardcoded, modded abilities loaded

#### 2. Targeting Validation Overhead

- Server validates every ability usage (CPU cost per ability)
- Range checks, line-of-sight, entity queries (NNTree)
- 100 players using abilities = 100 validations/sec

**Mitigation:**
- Validation functions lightweight (distance checks are cheap)
- NNTree queries optimized (spatial index)
- GCD rate-limits ability usage (max 2/sec per player)

#### 3. No Projectile System in MVP

- Defers projectile physics/interpolation (complex)
- Instant-only abilities feel less varied (no travel time dynamics)
- May discover projectile issues late (Phase 2)

**Mitigation:**
- MVP validates instant ability flow (foundation for projectiles)
- Projectile system designed in this ADR (clear implementation path)
- Instant abilities sufficient for Wild Dog combat testing

#### 4. Client Prediction Rollback Complexity

- Ability prediction harder than resource prediction (multi-effect)
- Rollback may require undoing damage, queue changes, animations
- Visual jank if rollback frequent (ability "cancelled" mid-animation)

**Mitigation:**
- Client-side validation reduces rollback frequency (validate before predict)
- Show "predicted" visual state (dimmed effects, tentative animations)
- MVP: Dodge rollback simple (queue restore handled in ADR-003)

#### 5. Triumvirate Mapping Deferred

- MVP: All actors have same abilities (no build diversity)
- Cannot test Approach/Resilience gameplay until Phase 2
- Risk: Triumvirate system may not fit ability design

**Mitigation:**
- Triumvirate signature skills already defined in spec (design validated)
- `get_available_abilities` clear path to implementation
- MVP focuses on combat mechanics, Phase 2 adds diversity

### Neutral

#### 1. Targeting UI Complexity

- Hover highlights, range indicators, line drawing (client-side only)
- Not architecturally critical (UX polish)
- MVP: Simple highlight, Phase 2: fancy visuals

#### 2. Ability Effect Enum Extensibility

- Current: Damage, ClearQueue
- Future: Heal, Buff, Debuff, Knockback, Teleport, Summon
- Risk: Enum grows large (many variants)

**Consideration:** May need trait-based approach later (plugin pattern)

#### 3. GCD Shared vs Per-Ability

- MVP: Shared GCD (all reactions 0.5s)
- Future: Per-ability cooldowns (Charge: 5s, Fireball: 3s)
- Design supports both (gcd_type in definition)

## Implementation Phases

### Phase 1: Ability Definition System (Foundation)

**Goal:** Define ability data structures and registry

**Tasks:**
1. Create `common/components/ability.rs`:
   - `AbilityType` enum (BasicAttack, Dodge)
   - `AbilityDefinition` struct
   - `AbilityCost`, `TargetingPattern`, `ExecutionType`, `AbilityEffect` enums

2. Create `common/systems/abilities.rs`:
   - `get_ability_definition(AbilityType) -> AbilityDefinition`
   - Implement BasicAttack and Dodge definitions

3. Add tests:
   - Get BasicAttack definition → verify cost, targeting, effects
   - Get Dodge definition → verify cost, clear_type

**Success Criteria:**
- Ability definitions compile and serialize
- Tests pass for BasicAttack and Dodge

**Duration:** 1 day

---

### Phase 2: Targeting Validation System

**Goal:** Shared targeting logic for client and server

**Tasks:**
1. Create `common/systems/targeting.rs`:
   - `get_targeted_hexes(caster_loc, target_loc, pattern, map) -> Vec<Qrz>`
   - `is_valid_target(caster_loc, target_loc, pattern, map) -> bool`
   - `get_entities_at_hexes(hexes, nntree) -> Vec<Entity>`

2. Implement targeting patterns:
   - `Single { max_range }`: Return [target] if distance <= range
   - `SelfTarget`: Return [caster]
   - (Future: Line, Radius, Adjacent)

3. Add tests:
   - Single targeting range 1: adjacent hex valid, distant hex invalid
   - SelfTarget: always returns caster location
   - get_entities_at_hexes: finds entities at exact Loc

**Success Criteria:**
- Targeting validation accurate (range checks correct)
- Client and server use same functions (no duplication)

**Duration:** 1-2 days

---

### Phase 3: Server-Side Ability Execution

**Goal:** Server processes UseAbility events

**Tasks:**
1. Create `server/systems/abilities::execute_ability`:
   - Run in Update schedule
   - Process `Try::UseAbility` events
   - Validate: resource cost, GCD, targeting
   - Execute ability effects (damage, queue clear)
   - Emit `Do::AbilityUsed` or `Do::AbilityFailed`

2. Implement BasicAttack execution:
   - Get ability definition
   - Validate target in range 1 (adjacent)
   - Get entities at target hex (NNTree query)
   - Calculate damage (base * (1 + might/100))
   - Insert into ReactionQueue (ADR-003)
   - Emit `Do::InsertThreat` for each target

3. Implement Dodge execution:
   - Validate: queue not empty, stamina >= 30
   - Clear queue (call ADR-003 clear_threats)
   - Spend stamina
   - Emit `Do::ClearQueue` and `Do::Stamina`

4. GCD integration:
   - Check GCD before execution (use existing gcd.rs)
   - Apply GCD after successful execution

**Success Criteria:**
- Player uses BasicAttack → damage inserted into Wild Dog's queue
- Player uses Dodge → queue clears, stamina spent
- Insufficient stamina → ability fails with error message

**Duration:** 2-3 days

---

### Phase 4: Client-Side Ability Prediction

**Goal:** Client predicts ability effects for local player

**Tasks:**
1. Create `client/systems/abilities::predict_ability_usage`:
   - Run in Update schedule (before sending Try::UseAbility)
   - Validate targeting client-side (is_valid_target)
   - Predict resource spend (stamina.step -= cost)
   - Predict effects (damage prediction in ADR-005, queue clear)
   - Send `Try::UseAbility { ent, ability_type, target_loc }`

2. Implement input handling:
   - Bind attack key (Left Click or specific key)
   - Raycast mouse to hex (camera → world position)
   - Select ability (BasicAttack or Dodge)
   - Trigger prediction on key press

3. Rollback handling:
   - Receive `Do::AbilityFailed` → undo predicted changes
   - Restore stamina (snap to server's value)
   - Show error message ("Not enough stamina", "Out of range")

**Success Criteria:**
- Local player presses attack → instant feedback (stamina decreases)
- Server confirms → no visual change (prediction correct)
- Server denies → stamina snaps back, error message shown

**Duration:** 2 days

---

### Phase 5: Client-Side Ability Response (Remote Players)

**Goal:** Client renders remote players' abilities

**Tasks:**
1. Create `client/systems/abilities::handle_ability_used`:
   - Run in Update schedule
   - Process `Do::AbilityUsed` events
   - Play ability animation (attack swing, dodge roll)
   - Apply visual effects (damage numbers, queue changes)

2. Animation system:
   - BasicAttack: Play attack animation on caster
   - Dodge: Play roll/dash animation
   - (Future: Projectile spawning, ground effect visuals)

3. Remote player flow:
   - Receive `Do::AbilityUsed { ent, ability_type, target_loc }`
   - Query entity for Transform
   - Play animation at entity's position
   - Receive `Do::InsertThreat` → insert into target's visual queue

**Success Criteria:**
- Remote player attacks → animation plays, damage appears in queue
- Remote player dodges → animation plays, queue clears

**Duration:** 2 days

---

### Phase 6: Targeting UI (Hover Highlight)

**Goal:** Client shows targeting indicator

**Tasks:**
1. Create `client/systems/targeting_ui.rs`:
   - Run in Update schedule (every frame)
   - Raycast mouse to hex (camera → world)
   - Show hover highlight (white outline on hex)
   - Validate targeting (green = valid, red = invalid)

2. Range indicator:
   - BasicAttack: Show circle around player (range 1)
   - Update on ability selection (future: multi-ability hotbar)

3. Click handling:
   - Left click on valid target → trigger ability prediction
   - Right click → cancel targeting

**Success Criteria:**
- Hover over hex → hex highlights
- Valid target (adjacent) → green highlight
- Invalid target (too far) → red highlight
- Click → ability executes (if valid)

**Duration:** 2 days

---

## Validation Criteria

### Functional Tests

- **Ability Definition:** Get BasicAttack → verify cost=0, range=1, damage=20, type=Physical
- **Targeting Validation:** Caster at (0,0), target at (1,0) → valid (range 1), target at (2,0) → invalid
- **Ability Execution:** Player uses BasicAttack on Wild Dog → damage inserted into Dog's queue
- **Resource Cost:** Player uses Dodge with 30 stamina → cost applied, ability succeeds; with 20 stamina → ability fails
- **GCD Enforcement:** Player uses Dodge, immediately tries Dodge again → second fails (GCD active)

### Network Tests

- **Ability Sync:** Client sends UseAbility, server executes within 100ms (measure latency)
- **Prediction Rollback:** Client predicts ability, server denies → rollback within 1 frame
- **Remote Ability:** Remote player uses BasicAttack → local client sees animation within 200ms

### Performance Tests

- **Targeting Validation:** 1000 targeting validations/sec → < 1ms total CPU time
- **Ability Execution:** 100 players using abilities simultaneously → server processes within 16ms (60fps)

### UX Tests

- **Targeting Clarity:** Player can clearly see valid vs invalid targets (green/red)
- **Ability Responsiveness:** Ability executes within 16ms of key press (local player)
- **Feedback:** Player understands why ability failed (error message clear)

## Open Questions

### Design Questions

1. **Ability Keybinds?**
   - Left Click for BasicAttack (standard attack)?
   - Spacebar for Dodge?
   - Number keys (1-6) for future abilities?
   - MVP: Left Click attack, Spacebar dodge, configurable later

2. **Targeting Confirmation?**
   - Click-to-confirm (select target, confirm with click)?
   - Instant-cast (click = cast immediately)?
   - MVP: Instant-cast (faster, more responsive)

3. **Range Indicator Visual?**
   - Circle outline around player (shader ring)?
   - Tile highlights (all hexes in range highlighted)?
   - MVP: Simple circle outline (less visual clutter)

### Technical Questions

1. **Ability Definition Storage?**
   - Hardcoded in `get_ability_definition` function? (MVP)
   - Static HashMap? (faster lookup)
   - Lazy_static or OnceCell? (cleaner)
   - MVP: Simple match expression (fast enough for <10 abilities)

2. **Targeting Validation Frequency?**
   - Every frame (smooth hover highlight)?
   - On mouse move only (event-driven)?
   - MVP: Every frame (simple, no event system needed)

3. **Animation System Integration?**
   - Use existing Bevy animation system? (complex setup)
   - Simple sprite swap? (2-3 frames, easy)
   - MVP: Sprite swap or placeholder (animation not critical)

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

- **Complex Targeting Patterns:** Line (Volley), Radius (Eruption), Adjacent (Cleave)
- **Projectile System:** Entities with velocity, collision, lifetime, visual trail
- **Telegraph System:** Ground indicators before damage (delay, visual warning)
- **Unavoidable Attacks:** Bypass queue, instant damage, rare/expensive
- **Triumvirate Ability Grants:** Derive available abilities from ActorImpl (Approach/Resilience)
- **Ability Unlocks:** Progression system (level up, learn abilities)
- **Ability Upgrades:** Enhanced versions (Charge II, Dodge+)

### Optimization

- **Ability Data Loading:** JSON/RON files for modding (data-driven)
- **Targeting Caching:** Cache valid targets for frame (avoid repeated checks)
- **Animation Pooling:** Reuse animation entities (object pooling)

### Advanced Features

- **Combo System:** Chain abilities (BasicAttack → Charge for bonus)
- **Channeled Abilities:** Hold key to charge power (release to cast)
- **Ability Interrupts:** Damage interrupts casting (stagger system)
- **Ability Queuing:** Queue next ability during GCD (execute on GCD end)

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (targeting types, execution patterns, MVP abilities)
- **Triumvirate System:** `docs/spec/triumvirate.md` (signature skill mapping)

### Codebase

- **GCD System:** `src/common/systems/gcd.rs` (GcdType enum, cooldown tracking)
- **Triumvirate Components:** `src/common/components/entity_type/actor.rs` (ActorImpl, Approach, Resilience)
- **NNTree:** `src/common/plugins/nntree.rs` (spatial queries for entity detection)
- **Client Prediction:** `GUIDANCE/ControlledPlugin.md` (prediction patterns)

### Related ADRs

- **ADR-002:** Combat Foundation (resource costs, GCD infrastructure)
- **ADR-003:** Reaction Queue System (ClearQueue effects, Dodge ability)
- **(Future) ADR-005:** Damage Pipeline (damage calculation, InsertThreat event)

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md`
- Existing GCD and Triumvirate systems

## Date

2025-10-29
