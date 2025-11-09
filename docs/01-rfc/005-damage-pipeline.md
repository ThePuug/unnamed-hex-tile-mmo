# RFC-005: Damage Pipeline and Combat Resolution

## Status

**Implemented** - 2025-10-31

## Feature Request

### Player Need

From the combat system spec: Complete damage flow from **abilities → queue → health changes → death**.

**Current Problem:**
Without damage pipeline:
- Abilities exist but don't deal damage (ADR-009 targeting done)
- Reaction queue exists but no threats inserted (ADR-006/007/008)
- Health exists but no way to decrease it (ADR-002)
- Combat system incomplete (no actual combat)

**We need a system that:**
- Generates damage from abilities (BasicAttack)
- Calculates damage with attribute scaling (Might/Focus)
- Inserts threats into reaction queue
- Applies damage when threats expire
- Handles death and mutual destruction

### Desired Experience

Players should experience:
- **Attribute-driven damage:** Might increases physical damage
- **Defensive stats matter:** Vitality reduces incoming damage (armor)
- **Critical hits:** Instinct increases crit chance and multiplier
- **Tactical timing:** React to threats before they resolve
- **Clear feedback:** Damage numbers show how much damage dealt/taken

### Specification Requirements

From `docs/00-spec/combat-system.md`:

**1. Damage Calculation:**
```
Physical: damage = base * (1 + might/100) * (1 - armor)
Magic: damage = base * (1 + focus/100) * (1 - resistance)
Critical: crit_chance = 5% + (instinct/200), crit_mult = 1.5 + (instinct/200)
```

**2. Damage → Queue:**
- Damage does NOT apply immediately
- Insert into ReactionQueue as QueuedThreat
- Timer starts (0.5-1.5s based on Instinct)
- Queue overflow → oldest resolves immediately

**3. Queue Resolution:**
- Timer expires → apply damage with passive modifiers
- Reaction ability used → clear threats without damage
- Queue overflow → oldest threat applies immediately

**4. Passive Modifiers:**
```
Armor: vitality / 200 (max 75% reduction)
Resistance: focus / 200 (max 75% reduction)
```

**5. Mutual Destruction:**
- Both entities have lethal damage queued
- Both die (no tiebreaker)

### MVP Scope

**Phase 1 includes:**
- Damage calculation functions (outgoing, passive modifiers, critical)
- Server damage pipeline (DealDamage → InsertThreat → ResolveThreat → ApplyDamage)
- Client damage response (health updates, damage numbers)
- Client prediction (local player health changes)
- Death flow (Health <= 0 → despawn)
- Mutual destruction support

**Phase 1 excludes:**
- Magic damage type (Physical only)
- Damage over time (DoT)
- Shields/absorb mechanics
- Damage reflection
- Lifesteal

### Priority Justification

**CRITICAL** - This completes the combat system loop. Without damage pipeline, abilities don't affect gameplay. Enables testing combat balance and player vs AI interactions.

**Blocks:**
- Combat playtesting
- AI behavior tuning
- Attribute balance validation

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Server-Authoritative Two-Phase Damage Pipeline**

#### Core Mechanism

**Two-Phase Calculation:**
1. **Phase 1 (Insertion):** Calculate outgoing damage with attacker's attributes
   - Base * (1 + Might/100) * crit_multiplier
   - Store in QueuedThreat
2. **Phase 2 (Resolution):** Apply defender's passive modifiers
   - Outgoing * (1 - armor)
   - Apply to Health

**Pipeline Flow:**
```
Ability hits → DealDamage event
  → Calculate outgoing damage (attacker attributes)
  → Roll critical hit (attacker Instinct)
  → Insert into target's ReactionQueue
  → Broadcast InsertThreat (client shows in queue UI)

Timer expires → ResolveThreat event
  → Apply passive modifiers (defender attributes)
  → Apply to Health
  → Broadcast ApplyDamage (client updates health bar, shows damage number)

Health <= 0 → Death event
  → Despawn entity
```

**Benefits:**
- Attacker scaling fair (at attack time)
- Defender mitigation fair (at defense time)
- Handles mid-queue attribute changes
- Server authoritative (prevents cheating)

#### Performance Projections

**Damage Processing:**
- Wild Dog attacks every 2 seconds
- 10 players in combat: ~5 damage events/sec
- Pipeline processing: < 1ms per damage event
- Total: < 5ms for 100 damage events/sec

**Network Bandwidth:**
- InsertThreat: ~40 bytes
- ApplyDamage: ~30 bytes
- 5 attacks/sec: 350 bytes/sec (negligible)

