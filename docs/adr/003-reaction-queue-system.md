# ADR-003: Reaction Queue System Architecture

## Status

Proposed

## Context

### Combat System Design Goal

From `docs/spec/combat-system.md`:

> **"Conscious but Decisive"** - Real-time tactical combat where skill comes from positioning, reading threats, and resource management. No twitch mechanics required.

The **Reaction Queue** is the core defensive mechanic that enables this philosophy:
- Incoming damage enters a queue before applying (time to respond)
- Each queued threat has an independent timer (visual countdown)
- Players use reaction abilities to clear/modify threats
- Attributes determine queue capacity and timer duration

### Why This System Exists

**Problem with instant damage:**
- No counterplay opportunity (damage applies immediately)
- Twitch reflexes required (react in milliseconds)
- No strategic depth (spam defensive abilities)

**Reaction Queue solves this:**
- Conscious decision-making (see threats coming, choose response)
- Tactical resource management (stamina costs, limited queue capacity)
- Attribute-driven playstyle (Focus = more queue slots, Instinct = longer timers)
- Skill expression through positioning and threat reading

### Specification Requirements

From combat spec, the queue must:

1. **Queue Capacity** = `base_capacity + floor(focus / 33.0)`
   - Focus = -100: 1 slot (everything instant)
   - Focus = 0: 3 slots
   - Focus = 100: 6 slots

2. **Timer Duration** = `base_window * (1.0 + instinct / 200.0)`
   - Instinct = -100: 0.5s window
   - Instinct = 0: 1.0s window
   - Instinct = 100: 1.5s window

3. **Overflow Behavior**: When queue full, oldest threat resolves immediately with passive modifiers

4. **Reaction Abilities**: Clear threats (Dodge clears all, Counter clears first, etc.)

5. **Visual Display**: Circular icons with depleting timer rings, left-to-right order

### Architectural Challenges

#### Challenge 1: Timer Synchronization

**Problem:** Client and server must agree on timer state.

- Client needs smooth timer animations (every frame)
- Server needs to know when timers expire (discrete ticks)
- Network latency causes desync (client sees threat before server confirms)

**Options:**
- **Server-authoritative timers**: Server sends timer updates, client interpolates (high bandwidth)
- **Client-predicted timers**: Both run timers locally, server confirms expiry (low bandwidth, desync risk)
- **Hybrid**: Client predicts, server sends corrections on mismatch

#### Challenge 2: Queue Insertion vs Resolution Order

**Problem:** Threats can arrive while others are resolving.

- Threat A expires → begins resolution (damage calculation)
- Threat B arrives mid-resolution → insert into queue?
- If Threat A kills entity, does Threat B still insert?

**Order matters for:**
- Death prevention (using reaction ability before lethal threat resolves)
- Queue overflow (new threat forces oldest to resolve)
- Mutual destruction (both entities have lethal damage queued)

#### Challenge 3: Client Prediction of Reaction Abilities

**Problem:** Using Dodge should clear queue instantly (UX), but server must validate.

- Client predicts: queue clears, stamina spent
- Server confirms: queue cleared (or ability failed due to insufficient stamina)
- Rollback scenario: server denies, restore queue state

**Prediction complexity:**
- Queue state is complex (multiple threats with timers)
- Rolling back a queue clear is expensive (restore 6 threats?)
- Validation must account for latency (threat arrived server-side after client cleared)

#### Challenge 4: UI Rendering Performance

**Problem:** Queue UI updates every frame for smooth timer animations.

- 100 players in combat = 100 queue UI instances
- Each queue has up to 6 threats with timer rings
- Timer ring graphics require shader updates (circle fill percentage)

**Performance concerns:**
- Overdraw (circular UI elements overlapping)
- Update frequency (every frame vs fixed intervals)
- Entity culling (only render visible player queues)

### Existing Codebase Patterns

**Client Prediction:**
- Input queue uses `state/step` pattern (`InputQueues` in `common/resources/mod.rs`)
- Client predicts, server confirms with sequence numbers
- Rollback via sequence number matching and removal

**Timer Systems:**
- `Time::elapsed()` for durations (not `Instant`)
- FixedUpdate for discrete ticks (125ms)
- Update for per-frame rendering

**UI Systems:**
- `UiPlugin` in `GUIDANCE/UiPlugin.md`
- Character panel, HUD, target cursor already implemented
- UI responds to component state changes

## Decision

We will implement a **server-authoritative reaction queue with client-predicted timers and optimistic ability usage**.

