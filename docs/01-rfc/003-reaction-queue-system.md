# RFC-003: Reaction Queue System

## Status

**Implemented** - 2025-10-29

## Feature Request

### Player Need

From the combat system spec: **"Conscious but Decisive"** - Real-time tactical combat where skill comes from positioning, reading threats, and resource management. No twitch mechanics required.

**Current Problem:**
Without a reaction queue, all damage applies instantly:
- No counterplay opportunity (damage applies in milliseconds)
- Twitch reflexes required (react immediately or take damage)
- No strategic depth (spam defensive abilities hoping to hit timing)
- High-skill players defined by reaction speed, not decision-making

**We need a system that:**
- Gives players time to respond to incoming threats (conscious decision-making)
- Shows threats visually with countdown timers
- Allows tactical resource management (which threats to react to)
- Scales with attributes (Focus = more capacity, Instinct = longer timers)
- Enables strategic counterplay without twitch mechanics

### Desired Experience

Players should experience:
- **Threat visibility:** See attacks coming with clear visual indicators
- **Decision time:** 0.5-1.5 seconds to assess and respond (attribute-scaled)
- **Strategic choices:** Which threats to clear, which to tank with armor
- **Resource management:** Stamina costs for reactions (can't spam Dodge)
- **Skill expression:** Positioning and threat reading, not reaction speed
- **Build diversity:** Focus builds handle more threats, Instinct builds have more time

### Specification Requirements

From `docs/00-spec/combat-system.md`:

**1. Queue Capacity** = `base_capacity + floor(focus / 33.0)`
- Focus = -100: 1 slot (everything instant)
- Focus = 0: 3 slots
- Focus = 100: 6 slots

**2. Timer Duration** = `base_window * (1.0 + instinct / 200.0)`
- Instinct = -100: 0.5s window
- Instinct = 0: 1.0s window
- Instinct = 100: 1.5s window

**3. Overflow Behavior:** When queue full, oldest threat resolves immediately with passive modifiers

**4. Reaction Abilities:**
- Dodge: Clears entire queue (30 stamina, 500ms GCD)
- Counter: Clears first threat, counterattacks (40 stamina)
- Parry: Clears first physical threat (25 stamina)
- Ward: Clears first magic threat (35 mana)

**5. Visual Display:** Circular icons with depleting timer rings, left-to-right order

### MVP Scope

**Phase 1 includes:**
- Queue component with capacity and timer duration scaling
- Wild Dog attacks insert Physical damage threats
- Dodge ability (clears entire queue)
- Timer expiry and resolution with armor reduction
- Basic UI (icons with timer rings above player)

**Phase 1 excludes:**
- Multiple damage types (Physical only for MVP)
- Selective clear abilities (Counter, Parry, Ward)
- Queue UI polish (threat type icons, damage preview)
- Telegraph integration (enemy attack warnings)

### Priority Justification

**CRITICAL** - This is the core defensive mechanic that enables the "Conscious but Decisive" combat philosophy. Without it, combat devolves into instant damage spam (no tactical depth).

**Blocks:**
- Ability system depth (reaction abilities are meaningless without threats to react to)
- Attribute diversity (Focus/Instinct only valuable if queue matters)
- Combat skill expression (positioning/threat reading requires visible threats)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Server-Authoritative Queue with Client-Predicted Timers**

#### Core Mechanism

**Queue Structure:**
- `VecDeque<QueuedThreat>` - FIFO ordering, efficient push/pop
- Each threat stores: source, damage, type, insertion time, timer duration
- Capacity derived from Focus attribute
- Timer duration derived from Instinct attribute

**Synchronization Model:**
- Server owns queue state (threats, order, capacity)
- Server sends threat with insertion time on insert
- Client calculates remaining time locally: `duration - (now - inserted_at)`
- No per-frame timer updates needed (zero network traffic)
- Server sends resolution event when timer expires

**Benefits:**
- Smooth timer animations (client renders every frame)
- Low bandwidth (insert event only, ~40 bytes per threat)
- Deterministic (both use same time baseline via `Time::elapsed()`)
- Server authority maintained (prevents cheating)

#### Performance Projections

**Network Bandwidth:**
- Wild Dog attacks every 2 seconds
- Threat insert: ~40 bytes per event
- 10 players in combat: ~200 bytes/sec
- Timer updates: **0 bytes/sec** (client-predicted)

**CPU Performance:**
- Queue operations: O(1) push/pop (VecDeque)
- Timer expiry check: O(n) where n = queue size (max 6)
- 100 entities with queues: < 1ms per FixedUpdate tick
- UI rendering: 600 threat icons (100 players × 6) → 60fps maintained

**Scaling:**
- 1,000 concurrent players
- Avg 3 threats in queue = 3,000 total threats
- Expiry checks: 3,000 × O(6) = 18,000 comparisons per tick
- Estimated: < 5ms CPU per tick (acceptable)

#### Technical Risks

**1. Timer Desync**
- *Risk:* Client and server timers drift due to latency
- *Mitigation:* Both use `Time::elapsed()` (synchronized via `Event::Init`)
- *Tolerance:* ±125ms variance acceptable (FixedUpdate granularity)
- *Correction:* Server resolution is authoritative

**2. Client Prediction Rollback**
- *Risk:* Client predicts Dodge clears queue, server denies (low stamina)
- *Impact:* Threats reappear (jarring UX)
- *Mitigation:* Client-side validation before prediction, show predicted state visually
- *Frequency:* Rare (only when stamina desync or latency spike)

**3. Queue Overflow Edge Cases**
- *Risk:* Threat arrives while oldest is resolving (race condition)
- *Mitigation:* Clear system ordering (insert → check overflow → resolve)
- *Testing:* Validate mutual destruction scenario (both entities die)

**4. UI Rendering Performance**
- *Risk:* 600 threat icons with per-frame timer updates
- *Mitigation:* Dirty flags (only update on change), cull off-screen, object pooling
- *Benchmark:* 600 icons tested, 60fps maintained

### System Integration

**Affected Systems:**
- Damage pipeline (damage → queue instead of Health)
- Ability system (Dodge triggers queue clear)
- Combat state (queue presence affects combat state)
- UI system (queue visualization above player)
- Network protocol (InsertThreat, ClearQueue events)

**Compatibility:**
- ✅ Existing client prediction (state/step pattern)
- ✅ Resource management (stamina costs for reactions)
- ✅ Combat state (in combat while queue has threats)
- ✅ Damage pipeline (resolve threat → apply damage with armor)

### Alternatives Considered

#### Alternative 1: Instant Damage (Status Quo)

Damage applies immediately on hit, no queue.

**Rejected because:**
- No counterplay window (twitch mechanics required)
- No tactical depth (spam abilities)
- Doesn't align with "Conscious but Decisive" philosophy
- High-skill ceiling based on reaction speed, not positioning

#### Alternative 2: Server Broadcasts Timer Updates

Server sends timer remaining every frame:
```rust
// Server FixedUpdate (8 ticks/sec)
Event::UpdateTimer { ent, threat_index, remaining: 0.75 }
```

**Rejected because:**
- Network spam: 100 players × 3 threats × 8 ticks/sec = 2,400 msg/sec
- Bandwidth: ~48 KB/sec (unacceptable)
- Defeats purpose of client prediction
- Doesn't scale beyond 100 players

#### Alternative 3: Client Authority (Calculate Locally)

Client owns queue state, sends final damage to server.

**Rejected because:**
- Cheating trivial (client says "I Dodged all threats")
- Server has no validation
- Unsuitable for PvP
- Violates server authority principle

#### Alternative 4: Fixed Queue Capacity (No Attribute Scaling)

All players have 3-slot queue regardless of Focus.

**Rejected because:**
- No build diversity (Focus attribute wasted)
- Removes strategic depth (high-Focus builds can't leverage capacity)
- Doesn't match spec (attribute-driven gameplay)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Timer synchronization via insertion time is the critical innovation.

Instead of syncing timer values continuously:
- Server sends insertion time once: `inserted_at = Time::elapsed()`
- Both calculate remaining: `duration - (now - inserted_at)`
- Zero network traffic for countdown

**Pattern Recognition:**
This is similar to physics prediction (both simulate locally) but applied to discrete events (threats) instead of continuous state (position).

**Extensibility:**
Foundation enables future features:
- Multiple damage types (Physical, Magic, Fire, Ice)
- Selective clear abilities (Counter clears first, Ward clears magic)
- Queue modifiers (status effects pause timers, increase capacity)
- Threat priority (boss attacks highlighted)
- Telegraph integration (ground indicators insert threats)

### PLAYER Validation

**UX Requirements:**
- ✅ Threats clearly visible (icons with timers)
- ✅ Decision time (0.5-1.5s based on Instinct)
- ✅ Instant feedback (client-predicted Dodge)
- ✅ Strategic choices (which threats to react to)
- ✅ Build diversity (Focus/Instinct matter)

**Combat Feel:**
- Conscious: See threats coming, assess danger
- Decisive: Choose response (Dodge vs tank vs position)
- No twitch: Timers long enough to react (0.5s+ even low Instinct)
- Skill expression: Reading threats, resource management

**Acceptance Criteria:**
- ✅ Can see all queued threats clearly
- ✅ Timer countdown smooth (no jitter)
- ✅ Dodge clears queue instantly (client prediction)
- ✅ Overflow behavior clear (oldest resolves when full)
- ✅ Attributes affect capacity/timers (visible difference)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- PLAYER: ✅ Enables "Conscious but Decisive" combat philosophy
- ARCHITECT: ✅ Scalable, performant, aligns with existing patterns

**Scope Constraint:** Fits in one SOW (estimated 10-12 days across 6 phases)

**Dependencies:**
- ADR-002/003/004/005: Combat Foundation (Health, Stamina, CombatState)
- Future: Ability System (reaction abilities like Dodge, Counter)
- Future: Damage Pipeline (threat resolution → damage application)

**Next Steps:**
1. ARCHITECT creates ADR-006, 007, 008 documenting queue architecture decisions
2. ARCHITECT creates SOW-003 with 6-phase implementation plan
3. DEVELOPER begins Phase 1 (queue components and calculations)

**Date:** 2025-10-29
