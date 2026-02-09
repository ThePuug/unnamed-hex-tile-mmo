# ADR-022: Dismiss Mechanic

## Status

Accepted

## Context

When fighting multiple enemies, a player's reaction queue fills with threats of varying severity. Currently, the only options are:

1. **Wait for timer expiry** — take full damage when timer runs out
2. **Use a reaction ability** — costs resources, triggers recovery lockout
3. **Use Deflect** — clears entire queue but costs 50 stamina and triggers lockout

There is no way to efficiently triage low-priority threats. A player fighting three level-0 NPCs alongside one level-10 NPC cannot quickly clear the weak threats to focus reaction abilities on the dangerous one. The weak threats clog the queue, and using Deflect wastes it on trivial damage.

**References:**
- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md) — System 4
- [Combat Balance Design Doc](../00-spec/combat-balance.md) — Full behavior spec
- [ADR-006: Server-Authoritative Reaction Queue](006-server-authoritative-reaction-queue.md) — Queue architecture
- [ADR-017: Universal Lockout + Synergy Architecture](017-universal-lockout-synergy-architecture.md) — Recovery system

## Decision

Add a new **dismiss** verb that resolves the front queue threat immediately at full unmitigated damage, with no lockout, no GCD, and no resource cost.

### Core Mechanism

**Dismiss behavior:**

```
1. Player presses dismiss key
2. Client sends Try::Dismiss { ent } to server
3. Server validates: entity has ReactionQueue with ≥1 entry
4. Server resolves front threat:
   - Apply full threat damage (no armor, no resistance mitigation)
   - Remove threat from queue
   - Free queue slot
5. Server broadcasts result
6. No GlobalRecovery component created (no lockout)
```

**Properties:**

| Property | Value | Rationale |
|----------|-------|-----------|
| Target | Front of own reaction queue | FIFO — consistent with queue model |
| Damage | Full unmitigated | Cost of speed — you pay in HP |
| Lockout | None | Must be usable rapid-fire for triage |
| GCD | None | Independent of ability system |
| Resource cost | None | Barrier to use IS the damage taken |
| Animation | Minimal/none | Speed is the point |

**Options Considered:**

### Option A: Dismiss with Partial Damage

Take 50% of threat damage instead of full.

**Rejected because:**
- Reduces the cost of dismissing, making it too attractive
- Competes with reaction abilities (why Counter if dismiss is cheaper?)
- Blurs the line between dismiss (triage) and reaction (defense)

### Option B: Dismiss with Cooldown

Full damage but 2-second cooldown between dismisses.

**Rejected because:**
- Defeats the triage purpose (can't rapidly clear multiple weak threats)
- Adds unnecessary complexity (another timer to track)
- Full damage is already sufficient cost

### Option C: Dismiss at Full Damage, No Cooldown (Selected)

Full unmitigated damage, no restrictions.

**Selected because:**
- Self-balancing: overuse is punished by HP loss
- Serves triage purpose: rapid clearing of weak threats
- Clean design: no hidden timers, no resource interactions
- Complements existing abilities: dismiss is fast but costly, reaction abilities are slower but protective

## Rationale

**Why full unmitigated damage (no armor/resistance):**
- Creates clear hierarchy: reaction abilities > timer expiry > dismiss
- Timer expiry already applies full damage (with mitigation) — dismiss is WORSE than waiting
- The value of dismiss is **speed**, not damage reduction
- Prevents dismiss from replacing defensive abilities

**Why no lockout:**
- Dismiss is not an ability — it's queue management
- Recovery lockout would prevent using reaction abilities after dismissing
- The design intent is "dismiss weak threat, then react to strong threat" — lockout breaks this flow

**Why no GCD:**
- Same reasoning as no lockout — dismiss must not interfere with abilities
- Player should be able to dismiss and immediately use Counter/Deflect

**Why front-of-queue only:**
- Consistent with existing FIFO queue model (ADR-006)
- Prevents cherry-picking (must deal with threats in order)
- Simpler implementation and mental model

## Consequences

**Positive:**
- Queue management becomes an active skill (not just waiting)
- Multi-enemy combat has a triage option (dismiss weak, react to strong)
- No interference with ability system (no lockout, no GCD)
- Self-balancing (HP cost prevents spam against dangerous threats)
- Simple implementation (queue pop + damage application)

**Negative:**
- New network message type (Try::Dismiss)
- New client input binding needed
- Could be confusing without UI feedback ("what just happened?")
- Players might dismiss reflexively without understanding the HP cost

**Mitigations:**
- UI: flash/pulse on dismissed threat, damage number visible
- Tutorial/tooltip: "Dismiss: Accept full damage to clear threat immediately"
- Damage numbers after dismiss make cost visible
- Future: dismiss confirmation for high-damage threats (optional setting)

## Implementation Notes

**Files Affected:**
- `src/common/message.rs` — Add `Try::Dismiss { ent: Entity }` variant
- `src/server/systems/reaction_queue.rs` — Handle dismiss (pop front, apply unmitigated damage)
- `src/client/systems/combat.rs` — Input handling for dismiss key
- `src/client/systems/input.rs` — Key binding for dismiss

**Network Flow:**
```
Client                          Server
  |                               |
  |-- Try::Dismiss { ent } ------>|
  |                               |-- Validate queue non-empty
  |                               |-- Pop front threat
  |                               |-- Apply full unmitigated damage
  |                               |-- Broadcast queue update
  |<---- QueueUpdate -------------|
  |<---- DamageEvent -------------|
```

**UI Considerations:**
- Keybind: suggest `D` key (dismiss) or right-click on queue slot
- Visual feedback: threat icon disappears with brief flash
- Damage number: show unmitigated damage amount (red, distinct from normal)

**Edge Cases:**
- Empty queue: dismiss fails silently (or shows "nothing to dismiss" feedback)
- Entity dead: dismiss fails (standard dead-entity validation)
- Multiple rapid dismisses: each processes independently (no batching needed)

## References

- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md)
- [Combat Balance Design Doc](../00-spec/combat-balance.md)
- [ADR-006: Server-Authoritative Reaction Queue](006-server-authoritative-reaction-queue.md)
- [ADR-017: Universal Lockout + Synergy Architecture](017-universal-lockout-synergy-architecture.md)

## Date

2026-02-09