### Core Architectural Principles

#### 1. Server-Authoritative Queue State

**Authority Model:**
- Server owns queue state (threats, insertion order, capacity)
- Server determines when timers expire and threats resolve
- Client predicts queue changes for local player (instant feedback)
- Remote players: client displays server-confirmed state only

**Rationale:**
- Prevents cheating (client cannot fake cleared threats)
- Server is source of truth for damage application
- Allows rollback if client prediction wrong

#### 2. Client-Predicted Timers

**Timer Synchronization:**
- Server sends threat with insertion time (`threat_inserted_at: Duration`)
- Client calculates remaining time locally: `timer_duration - (Time::elapsed() - threat_inserted_at)`
- Server does NOT send timer updates every frame (bandwidth optimization)
- Server sends expiry event only when threat resolves

**Benefits:**
- Smooth timer animations (client renders every frame)
- Low bandwidth (insert event only, no per-frame updates)
- Deterministic (both use same formula, same attributes)

**Desync Mitigation:**
- Client and server use same `Time::elapsed()` baseline (synced via `Event::Init`)
- If latency causes mismatch, client accepts server's resolution (rollback if needed)

#### 3. Optimistic Reaction Ability Usage

**Prediction Flow:**
1. Client uses Dodge → predict queue clear, spend stamina
2. Client sends `Try::UseAbility { ent, ability: Dodge }`
3. Server validates: sufficient stamina, queue not empty, no GCD
4. Server confirms: `Do::ClearQueue { ent }` + `Do::Stamina { current }`
5. Client receives confirmation: already cleared (prediction correct) OR rollback (restore queue)

**Rollback Scenario:**
```
Client state: Queue = [], Stamina = 70 (predicted)
Server state: Queue = [Threat A, Threat B], Stamina = 98 (actual)
Server sends: Do::Stamina { current: 98 }, Do::AbilityFailed { reason: "Not enough stamina" }
Client rollback: Restore queue from server state, snap stamina to 98
```

**Rationale:**
- Instant feedback critical for combat feel
- Rollbacks rare (client prediction usually correct)
- UX better with occasional rollback than high latency confirms

#### 4. Queue Component Structure

**Component Design:**

```rust
#[derive(Component)]
pub struct ReactionQueue {
    pub threats: VecDeque<QueuedThreat>,
    pub capacity: usize,  // Derived from Focus attribute
}

pub struct QueuedThreat {
    pub source: Entity,           // Attacker
    pub damage: f32,              // Base damage (before modifiers)
    pub damage_type: DamageType,  // Physical or Magic
    pub inserted_at: Duration,    // Time::elapsed() when inserted
    pub timer_duration: Duration, // Instinct-scaled duration
}

pub enum DamageType {
    Physical,
    Magic,
}
```

**Why VecDeque:**
- FIFO ordering (oldest threat at front)
- Efficient push_back (new threats) and pop_front (expired threats)
- Iteration for UI rendering (left-to-right display)

**Why store `inserted_at` and `timer_duration`:**
- Client calculates remaining time: `timer_duration - (now - inserted_at)`
- Server checks expiry: `now >= inserted_at + timer_duration`
- No need to sync timers every frame

#### 5. Shared Queue Logic in `common/`

**Queue Operations Module:** `common/systems/reaction_queue.rs`

Functions used by both client and server:
```rust
pub fn calculate_queue_capacity(attrs: &ActorAttributes) -> usize;
pub fn calculate_timer_duration(attrs: &ActorAttributes) -> Duration;
pub fn insert_threat(queue: &mut ReactionQueue, threat: QueuedThreat, now: Duration) -> Option<QueuedThreat>;
pub fn check_expired_threats(queue: &ReactionQueue, now: Duration) -> Vec<QueuedThreat>;
pub fn clear_threats(queue: &mut ReactionQueue, clear_type: ClearType) -> Vec<QueuedThreat>;
```

**`insert_threat` logic:**
- If queue has capacity, push to back
- If queue full, pop oldest (front), resolve immediately, then push new threat
- Returns `Some(overflow_threat)` if overflow occurred, `None` otherwise

**`check_expired_threats` logic:**
- Iterate front-to-back
- Collect threats where `now >= inserted_at + timer_duration`
- Does NOT remove (caller decides when to remove after damage applies)

**`clear_threats` logic:**
- `ClearType::All` (Dodge) → drain entire queue
- `ClearType::First(n)` (Counter, Parry) → drain first N
- `ClearType::ByType(damage_type)` (Ward for magic only)
- Returns cleared threats for logging/effects

