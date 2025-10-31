# ADR-005: Damage Pipeline and Combat Resolution

## Status

**Accepted** - 2025-10-31

See `docs/adr/005-acceptance.md` for implementation review and acceptance criteria.

## Context

### Combat System Integration

This ADR integrates the previous combat ADRs into a complete damage pipeline:

- **ADR-002:** Provides Health/Stamina/Mana resources, attribute-based armor/resistance
- **ADR-003:** Provides ReactionQueue for threat queueing and timer management
- **ADR-004:** Provides directional targeting and ability system for damage generation (BasicAttack targets indicated hostile, future abilities)

**Note:** ADR-004 has been updated to use **directional targeting** (heading-based automatic target selection). The damage pipeline is agnostic to targeting method - it receives damage events from abilities regardless of whether targets were selected via mouse clicks or directional facing. All examples below reference directional targeting (e.g., "indicated target" instead of "clicked hex").

### Damage Flow Requirements

From `docs/spec/combat-system.md`:

1. **Damage Calculation:**
   ```
   Physical: damage = base * (1 + might/100) * (1 - armor)
   Magic: damage = base * (1 + focus/100) * (1 - resistance)
   Critical: crit_chance = base + (instinct/200), crit_mult = 1.5 + (instinct/200)
   ```

2. **Damage → Queue:**
   - Damage does NOT apply immediately
   - Insert into ReactionQueue as `QueuedThreat`
   - Timer starts (duration from Instinct attribute)
   - Queue overflow → oldest resolves immediately

3. **Queue Resolution:**
   - Timer expires → apply damage with passive modifiers
   - Reaction ability used → clear threats without damage
   - Queue overflow → oldest threat applies immediately

4. **Passive Modifiers (No Reaction):**
   ```
   Armor: vitality / 200 (max 75% reduction)
   Resistance: focus / 200 (max 75% reduction)
   Stagger resist: vitality / 100 (interrupt prevention)
   ```

5. **Mutual Destruction:**
   - Both entities have lethal damage queued
   - Both die (no tie-breaker)

### Architectural Challenges

#### Challenge 1: Damage Calculation Timing

**Problem:** When to calculate final damage (at insertion or resolution)?

**Options:**
- **Calculate at insertion:** Store final damage in QueuedThreat (simple, but wrong if attributes change)
- **Calculate at resolution:** Store base damage, apply modifiers when resolving (correct, more complex)
- **Hybrid:** Calculate outgoing damage at insertion, apply defensive modifiers at resolution

**Considerations:**
- Attributes may change mid-queue (buffs, debuffs, gear swap)
- Armor/resistance should reflect target's state at resolution time (not insertion)
- Attacker's damage scaling should reflect state at attack time (not resolution)

#### Challenge 2: Critical Hit Determination

**Problem:** When to roll for crit (at insertion or resolution)?