**Scaling:**
- 100 players in combat: 50 damage events/sec
- 3.5 KB/sec sustained bandwidth
- CPU: < 10ms total processing time

#### Technical Risks

**1. Two-Phase Complexity**
- *Risk:* Confusing to track damage through pipeline
- *Mitigation:* Clear function names, comprehensive tests
- *Documentation:* Comments explain phase separation

**2. Damage Prediction Errors**
- *Risk:* Client predicts damage, threat cleared by Dodge
- *Impact:* Health snaps back up (jarring)
- *Mitigation:* Only predict at resolution (not insertion), visual feedback on rollback
- *Frequency:* Rare (< 1% of damage events)

**3. Floating Text Performance**
- *Risk:* 100 damage numbers = 100 entities
- *Impact:* Rendering overhead
- *Mitigation:* Object pooling if needed, limit max simultaneous (10)

**4. Critical Hit RNG Sync**
- *Risk:* Client can't verify crit rolls
- *Impact:* Trust server (can't detect cheating)
- *Mitigation:* MVP accepts trust model, future: deterministic RNG seed

### System Integration

**Affected Systems:**
- Abilities (generate damage events)
- Reaction queue (threat insertion, expiry)
- Health (damage application)
- Death (Health <= 0)
- UI (damage numbers, health bars)

**Compatibility:**
- ✅ ADR-002: Health resources
- ✅ ADR-006/007/008: Reaction queue
- ✅ ADR-009: Ability system
- ✅ Existing state/step prediction pattern

### Alternatives Considered

#### Alternative 1: Single-Phase Damage Calculation

Calculate final damage at insertion time:
```rust
let final_damage = base * attacker_scaling * (1 - defender_armor);
threat.damage = final_damage;
```

**Rejected because:**
- Attributes may change mid-queue (buffs, debuffs)
- Defender's armor should reflect resolution time (not insertion)
- Less fair (snapshot of defender state at wrong time)

#### Alternative 2: Client-Predicted Damage

Client predicts damage to all entities (local and remote).

**Rejected because:**
- High rollback potential (target may use Dodge)
- Target's armor unknown to attacker
- Errors feel worse for Health than resources
- Simplifies client to predict local only

#### Alternative 3: Combined Damage Event

Send single event with threat insertion and damage application:
```rust
Event::DamageApplied { ent, source, amount, threat }
```

**Rejected because:**
- Hides queue insertion timing (UI doesn't know when to show threat)
- Can't handle queue overflow separately (oldest resolves immediately)
- Less flexible for future features (DoT, shields)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Two-phase calculation separates attacker fairness (damage at attack time) from defender fairness (mitigation at defense time).

**Pattern Recognition:**
- Similar to networking (send time vs receive time)
- Applied to combat (attack time vs resolution time)
- Handles latency in game mechanics (not just network)

**Extensibility:**
Foundation enables future features:
- Damage over time (DoT ticks)
- Shields (absorb before Health)
- Damage modifiers (buffs, debuffs)
- Reflect damage (Counter ability)
- Lifesteal (heal attacker)

### PLAYER Validation

**UX Requirements:**
- ✅ Clear damage numbers (shows how much damage taken)
- ✅ Health bars update smoothly
- ✅ Attributes affect damage (Might increases, Vitality reduces)
- ✅ Critical hits feel impactful (higher damage)
- ✅ Death clear (despawn, respawn flow)

**Combat Feel:**
- Tactical: React to threats before they resolve
- Fair: Attacker/defender attributes matter
- Clear: Visual feedback for damage
- Impactful: Crits and attributes affect outcomes

**Acceptance Criteria:**
- ✅ Wild Dog attacks insert threats into queue
- ✅ Threats expire and apply damage to player
- ✅ Player sees damage numbers and health decrease
- ✅ Player dies when Health reaches 0
- ✅ Mutual destruction: Both die if lethal damage queued

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- PLAYER: ✅ Completes combat system, enables gameplay testing
- ARCHITECT: ✅ Clean pipeline, extensible, integrates ADR-002/006/009

**Scope Constraint:** Fits in one SOW (estimated 9-11 days across 6 phases)

**Dependencies:**
- ADR-002: Health resources
- ADR-006/007/008: Reaction queue
- ADR-009: Ability system and targeting

**Next Steps:**
1. ARCHITECT creates ADR-010 documenting damage pipeline decision
2. ARCHITECT creates SOW-005 with 6-phase implementation plan
3. DEVELOPER begins Phase 1 (damage calculation functions)

**Date:** 2025-10-31