### Detailed Design Decisions

#### Decision 1: Queue Capacity Scaling

**Formula (from spec):**
```rust
pub fn calculate_queue_capacity(attrs: &ActorAttributes) -> usize {
    let focus = attrs.focus() as i16;  // -100 to 100
    let base_capacity = 3;
    let bonus = (focus / 33).max(0);  // Integer division, clamp negative to 0
    (base_capacity + bonus as usize).min(10)  // Cap at 10 for sanity
}
```

**Examples:**
- Focus = -100: 3 + (-3) = 0 → clamped to 3 (or set base to 1 + bonus for spec compliance)
- Focus = 0: 3 + 0 = 3
- Focus = 100: 3 + 3 = 6

**Spec Discrepancy:**
- Spec says Focus=-100 gives 1 slot, but formula gives 3 + floor(-100/33) = 3 + (-3) = 0
- Resolution: Change base to 1, so: `1 + (focus / 33).max(0)` OR change spec to match

**Decision:** Clarify with PLAYER role during implementation. ADR documents both interpretations.

---

#### Decision 2: Timer Duration Scaling

**Formula (from spec):**
```rust
pub fn calculate_timer_duration(attrs: &ActorAttributes) -> Duration {
    let instinct = attrs.instinct() as f32;  // 0 to 100
    let base_window = 1.0;  // 1 second
    let multiplier = 1.0 + (instinct / 200.0);
    Duration::from_secs_f32(base_window * multiplier)
}
```

**Examples:**
- Instinct = 0: 1.0 * (1.0 + 0/200) = 1.0s
- Instinct = 50: 1.0 * (1.0 + 50/200) = 1.25s
- Instinct = 100: 1.0 * (1.0 + 100/200) = 1.5s

**Note:** Spec mentions Instinct=-100 gives 0.5s, but `ActorAttributes::instinct()` returns u8 (0-100 range). Negative instinct not possible with current attribute system.

**Resolution:** Spec assumes raw axis values (-100 to 100). Use `instinct_presence()` helper (returns i8) instead of `instinct()`.

**Corrected Formula:**
```rust
pub fn calculate_timer_duration(attrs: &ActorAttributes) -> Duration {
    let instinct = attrs.instinct_presence() as f32;  // -100 to 100
    let base_window = 1.0;
    let multiplier = 1.0 + (instinct / 200.0);  // -100: 0.5x, 0: 1.0x, 100: 1.5x
    Duration::from_secs_f32(base_window * multiplier).max(Duration::from_millis(250))  // Min 250ms
}
```

---

#### Decision 3: Threat Insertion Flow

**Server Flow (authoritative):**
1. Damage event occurs (ability hits, enemy attack lands)
2. Query target's `(ReactionQueue, ActorAttributes)`
3. Create `QueuedThreat` with `inserted_at = Time::elapsed()`
4. Calculate `timer_duration` from Instinct attribute
5. Call `insert_threat()`:
   - If capacity available: push to back
   - If full: pop oldest, emit `Try::ResolveThreat` (overflow), then push new
6. Emit `Do::InsertThreat { ent, threat }` to clients

**Client Flow (prediction for local player):**
1. Receive `Do::InsertThreat { ent, threat }`
2. If local player: predict insertion (already done on attack impact)
3. If remote player: insert into visual queue immediately
4. UI updates to show new threat icon with timer

**Overflow Handling:**
- Server emits `Try::ResolveThreat { ent, threat, reason: Overflow }`
- Resolution system applies damage immediately with passive modifiers
- Client receives `Do::ApplyDamage { ent, amount }` (from resolution)
- Client does NOT receive separate overflow notification (damage event is enough)

---

#### Decision 4: Timer Expiry and Resolution

**Server System:** `server/systems/reaction_queue::process_expired_threats`

**Schedule:** FixedUpdate (125ms ticks)

**Logic:**
```
For each entity with ReactionQueue:
    expired = check_expired_threats(queue, Time::elapsed())
    For each expired_threat in expired:
        emit Try::ResolveThreat { ent, threat, reason: TimerExpired }
        remove threat from queue
```

**Resolution System:** `server/systems/combat::resolve_threat` (processes Try::ResolveThreat)

**Logic:**
```
For each Try::ResolveThreat:
    Calculate damage with passive modifiers (armor/resistance)
    Apply damage to Health
    Emit Do::ApplyDamage { ent, amount, source }
    Check if death occurred (Health <= 0)
```