**Options:**
- **Roll at insertion:** Fair (attacker's Instinct at attack time), stored in QueuedThreat
- **Roll at resolution:** Affected by latency (attacker's state may have changed)

**Design intent from spec:**
- Crit chance tied to attacker's Instinct (attribute at attack time)
- Crit multiplier also from attacker's Instinct

#### Challenge 3: Damage Event Flow (Server → Client)

**Problem:** How to communicate damage to clients (single event or multi-step)?

**Options:**
- **Single event:** `Do::ApplyDamage { ent, amount, source, was_crit }` (simple, but hides queue)
- **Multi-event:** `Do::InsertThreat` → (timer expires) → `Do::ResolveThreat` → `Do::ApplyDamage` (verbose, clear flow)
- **Hybrid:** InsertThreat + ApplyDamage (skip ResolveThreat event)

**Client needs:**
- Know when threat inserted (show in queue UI)
- Know when threat resolves (apply damage to Health, remove from queue)
- Know damage amount (show damage number, update health bar)

#### Challenge 4: Order of Operations (Death Check)

**Problem:** When to check for death in the damage pipeline?

**Flow:**
```
Threat expires → Calculate damage → Apply to Health → Check death → Despawn?
OR
Threat expires → Calculate damage → Check if lethal → Apply → Despawn immediately?
```

**Considerations:**
- Mutual destruction requires both entities to apply damage before death check
- Death may trigger events (loot drop, respawn timer)
- Health may go negative (cosmetic, or clamp to 0?)

#### Challenge 5: Damage Prediction (Client-Side)

**Problem:** Should client predict damage to local player (or remote entities)?

**Prediction scenarios:**
- **Local player receives damage:** Predict Health decrease for instant feedback?
- **Local player deals damage:** Predict target's Health decrease (risky, may be wrong)?
- **Remote entities:** Never predict (wait for server confirmation)

**Considerations:**
- Health prediction errors feel worse than resource prediction (life-or-death)
- Target may use reaction ability (Dodge) → damage never applies
- Target's armor unknown to attacker (private attribute state)

### Existing Codebase Patterns

**Resource Updates:** `Health.state/step` pattern (from ADR-002)

**Queue Management:** `ReactionQueue` component, `insert_threat`, `check_expired_threats` (from ADR-003)

**Ability Execution:** `execute_ability` system, `AbilityEffect::Damage` (from ADR-004)

**Client Prediction:** `Offset.state/step`, `InputQueues` (local player only)

## Decision

We will implement a **server-authoritative damage pipeline with hybrid calculation timing and minimal client prediction**.

### Core Architectural Principles

#### 1. Server-Authoritative Damage

**Authority Model:**
- Server calculates all damage (outgoing and incoming)
- Server owns ReactionQueue state (insertion, expiry, resolution)
- Server applies damage to Health (final authority)
- Client predicts local player Health changes only (not remote entities)

**Rationale:**
- Prevents cheating (client cannot fake damage dealt/avoided)
- Server is source of truth for combat outcomes
- Simplifies client (no complex prediction rollback for damage)

#### 2. Hybrid Damage Calculation Timing

**Two-Phase Calculation:**

**Phase 1: Outgoing Damage (at insertion time)**
```rust
let outgoing_damage = calculate_outgoing_damage(
    base_damage,
    attacker_attributes,  // Might or Focus
    was_crit,             // Rolled at attack time
);
// Store in QueuedThreat
```

**Phase 2: Incoming Damage (at resolution time)**
```rust
let final_damage = apply_passive_modifiers(
    outgoing_damage,
    target_attributes,    // Vitality or Focus (for armor/resistance)
);
// Apply to Health
```

**Rationale:**
- Attacker's scaling reflects state at attack time (fair)
- Defender's mitigation reflects state at defense time (fair)
- Handles attribute changes mid-queue correctly
- Critical roll at attack time (attacker's Instinct)

#### 3. Damage Event Flow

**Server → Client Events:**

1. **Do::InsertThreat:** Threat added to queue (shows in UI)
2. **Do::ResolveThreat:** Threat resolved (timer expired or overflow), damage calculated
3. **Do::ApplyDamage:** Damage applied to Health (update health bar, show damage number)
4. **Do::Death:** Entity died (despawn, respawn flow)

**Flow Diagram:**
```
Ability hits → InsertThreat → Client shows in queue UI
Timer expires → ResolveThreat → Client prepares damage effect
Damage calculated → ApplyDamage → Client updates Health, shows number
Health <= 0 → Death → Client despawns entity
```

**Why separate events:**
- InsertThreat needed for queue UI (ADR-003)
- ResolveThreat optional (could merge with ApplyDamage for bandwidth)
- ApplyDamage always needed (Health update, damage number)
- Death separate (from ADR-002, triggers despawn/respawn)

**Optimization:** Skip ResolveThreat event for MVP, send InsertThreat + ApplyDamage only

#### 4. Damage Calculation Functions (Shared)

**Module:** `common/systems/damage.rs`

Functions used by both client and server:

```rust
pub fn calculate_outgoing_damage(
    base_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
) -> f32;

pub fn roll_critical(attrs: &ActorAttributes) -> (bool, f32);  // (was_crit, multiplier)

pub fn apply_passive_modifiers(
    outgoing_damage: f32,
    attrs: &ActorAttributes,
    damage_type: DamageType,
) -> f32;
```

**`calculate_outgoing_damage` formula (from spec):**
```rust
let scaling_attribute = match damage_type {
    DamageType::Physical => attrs.might() as f32,
    DamageType::Magic => attrs.focus() as f32,
};
let scaled_damage = base_damage * (1.0 + scaling_attribute / 100.0);
scaled_damage
```

**`roll_critical` formula (from spec):**
```rust
let instinct = attrs.instinct_presence() as f32;  // -100 to 100
let base_crit_chance = 0.05;  // 5%
let crit_chance = base_crit_chance + (instinct / 200.0);  // -100: 0%, 0: 5%, 100: 55%
let was_crit = rand::random::<f32>() < crit_chance;

let crit_multiplier = if was_crit {
    1.5 + (instinct / 200.0)  // -100: 1.0x, 0: 1.5x, 100: 2.0x
} else {
    1.0
};

(was_crit, crit_multiplier)
```

**`apply_passive_modifiers` formula (from spec):**
```rust
let mitigation = match damage_type {
    DamageType::Physical => {
        let vitality = attrs.vitality() as f32;
        (vitality / 200.0).min(0.75)  // Cap at 75% reduction
    },
    DamageType::Magic => {
        let focus = attrs.focus() as f32;
        (focus / 200.0).min(0.75)  // Cap at 75% reduction
    },
};
let final_damage = outgoing_damage * (1.0 - mitigation);
final_damage.max(0.0)  // Clamp to 0 (no healing from negative damage)
```

**Benefits:**
- Client can show predicted damage (if needed)
- Both use same formulas (no desync)
- Easy to test (pure functions)

---

### Detailed Design Decisions

#### Decision 1: Damage Pipeline Flow (Server Systems)

**System 1: `server/systems/abilities::execute_ability`** (from ADR-004)

Emits damage intent:
```rust
// When BasicAttack hits target
let ability_def = get_ability_definition(AbilityType::BasicAttack);
let targets = get_entities_at_hexes(&targeted_hexes, &nntree);

for target in targets {
    for effect in &ability_def.effects {
        match effect {
            AbilityEffect::Damage { amount, damage_type } => {
                // Emit damage event (processed by damage system)
                writer.write(Try { event: Event::DealDamage {
                    source: ent,
                    target,
                    base_damage: *amount,
                    damage_type: *damage_type,
                }});
            },
            // Other effects...
        }
    }
}
```

**System 2: `server/systems/damage::process_deal_damage`**

Processes Try::DealDamage:
```rust
for Try { event: Event::DealDamage { source, target, base_damage, damage_type } } in reader.read() {
    // Get attacker attributes for scaling
    let Ok(attacker_attrs) = attributes.get(source) else { continue };

    // Roll for critical hit
    let (was_crit, crit_mult) = roll_critical(&attacker_attrs);

    // Calculate outgoing damage (Phase 1)
    let outgoing = calculate_outgoing_damage(base_damage, &attacker_attrs, damage_type);
    let outgoing_with_crit = outgoing * crit_mult;

    // Get target's queue
    let Ok(mut queue) = queues.get_mut(target) else { continue };

    // Create threat
    let threat = QueuedThreat {
        source,
        damage: outgoing_with_crit,
        damage_type,
        inserted_at: time.elapsed(),
        timer_duration: calculate_timer_duration(&target_attrs),
    };

    // Insert into queue (may overflow)
    if let Some(overflow_threat) = insert_threat(&mut queue, threat, time.elapsed()) {
        // Overflow: resolve immediately
        writer.write(Try { event: Event::ResolveThreat {
            ent: target,
            threat: overflow_threat,
            reason: ResolveReason::QueueOverflow,
        }});
    }

    // Broadcast threat insertion
    writer.write(Do { event: Event::InsertThreat {
        ent: target,
        threat,
    }});
}
```

**System 3: `server/systems/damage::process_resolve_threat`**

Processes Try::ResolveThreat (from expiry or overflow):
```rust
for Try { event: Event::ResolveThreat { ent, threat, reason } } in reader.read() {
    // Get target attributes for mitigation
    let Ok(target_attrs) = attributes.get(ent) else { continue };

    // Apply passive modifiers (Phase 2)
    let final_damage = apply_passive_modifiers(
        threat.damage,
        &target_attrs,
        threat.damage_type,
    );

    // Apply to Health
    let Ok(mut health) = healths.get_mut(ent) else { continue };
    health.state -= final_damage;
    health.step = health.state;  // Sync step (no prediction for remote)

    // Broadcast damage application
    writer.write(Do { event: Event::ApplyDamage {
        ent,
        source: threat.source,
        amount: final_damage,
    }});

    // Check death (separate system handles Try::Death)
    if health.state <= 0.0 {
        writer.write(Try { event: Event::Death { ent } });
    }
}
```

**System 4: `common/systems/reaction_queue::process_expired_threats`** (from ADR-003)

Emits Try::ResolveThreat on timer expiry:
```rust
for (ent, queue, attrs) in &query {
    let expired = check_expired_threats(&queue, time.elapsed());
    for threat in expired {
        writer.write(Try { event: Event::ResolveThreat {
            ent,
            threat,
            reason: ResolveReason::TimerExpired,
        }});
    }
}
```

**Benefits:**
- Clear separation of concerns (deal → insert → resolve → apply)
- Each system testable independently
- Easy to add hooks (damage modifiers, shields, invulnerability)

---

#### Decision 2: Critical Hit System

**Critical Roll Timing:** At damage deal time (in `process_deal_damage`)

**Storage:** Store crit-modified damage in `QueuedThreat.damage`

**Rationale:**
- Crit roll reflects attacker's Instinct at attack time (fair)
- Target cannot "change fate" of crit after attack lands
- Simpler: QueuedThreat stores final outgoing damage (no separate crit flag)

**Visual Feedback:**
- Client receives `Do::InsertThreat { threat }`
- Threat does NOT indicate if crit (damage amount is higher)
- On resolution: `Do::ApplyDamage { amount }` with crit-inflated value
- Damage number shown as orange (if amount > expected) - future polish

**Alternative Considered: Store crit flag**
```rust
pub struct QueuedThreat {
    pub damage: f32,       // Base outgoing (no crit)
    pub was_crit: bool,    // Crit flag
    // ...
}
```
- Rejected: Adds complexity, client must recalculate crit damage for display
- Current: Store final damage (simpler, fewer calculations)

---

#### Decision 3: Damage Prediction (Client-Side)

**Local Player Health:**

Predict damage when threat resolves (not when inserted):
```rust
// Client system: predict_threat_resolution
for (ent, queue, health, attrs) in &local_player_query {
    let expired = check_expired_threats(&queue, time.elapsed());
    for threat in expired {
        // Predict damage application
        let final_damage = apply_passive_modifiers(
            threat.damage,
            &attrs,
            threat.damage_type,
        );
        health.step -= final_damage;  // Predict local health drop

        // Server will send Do::ApplyDamage to confirm
    }
}
```

**Why predict at resolution (not insertion):**
- Threat may be cleared by reaction ability (Dodge)
- Predicting at insertion causes premature health drop (feels wrong)
- Resolution prediction only off if server denies (rare)

**Remote Entities:**

Never predict damage:
```rust
// Remote players/NPCs
// health.step = health.state (always use server-confirmed value)
// No prediction
```

**Rationale:**
- Local player: instant feedback critical for combat feel
- Remote: accuracy > responsiveness (observing others)
- Simplifies client (no rollback for remote entity predictions)

---

#### Decision 4: Mutual Destruction Handling

**Scenario:** Both entities have lethal damage queued simultaneously.

**Server Flow:**
```
Frame N:
- Entity A's threat expires → ResolveThreat(A) → ApplyDamage(A) → Health A = -10
- Entity B's threat expires → ResolveThreat(B) → ApplyDamage(B) → Health B = -5

Frame N (death check):
- Health A <= 0 → Death(A)
- Health B <= 0 → Death(B)

Frame N+1:
- Despawn(A)
- Despawn(B)
```

**Key:** Death checks run AFTER all damage application in frame (order-independent)

**System Schedule:**
```rust
app.add_systems(FixedUpdate, (
    reaction_queue::process_expired_threats,  // Emit ResolveThreat
    damage::process_resolve_threat,           // Apply damage
    resources::check_death,                   // Emit Death (from ADR-002)
).chain());  // Sequential, all damage applies before death checks
```

**Benefits:**
- Both entities' damage applies (mutual destruction possible)
- No "who died first" race condition
- Spec-compliant ("both die")

---

#### Decision 5: Damage Number Display (Client UI)

**Visual Feedback:**

When client receives `Do::ApplyDamage`:
```rust
// Spawn floating damage number
let damage_text = format!("{:.0}", amount);  // Round to integer
let color = if amount > 50.0 { Color::RED } else { Color::ORANGE };  // High damage red

commands.spawn((
    Text2dBundle {
        text: Text::from_section(damage_text, TextStyle {
            font_size: 24.0,
            color,
            ..default()
        }),
        transform: Transform::from_translation(target_pos + Vec3::Y * 2.0),  // Above entity
        ..default()
    },
    FloatingText {
        lifetime: Duration::from_secs(1),
        velocity: Vec3::Y * 0.5,  // Float upward
    },
));
```

**Floating Text System:** `client/systems/floating_text.rs`

- Update: Move upward, fade out
- Despawn after lifetime

**MVP Scope:**
- Basic damage numbers (white text, no color coding)
- No crit indication (future: orange/yellow for crits)
- No damage type icons (future: sword icon for physical, star for magic)

---

#### Decision 6: Network Message Structure

**New Event Types (add to `common/message.rs`):**

```rust
pub enum Event {
    // Existing events...

    /// Server-internal: Deal damage (Try event)
    DealDamage {
        source: Entity,
        target: Entity,
        base_damage: f32,
        damage_type: DamageType,
    },

    /// Server-internal: Resolve threat (Try event)
    ResolveThreat {
        ent: Entity,
        threat: QueuedThreat,
        reason: ResolveReason,
    },

    /// Server → Client: Damage applied to entity (Do event)
    ApplyDamage {
        ent: Entity,
        source: Entity,
        amount: f32,
    },
}
```

**Event Classification:**
- **Try events (server-internal):** `DealDamage`, `ResolveThreat`
- **Do events (broadcast):** `InsertThreat` (from ADR-003), `ApplyDamage`
- **Death/Despawn:** Already defined in ADR-002

**Message Frequency:**
- Wild Dog attacks every 2 seconds → 1 InsertThreat + 1 ApplyDamage per 2 seconds
- 10 players fighting → 10 threats/2sec = 5 events/sec
- Bandwidth: 5 * 40 bytes = 200 bytes/sec (negligible)

---

#### Decision 7: Damage Type Extensibility

**Current Types (MVP):**
```rust
pub enum DamageType {
    Physical,
    Magic,
}
```

**Future Types:**
```rust
pub enum DamageType {
    Physical,
    Magic,
    Fire,      // Resisted by fire resistance (separate attribute)
    Ice,       // Slows on hit
    Poison,    // Damage over time
    True,      // Ignores armor/resistance
}
```

**Mitigation Calculation (Future):**
```rust
let mitigation = match damage_type {
    DamageType::Physical => armor,
    DamageType::Magic => resistance,
    DamageType::Fire => fire_resistance,
    DamageType::True => 0.0,  // No mitigation
};
```

**MVP:** Physical only (Wild Dog), Magic deferred to Phase 2

---

## Consequences

### Positive

#### 1. Complete Damage Pipeline

- Ability → DealDamage → InsertThreat → ResolveThreat → ApplyDamage → Death
- Each step clear, testable, extensible
- Integrates ADR-002, ADR-003, ADR-004

#### 2. Fair Damage Calculation

- Attacker's scaling at attack time (Might/Focus)
- Defender's mitigation at defense time (armor/resistance)
- Crit roll at attack time (Instinct)
- Handles mid-queue attribute changes correctly

#### 3. Mutual Destruction Supported

- Death checks after all damage (order-independent)
- Spec-compliant ("both die")
- Emergent dramatic moments

#### 4. Minimal Client Prediction

- Local player: predict Health drop on threat resolution
- Remote entities: no prediction (simpler, fewer rollbacks)
- Prediction errors rare (only if reaction ability used mid-prediction)

#### 5. Extensible Damage Types

- Add new types easily (Fire, Ice, Poison, True)
- Mitigation calculation per-type
- No breaking changes to pipeline

### Negative

#### 1. Two-Phase Damage Calculation Complexity

- Outgoing damage at insertion (attacker attributes)
- Incoming damage at resolution (defender attributes)
- QueuedThreat stores outgoing damage (must document clearly)

**Mitigation:**
- Clear function names (`calculate_outgoing_damage`, `apply_passive_modifiers`)
- Tests for both phases
- Comments in QueuedThreat struct

#### 2. Damage Number Display Performance

- Spawning entities for floating text (100 damage numbers = 100 entities)
- Text rendering overhead (each frame, multiple texts)
- Despawn logic (lifecycle management)

**Mitigation:**
- Object pooling (reuse text entities)
- Limit max simultaneous damage numbers (10 max, despawn oldest)
- MVP: Accept overhead (optimize if profiling shows issue)

#### 3. Crit Roll RNG Determinism

- Server rolls crit (RNG seed)
- Client cannot verify crit (no RNG sync)
- Trust server (can't detect server-side cheating)

**Mitigation:**
- MVP: Accept trust model (not PvP yet)
- Future: Seed RNG deterministically (both client/server can verify)

#### 4. Damage Prediction Errors

- Local player predicts Health drop, but threat cleared by Dodge
- Health snaps back up (jarring)
- Rare but noticeable

**Mitigation:**
- Show "predicted" visual state (dimmed health bar)
- Audio/visual feedback on rollback (heal effect when restored)
- Test with various latencies

#### 5. Multiple Network Events per Damage

- InsertThreat (queue UI) + ApplyDamage (health update)
- 2 events per attack (vs 1 combined event)
- More bandwidth, more processing

**Mitigation:**
- Events small (~40 bytes each)
- Bandwidth negligible (200 bytes/sec for 10 players)
- Could optimize later (combine events if needed)

### Neutral

#### 1. Damage Type Enum Simplicity

- MVP: Physical only
- Magic deferred (no magic abilities yet)
- Risk: Discover type system issues when adding Magic

**Acceptance:** Type system designed, implementation straightforward

#### 2. Damage Scaling Formula Balance

- Might/Focus scaling (1 + attr/100)
- At 100 Might: 2x damage (100% increase)
- May need tuning (playtest balance)

**Consideration:** Formulas from spec (validated by PLAYER role)

## Implementation Phases

### Phase 1: Damage Calculation Functions (Foundation)

**Goal:** Shared damage calculation logic

**Tasks:**
1. Create `common/systems/damage.rs`:
   - `calculate_outgoing_damage(base, attrs, type) -> f32`
   - `roll_critical(attrs) -> (bool, f32)`
   - `apply_passive_modifiers(damage, attrs, type) -> f32`

2. Add tests:
   - Outgoing: base=20, Might=50 → 20 * 1.5 = 30
   - Crit: Instinct=100 → ~55% crit chance, 2.0x multiplier
   - Mitigation: damage=50, Vitality=100 → 50 * (1 - 0.5) = 25

**Success Criteria:**
- All formula tests pass
- Crit roll probabilities correct (sample 1000 rolls, verify ~55% for Instinct=100)

**Duration:** 1 day

---

### Phase 2: Server Damage Pipeline (Deal → Insert → Resolve → Apply)

**Goal:** Server processes damage from abilities to Health changes

**Tasks:**
1. Create `server/systems/damage::process_deal_damage`:
   - Process `Try::DealDamage` events
   - Roll crit, calculate outgoing damage
   - Insert into ReactionQueue
   - Emit `Do::InsertThreat`

2. Create `server/systems/damage::process_resolve_threat`:
   - Process `Try::ResolveThreat` events
   - Apply passive modifiers
   - Apply to Health component
   - Emit `Do::ApplyDamage`
   - Check death (emit `Try::Death` if HP <= 0)

3. Update `server/systems/abilities::execute_ability` (from ADR-004):
   - Emit `Try::DealDamage` for damage effects (instead of direct queue insertion)

4. System schedule:
   - FixedUpdate: `reaction_queue::process_expired_threats` → `damage::process_resolve_threat` → `resources::check_death` (chained)
   - Update: `abilities::execute_ability` → `damage::process_deal_damage`

**Success Criteria:**
- Wild Dog BasicAttack → InsertThreat in player queue
- Timer expires → ApplyDamage to player Health
- Player Health decreases by (base * (1 + dog_might/100) * (1 - player_armor))

**Duration:** 2-3 days

---

### Phase 3: Client Damage Response (Health Update, Damage Numbers)

**Goal:** Client responds to damage events

**Tasks:**
1. Update `client/systems/renet.rs::do_manage_connections`:
   - Process `Do::ApplyDamage` events
   - Update Health component (remote: set `state`, local: accept server correction)
   - Spawn floating damage number

2. Create `client/systems/floating_text.rs`:
   - `FloatingText` component (lifetime, velocity)
   - System: update position (move upward), update alpha (fade out)
   - Despawn after lifetime

3. Damage number visuals:
   - White text, 24pt font
   - Spawns above entity (Y + 2.0)
   - Floats upward (velocity Y * 0.5)
   - Fades out over 1 second

**Success Criteria:**
- Player takes damage → Health bar decreases
- Damage number appears above player, floats upward, fades out
- Remote entities: damage numbers appear on damage events

**Duration:** 2 days

---

### Phase 4: Client Damage Prediction (Local Player)

**Goal:** Client predicts local player Health changes

**Tasks:**
1. Create `client/systems/damage::predict_threat_resolution`:
   - Run in Update schedule
   - Query local player's ReactionQueue
   - Call `check_expired_threats` (client-side, same logic as server)
   - Predict damage: `health.step -= apply_passive_modifiers(...)`

2. Rollback handling:
   - Receive `Do::ApplyDamage` → compare to predicted amount
   - If mismatch (rare): snap `step` to server's value
   - If threat cleared by Dodge: no ApplyDamage received, restore health (handled by ADR-003)

**Success Criteria:**
- Local player takes damage → Health bar drops instantly (predicted)
- Server confirms → no visual change (prediction correct)
- Player uses Dodge before expiry → Health not decreased (threat cleared)

**Duration:** 1-2 days

---

### Phase 5: Death and Mutual Destruction

**Goal:** Death flow completes damage pipeline

**Tasks:**
1. Ensure death check in `resources::check_death` (from ADR-002):
   - Run after damage application (FixedUpdate, chained)
   - Query entities with `Health.state <= 0.0`
   - Emit `Try::Death { ent }`

2. Mutual destruction test:
   - Spawn 2 Wild Dogs
   - Both attack player simultaneously
   - Both threats expire same frame
   - Both apply lethal damage
   - Both die (Death events for both)

3. Client death handling:
   - Receive `Do::Despawn` (from death handler)
   - Remove entity from world
   - If local player: show respawn UI (from ADR-002)

**Success Criteria:**
- Player Health reaches 0 → Death event emitted
- Player despawns (or respawns after 5s)
- Mutual destruction: both entities die simultaneously

**Duration:** 1 day

---

### Phase 6: Damage UI Polish (Health Bars)

**Goal:** Visual clarity for Health changes

**Tasks:**
1. Health bar rendering (above entities):
   - Green bar, depletes left-to-right
   - Background bar (grey, shows max HP)
   - Width proportional to current/max HP

2. Health bar updates:
   - Interpolate smoothly (no snapping)
   - Use `health.step` for local, `health.state` for remote
   - Only show in combat (CombatState component check)

3. Health bar positioning:
   - World-space UI (follows entity)
   - Above entity (Y + 1.5)
   - Scale with camera zoom

**Success Criteria:**
- Health bars visible on all entities in combat
- Bars update smoothly on damage
- Bars hidden when out of combat and full HP

**Duration:** 2 days

---

## Validation Criteria

### Functional Tests

- **Damage Calculation:** Base=20, Might=50 → outgoing=30, Vitality=100 → final=15 (50% mitigation)
- **Critical Hits:** Instinct=100 → ~55% crit rate, 2.0x damage on crit
- **Damage Application:** Wild Dog attacks → threat inserted → timer expires → damage applies to Health
- **Death Flow:** Health reaches 0 → Death event → Despawn
- **Mutual Destruction:** Both entities have lethal damage → both die same frame

### Network Tests

- **Damage Sync:** Server applies damage, client receives within 100ms
- **Damage Prediction:** Local player predicts damage, server confirms → no visual snap (prediction accurate)
- **Rollback:** Player uses Dodge, server clears queue → predicted damage not applied (rollback)

### Performance Tests

- **Damage Numbers:** 100 simultaneous damage numbers → 60fps maintained
- **Pipeline Throughput:** 100 damage events/sec → server processes < 5ms total

### UX Tests

- **Damage Clarity:** Player understands how much damage taken (number visible, health bar accurate)
- **Death Feedback:** Player knows they died (screen effect, respawn UI)
- **Crit Feel:** Crits feel impactful (higher damage, visual indicator future)

## Open Questions

### Design Questions

1. **Damage Number Color Coding?**
   - White for normal, orange for crit? (visual distinction)
   - Red for lethal damage (killed entity)?
   - MVP: White only, color coding Phase 2

2. **Health Bar Visibility?**
   - Always visible (even out of combat)?
   - Only in combat (CombatState check)?
   - Only when damaged (hide if full HP)?
   - MVP: Always visible in combat, hidden out of combat

3. **Damage Rounding?**
   - Display as integer (50) or decimal (50.5)?
   - Store as f32, display as integer?
   - MVP: Display as integer (cleaner)

### Technical Questions

1. **Floating Text Object Pooling?**
   - Spawn/despawn every damage (simple, potential overhead)?
   - Object pool (reuse entities, more complex)?
   - MVP: Spawn/despawn, optimize if needed

2. **Crit RNG Seed Sync?**
   - Server RNG unseeded (non-deterministic)?
   - Seed from game time (deterministic, verifiable)?
   - MVP: Unseeded (trust server), seed in Phase 2 for PvP

3. **Damage Pipeline Optimization?**
   - Combine InsertThreat + ApplyDamage (single event)?
   - Skip intermediate events (bandwidth)?
   - MVP: Separate events (clarity), combine if bandwidth issue

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

- **Damage Types:** Fire, Ice, Poison, True (ignore armor)
- **Damage Over Time (DoT):** Poison ticks, burn damage
- **Shields:** Absorb damage before Health
- **Damage Modifiers:** Buffs (increase damage), debuffs (reduce damage)
- **Reflect Damage:** Counter ability reflects damage to attacker
- **Lifesteal:** Heal attacker for % of damage dealt
- **Area Damage:** Damage numbers show cumulative (AoE hits 5 enemies)

### Optimization

- **Damage Event Batching:** Combine multiple damage events (AoE)
- **Floating Text Pooling:** Reuse text entities (object pool)
- **Health Bar Culling:** Only render visible entities (camera frustum check)

### Advanced Features

- **Damage Log:** Text feed of damage dealt/taken (combat log)
- **Damage Meters:** DPS tracking, damage breakdown by source
- **Kill Feed:** UI showing recent kills (X killed Y)

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (damage formulas, passive modifiers, mutual destruction)
- **Attribute System:** `docs/spec/attribute-system.md` (Might, Vitality, Focus, Instinct)

### Codebase

- **Health Component:** ADR-002 (`Health.state/step`, armor/resistance formulas)
- **ReactionQueue:** ADR-003 (`QueuedThreat`, `insert_threat`, `check_expired_threats`)
- **Ability System:** ADR-004 (`AbilityEffect::Damage`, `execute_ability`)

### Related ADRs

- **ADR-002:** Combat Foundation (Health, passive modifiers, death handling)
- **ADR-003:** Reaction Queue System (threat insertion, timer expiry, resolution)
- **ADR-004:** Ability System and Directional Targeting (heading-based target selection, damage generation, ability effects)

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md`
- Integration with ADR-002, ADR-003, ADR-004

## Date

2025-10-30 (Updated to reference ADR-004 directional targeting integration)