**Client Response:**
- Receive `Do::ApplyDamage`
- Update Health component (state/step)
- Play damage effect (animation, sound)
- Remove threat from visual queue (matched by source + damage + insertion time)

**Timer Check Frequency:**
- Server: FixedUpdate (125ms) is acceptable (timer durations 0.5s - 1.5s)
- Small variance (±125ms) acceptable for MVP
- Future: More frequent checks if needed (every 50ms)

---

#### Decision 5: Reaction Ability Integration

**Ability Usage Flow:**

1. **Client initiates:**
   - Player presses Dodge key
   - Client predicts: `clear_threats(queue, ClearType::All)`, `stamina.step -= 30`
   - Client sends `Try::UseAbility { ent, ability: Dodge }`

2. **Server validates:**
   - Check stamina >= cost
   - Check queue not empty (can't Dodge if no threats)
   - Check GCD not active
   - If valid: spend stamina, clear queue, emit confirmations
   - If invalid: emit `Do::AbilityFailed { ent, reason }`

3. **Server broadcasts:**
   - `Do::ClearQueue { ent, clear_type: All }`
   - `Do::Stamina { ent, current, max }`
   - `Do::Gcd { ent, typ: Reaction, duration: 500ms }`

4. **Client receives confirmation:**
   - Local player: queue already cleared (prediction correct)
   - Remote players: clear queue on `Do::ClearQueue`
   - If rollback: `Do::AbilityFailed` → restore queue from server state

**Rollback Mechanism:**
- Server must send current queue state if ability fails
- `Do::AbilityFailed { ent, reason, queue_snapshot: Vec<QueuedThreat> }`
- Client replaces local queue with snapshot

**Alternative (simpler):**
- Don't send queue snapshot, client just doesn't clear on failure
- Risk: Client may have predicted other changes (new threats arrived)
- Resolution: Accept minor desync, server corrections will fix within 1-2 ticks

---

#### Decision 6: Network Message Structure

**New Event Types (add to `common/message.rs`):**

```rust
pub enum Event {
    // Existing events...

    /// Server → Client: Insert threat into reaction queue
    InsertThreat {
        ent: Entity,
        threat: QueuedThreat,
    },

    /// Server → Client: Threat resolved (damage applied)
    ResolveThreat {
        ent: Entity,
        source: Entity,
        damage: f32,
        reason: ResolveReason,
    },

    /// Server → Client: Clear threats from queue
    ClearQueue {
        ent: Entity,
        clear_type: ClearType,
    },

    /// Client → Server: Use reaction ability (Try event)
    UseAbility {
        ent: Entity,
        ability: AbilityType,
    },

    /// Server → Client: Ability usage failed
    AbilityFailed {
        ent: Entity,
        ability: AbilityType,
        reason: String,
    },
}

pub enum ResolveReason {
    TimerExpired,
    QueueOverflow,
}

pub enum ClearType {
    All,             // Dodge
    First(usize),    // Counter (1), Parry (1)
    ByType(DamageType),  // Ward (magic only)
}
```

**Serialization Considerations:**
- `QueuedThreat` must be `Serialize + Deserialize`
- `Duration` serializes as `u64` (nanoseconds)
- Message size: ~40 bytes per threat (acceptable)

**Event Classification:**
- **Try events**: `UseAbility`, `ResolveThreat` (server-internal)
- **Do events**: `InsertThreat`, `ClearQueue`, `AbilityFailed`

---

#### Decision 7: UI Rendering Strategy

**UI Component:** `client/systems/reaction_queue_ui.rs`

**Rendering Approach:**
- React-style: Query `(ReactionQueue, ActorAttributes)`, spawn UI entities for each threat
- UI entities are children of player entity (despawn with player)
- Each threat = one UI entity with `Sprite`, `Transform`, shader for circular timer

**Timer Ring Shader:**
- Fragment shader takes `fill_percentage: f32` uniform
- Renders circle outline, fills clockwise from top
- Calculate per-frame: `fill = (remaining / duration).clamp(0.0, 1.0)`

**Update Frequency:**
- Run in `Update` schedule (every frame)
- Only update local player's queue (remote players' queues less critical)
- Batch UI updates (single system updates all threat UI entities)

**Layout:**
- Horizontal row above player character (world-space UI)
- Each icon 32x32 pixels, 4px spacing
- Left-to-right = oldest to newest (matches queue order)

**Performance Optimization:**
- Only render threats in camera view (cull off-screen players)
- Reuse UI entities (object pooling for threat icons)
- Dirty flag: only update if queue changed (new threat, cleared, expired)

---

#### Decision 8: MVP Scope and Simplifications

**MVP Includes:**
- Queue component with capacity and timer duration
- Single threat type: Physical damage from Wild Dog
- Dodge ability (clears entire queue)
- Timer expiry and resolution
- Basic UI (icons with timer rings)
- Server-authoritative with client prediction

**MVP Excludes:**
- Multiple damage types (Physical vs Magic) - add in Phase 2
- Selective clear abilities (Counter, Parry) - Dodge only for MVP
- Queue UI refinements (threat type icons, damage numbers)
- Advanced prediction rollback (accept minor desync)
- Telegraph integration (enemy attacks don't show ground indicators yet)

**Simplification Rationale:**
- Validate core mechanic (queue + timers + Dodge) before complexity
- Wild Dog provides sufficient test case (melee attacks every 2 seconds)
- Single ability reduces GCD/ability system dependencies
- Basic UI proves concept, polish later

---

## Consequences

### Positive

#### 1. Enables "Conscious but Decisive" Combat

- Time to read threats (timers provide reaction window)
- Strategic decisions (which threats to react to, resource spending)
- No twitch mechanics (timers scale with Instinct attribute)

#### 2. Attribute-Driven Playstyle

- Focus builds: More queue slots = handle more threats simultaneously
- Instinct builds: Longer timers = more time to decide
- Vitality/armor builds: Fewer reactions needed (passively mitigate)

#### 3. Low Network Bandwidth

- Threat insert: ~40 bytes per threat
- No per-frame timer updates (client calculates locally)
- Wild Dog attacking every 2 seconds = ~20 bytes/sec per player

#### 4. Smooth Client UX

- Optimistic ability usage (instant queue clear)
- Client-predicted timers (smooth countdown animations)
- Rare rollbacks (only when server denies ability)

#### 5. Extensible Design

- Add new damage types (elemental, true, etc.)
- Add selective clear abilities (Counter, Deflect)
- Add queue modifiers (status effects that slow/pause timers)
- Add threat priority (urgent threats highlighted)

### Negative

#### 1. Complex State Synchronization

- Queue state is multi-element (up to 6 threats with timers)
- Prediction rollback harder than single-value rollback (Health, Stamina)
- Desync scenarios possible (client sees expired threat, server hasn't processed)

**Mitigation:**
- Server corrections frequent (FixedUpdate every 125ms)
- Client accepts server state as truth (snaps queue to server's state)
- MVP: Accept minor visual glitches, fix in Phase 2

#### 2. Timer Precision Tradeoffs

- FixedUpdate 125ms granularity vs timer durations 500ms-1500ms
- Threat may expire up to 125ms late (noticeable at low Instinct)
- Visual timer reaches 0, but damage not applied yet (brief delay)

**Mitigation:**
- MVP: 125ms variance acceptable (human reaction time ~200ms)
- Future: Run expiry checks more frequently (every 50ms)
- Future: Visual timer depletes 100ms slower (reach 0 when server resolves)

#### 3. Rollback UX Friction

- Client predicts Dodge clears queue, server denies → threats reappear
- Feels like input didn't register (but actually failed validation)
- Confusion if stamina cost already shown (client predicted spend)

**Mitigation:**
- Show "predicted" visual state (dimmed stamina bar, greyed queue)
- Audio/visual feedback on rollback ("Not enough stamina" message)
- Test with various latencies (ensure rollback feels acceptable)

#### 4. UI Rendering Complexity

- Circular timer shader non-trivial (requires custom shader)
- Per-frame updates for smooth animations (performance concern)
- World-space UI positioning (must stay above player during movement)

**Mitigation:**
- Reuse existing UI infrastructure (`UiPlugin` patterns)
- Optimize shaders (pre-bake timer ring textures, use sprite sheet)
- Cull off-screen queues (only render visible players)

#### 5. Threat Matching on Removal

- Client must match server's threat to remove correct icon
- Identify by (source, damage, inserted_at) tuple
- Floating-point damage may not match exactly (rounding errors)

**Mitigation:**
- Use Duration (integer nanoseconds) as primary key
- Tolerate small damage mismatches (±0.01 difference)
- Server sends unique threat_id per threat (future enhancement)

### Neutral

#### 1. Queue Capacity Formula Ambiguity

- Spec says Focus=-100 gives 1 slot, formula gives 3 + floor(-100/33) = 0
- Need to clarify with PLAYER role or update spec
- MVP: Use formula that matches spec intent (1-6 range)

#### 2. Attribute System Integration

- Instinct uses `instinct_presence()` (i8) not `instinct()` (u8)
- Formula assumes -100 to 100 range (spec) vs 0-100 (attribute getter)
- Must ensure correct attribute method used

#### 3. MVP Defers Damage Types

- Physical-only for MVP (Wild Dog)
- Magic damage, Ward ability, type-specific clears in Phase 2
- Risk: May discover type system issues late

**Acceptance:** MVP validates core mechanic, type system is additive

#### 4. No Queue UI Polish

- MVP: Simple circles with timer rings
- No threat type icons (sword, fireball, etc.)
- No damage number preview
- Polish in Phase 2 based on playtest feedback

## Implementation Phases

### Phase 1: Queue Component and Calculation Functions (Foundation)

**Goal:** Add queue component and shared calculation logic

**Tasks:**
1. Create `common/components/reaction_queue.rs`:
   - `ReactionQueue { threats: VecDeque<QueuedThreat>, capacity: usize }`
   - `QueuedThreat { source, damage, damage_type, inserted_at, timer_duration }`
   - `DamageType` enum

2. Create `common/systems/reaction_queue.rs`:
   - `calculate_queue_capacity(attrs) -> usize`
   - `calculate_timer_duration(attrs) -> Duration`
   - `insert_threat(queue, threat, now) -> Option<QueuedThreat>` (returns overflow)
   - `check_expired_threats(queue, now) -> Vec<QueuedThreat>`
   - `clear_threats(queue, clear_type) -> Vec<QueuedThreat>`

3. Add tests for all functions:
   - Capacity scaling with Focus attribute
   - Timer duration scaling with Instinct attribute
   - Queue overflow behavior (capacity 3, insert 4th → oldest pops)
   - Expiry detection (insert threat, advance time, check expired)
   - Clear types (All, First(n))

**Success Criteria:**
- All calculation tests pass
- Queue overflow correctly returns oldest threat
- Timer expiry detection accurate

**Duration:** 1-2 days

---

### Phase 2: Server-Side Queue Management

**Goal:** Server maintains authoritative queue state

**Tasks:**
1. Initialize queue on spawn:
   - Update `server/systems/renet.rs::do_manage_connections` (player spawn)
   - Update `server/systems/spawner.rs::spawn_npc` (NPC spawn)
   - Insert `ReactionQueue { threats: VecDeque::new(), capacity: calculate_queue_capacity(attrs) }`

2. Create `server/systems/reaction_queue::process_expired_threats`:
   - Run in FixedUpdate
   - Query entities with `(ReactionQueue, ActorAttributes)`
   - Call `check_expired_threats()`
   - Emit `Try::ResolveThreat { ent, threat, reason: TimerExpired }` for each expired
   - Remove expired threats from queue

3. Create placeholder resolution system (damage application in Phase 4):
   - `server/systems/combat::resolve_threat`
   - Processes `Try::ResolveThreat`
   - For now: just log threat resolution (damage pipeline not ready)

**Success Criteria:**
- Entities spawn with empty queue, correct capacity
- Server detects expired threats in FixedUpdate
- Expiry events emitted (logged, not processed yet)

**Duration:** 1-2 days

---

### Phase 3: Threat Insertion (Damage Event → Queue)

**Goal:** Incoming damage inserts into queue instead of applying immediately

**Tasks:**
1. Update damage event flow (current: damage applies to Health directly):
   - When damage occurs, create `QueuedThreat`
   - Insert into target's `ReactionQueue`
   - If overflow: emit `Try::ResolveThreat { reason: Overflow }`
   - Emit `Do::InsertThreat { ent, threat }` to clients

2. Network message support:
   - Add `Event::InsertThreat` to `common/message.rs`
   - Serialize `QueuedThreat` struct
   - Client receives and inserts into visual queue

3. Test insertion:
   - Wild Dog attacks player → threat inserted into queue
   - Queue fills → 4th threat causes overflow, oldest resolves immediately
   - Client receives `InsertThreat` events

**Success Criteria:**
- Wild Dog attacks insert threats into player queue
- Queue overflow correctly triggers immediate resolution
- Client sees threats appear in queue (no UI yet, just component state)

**Duration:** 1-2 days

---

### Phase 4: Threat Resolution and Damage Application

**Goal:** Expired threats apply damage with passive modifiers

**Tasks:**
1. Complete `server/systems/combat::resolve_threat`:
   - Query target's `ActorAttributes` for armor/resistance
   - Calculate modified damage (from ADR-002 passive modifier formulas)
   - Apply damage to `Health` component
   - Emit `Do::ApplyDamage { ent, amount, source }`

2. Client damage handling:
   - Receive `Do::ApplyDamage`
   - Update Health component (state/step)
   - Remove corresponding threat from visual queue (match by source + inserted_at)
   - Play damage effect

3. Integration test:
   - Wild Dog attacks player → threat queued
   - Timer expires (wait 1 second) → damage applies
   - Health decreases by (damage * (1 - armor))

**Success Criteria:**
- Threats apply damage on expiry
- Armor/resistance correctly reduce damage
- Client Health updates and threat disappears from queue

**Duration:** 2 days

---

### Phase 5: Dodge Ability (Queue Clearing)

**Goal:** Player can use Dodge to clear entire queue

**Tasks:**
1. Create ability usage flow:
   - Client input: bind Dodge to key (e.g., Spacebar)
   - Client predicts: `clear_threats(queue, ClearType::All)`, `stamina.step -= 30`
   - Client sends `Try::UseAbility { ent, ability: Dodge }`

2. Server validation (`server/systems/abilities::process_use_ability`):
   - Check `stamina.current >= 30`
   - Check `queue.threats.len() > 0` (can't Dodge empty queue)
   - Check GCD not active (from ADR-002 GCD system)
   - If valid: spend stamina, clear queue, emit confirmations
   - If invalid: emit `Do::AbilityFailed { reason }`

3. Server broadcasts:
   - `Do::ClearQueue { ent, clear_type: All }`
   - `Do::Stamina { ent, current, max }` (updated stamina)
   - `Do::Gcd { ent, typ: Reaction, duration: 500ms }`

4. Client confirmation:
   - Receive `Do::ClearQueue` → queue already cleared (prediction correct)
   - Receive `Do::AbilityFailed` → rollback (restore queue? or accept desync)

**Success Criteria:**
- Player can press Dodge key and clear queue
- Stamina cost applied correctly
- GCD prevents spamming Dodge
- Ability fails if insufficient stamina (with error message)

**Duration:** 2-3 days

---

### Phase 6: Queue UI Rendering

**Goal:** Display queue as circular icons with timer rings

**Tasks:**
1. Create `client/systems/reaction_queue_ui.rs`:
   - Query local player's `ReactionQueue`
   - Spawn UI entities for each threat (children of player entity)
   - Position horizontally above player (world-space UI)

2. Timer ring rendering:
   - Use sprite-based approach (not shader for MVP simplicity)
   - Pre-render 10 timer ring frames (0%, 10%, 20%, ..., 100%)
   - Select frame based on `(remaining / duration) * 10`
   - Update sprite every frame

3. UI layout:
   - Icons: 32x32 pixels, 4px spacing
   - Left-to-right = oldest to newest
   - Color-code by damage type (red = physical, blue = magic - future)

4. Update logic:
   - Run in `Update` schedule (every frame)
   - Calculate remaining time per threat
   - Update sprite frame if changed (dirty check)

**Success Criteria:**
- Queue UI visible above player
- Timer rings deplete smoothly
- Icons disappear when threats resolve or cleared
- UI stays positioned above player during movement

**Duration:** 2-3 days

---

## Validation Criteria

### Functional Tests

- **Queue Capacity:** Spawn entities with varying Focus, verify capacity (1-6 slots)
- **Timer Duration:** Spawn entities with varying Instinct, measure expiry time (0.5-1.5s)
- **Overflow:** Fill queue (capacity 3), insert 4th threat → oldest resolves immediately
- **Expiry:** Insert threat, wait timer duration → threat resolves and damages
- **Dodge Clear:** Use Dodge with 3 queued threats → all clear, stamina spent
- **Dodge Fail:** Use Dodge with 20 stamina → fails, queue unchanged

### Network Tests

- **Insertion Sync:** Server inserts threat, client receives within 100ms (measure latency)
- **Timer Desync:** Client/server timer expiry differs by <200ms (acceptable variance)
- **Prediction Rollback:** Client predicts Dodge, server denies → queue restored within 1 frame
- **Queue Overflow:** Overflow threat resolves server-side, client sees damage event

### Performance Tests

- **UI Rendering:** 100 players with full queues (600 threats) → 60fps maintained
- **FixedUpdate Load:** 1000 entities with queues, expiry checks < 1ms per tick
- **Network Bandwidth:** 10 players in combat, threat insertions < 500 bytes/sec per player

### UX Tests

- **Threat Visibility:** Player can clearly see queued threats and remaining time
- **Dodge Responsiveness:** Dodge clears queue within 16ms (1 frame) on client
- **Overflow Feedback:** Player understands when queue full (visual/audio cue)
- **Timer Accuracy:** Visual timer matches actual expiry time (±100ms acceptable)

## Open Questions

### Design Questions

1. **Queue Capacity Formula Clarification?**
   - Spec says Focus=-100 → 1 slot, but formula gives 0
   - Should base be 1 (instead of 3) to match spec?
   - MVP: Use base=1 + bonus formula, validate with PLAYER role

2. **Timer Visual Feedback?**
   - Should timer ring turn red when <250ms remaining (urgent)?
   - Should queue shake/pulse when full (overflow imminent)?
   - MVP: Simple ring depletion, polish in Phase 2

3. **Dodge Keybind?**
   - Spacebar (standard dodge key in many games)?
   - Shift (modifier key, easy to reach)?
   - MVP: Spacebar, configurable in Phase 2

4. **Queue UI Position?**
   - Above character (world-space, moves with player)?
   - Bottom-center HUD (screen-space, fixed position)?
   - MVP: Above character (more intuitive for "threats on you")

### Technical Questions

1. **Rollback on Ability Failure?**
   - Send full queue snapshot on `AbilityFailed`? (expensive)
   - Accept desync, rely on server corrections? (simpler)
   - MVP: Accept desync, server corrections within 125ms

2. **Threat Matching for Removal?**
   - Match by (source, inserted_at)? (Duration is unique)
   - Send unique threat_id from server? (cleaner but more state)
   - MVP: Match by (source, inserted_at), accept rare mismatches

3. **FixedUpdate Expiry Frequency?**
   - 125ms ticks sufficient? (±125ms variance on timer expiry)
   - Run more frequently (50ms)? (more CPU, better precision)
   - MVP: 125ms, optimize if playtest feedback indicates issue

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

- **Multiple Damage Types:** Physical, Magic, Fire, Ice, Poison, True
- **Type-Specific Reactions:** Ward (magic only), Deflect (physical only)
- **Selective Clear:** Counter (first 1), Parry (first 1), Dispel (first 2)
- **Queue Modifiers:** Status effects that pause timers, increase capacity, reduce duration
- **Threat Priority:** Boss attacks highlighted, urgent threats flash red
- **Damage Preview:** Show damage number on threat icon (before resolution)
- **Telegraph Integration:** Ground indicators spawn threats (visual warning)

### Optimization

- **Threat Pooling:** Reuse `QueuedThreat` structs (object pooling)
- **UI Batching:** Single draw call for all queue icons (sprite batching)
- **Delta Updates:** Send queue changes (insert/remove) instead of full state
- **Threat IDs:** Unique IDs for precise matching on removal

### Advanced Features

- **Queue Reordering:** Abilities that swap threat order (tactics)
- **Threat Transfer:** Abilities that move threats to ally's queue (tank mechanic)
- **Delayed Insertion:** Projectiles insert threats on impact (not on launch)
- **Multi-Target Queues:** AOE attacks insert into multiple queues simultaneously

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (reaction queue mechanics, Dodge ability)
- **Attribute System:** `docs/spec/attribute-system.md` (Focus, Instinct scaling)

### Codebase

- **Client Prediction:** `GUIDANCE/ControlledPlugin.md` (input queue pattern)
- **Resource Components:** `src/common/resources/mod.rs` (InputQueues structure)
- **Time Handling:** ADR-002 (Duration from Time::elapsed())
- **UI Plugin:** `GUIDANCE/UiPlugin.md` (character panel, HUD patterns)

### Related ADRs

- **ADR-002:** Combat Foundation (Health, Stamina, CombatState components)
- **(Future) ADR-004:** Ability System and Targeting (ability definitions, UseAbility event)
- **(Future) ADR-005:** Damage Pipeline and Combat Resolution (damage calculation, ResolveThreat)

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md`
- Client prediction patterns: `InputQueues`, `Offset.state/step`

## Date

2025-10-29
