# ADR-011: GCD Component for Cooldown Tracking

## Status

**Ready for Implementation** - 2025-10-30

## Context

**Related RFC:** [RFC-006: AI Behavior and Ability Integration](../01-rfc/006-ai-behavior-and-ability-integration.md)

Combat system requires cooldown enforcement to prevent ability spam. Both players and NPCs need cooldown tracking.

### Requirements

- Prevent ability spam (enforce cooldown between uses)
- Shared by players and NPCs (DRY principle)
- Server-authoritative (prevent cheating)
- Integrates with ability execution (ADR-009)

### Options Considered

**Option 1: Component per Entity** ✅ **SELECTED**
- `Gcd` component on each actor
- Standard ECS pattern
- Easy to query

**Option 2: Global Resource HashMap**
- `HashMap<Entity, GcdState>`
- Centralized tracking
- ❌ Not idiomatic ECS
- ❌ Harder to query

**Option 3: Per-Ability Cooldowns**
- Track each ability separately
- ❌ More complex
- ❌ Overkill for MVP (single ability per actor)

## Decision

**Use component-based GCD tracking (Option 1).**

### Core Mechanism

**GCD Component:**
```rust
pub struct Gcd {
    pub gcd_type: Option<GcdType>,  // None = no GCD active
    pub expires_at: Duration,       // Time::elapsed() when GCD ends
}
```

**Methods:**
```rust
impl Gcd {
    pub fn is_active(&self, now: Duration) -> bool {
        now < self.expires_at
    }

    pub fn activate(&mut self, gcd_type: GcdType, duration: Duration, now: Duration) {
        self.gcd_type = Some(gcd_type);
        self.expires_at = now + duration;
    }

    pub fn clear(&mut self) {
        self.gcd_type = None;
        self.expires_at = Duration::ZERO;
    }
}
```

**Integration with Ability Execution:**
```rust
// Server: execute_ability (from ADR-009)
pub fn execute_ability(
    mut query: Query<(&mut Gcd, ...)>,
    time: Res<Time>,
    // ...
) {
    let (mut gcd, ...) = query.get_mut(ent)?;

    // Check GCD
    if gcd.is_active(time.elapsed()) {
        return Err("Ability on cooldown");
    }

    // Execute ability...

    // Apply GCD
    gcd.activate(def.gcd_type, def.gcd_duration, time.elapsed());
}
```

**Initialization:**
- Spawned NPCs get `Gcd::new()`
- Players get `Gcd::new()` on connection
- Default state: `gcd_type = None`, `expires_at = 0`

---

## Rationale

### 1. Component-Based Storage

- Standard ECS pattern (queries natural)
- Per-entity state (isolated, no shared state issues)
- Easy to extend (add fields without breaking HashMap)

### 2. Duration-Based Expiry

**Uses `expires_at` (not `started_at` + `duration`):**
- Simpler checks: `now < expires_at` vs `now - started_at < duration`
- Matches `Time::elapsed()` pattern (from ADR-002)
- No need to store start time

### 3. Shared by Players and NPCs

- Players: Prevents ability spam (user can't bypass cooldown)
- NPCs: Prevents behavior tree from emitting too many abilities
- Unified validation (same code path for both)

### 4. Server-Authoritative

- Component NOT synced to clients
- Clients infer cooldown from `Do::AbilityUsed` events
- Prevents cheating (client can't fake cooldown state)

---

## Consequences

### Positive

- **Prevents spam:** Enforces minimum time between abilities
- **Shared infrastructure:** Players and NPCs use same component (DRY)
- **Simple queries:** Standard ECS pattern
- **Server-authoritative:** No cheating (clients don't control cooldowns)
- **Low overhead:** Small struct, sparse entities (only actors)

### Negative

- **No client sync:** Clients can't show exact cooldown timers (must infer)
- **Component bloat:** Every actor gets component (but small struct)
- **Validation duplication:** Behavior nodes check GCD, server also checks

### Mitigations

- Clients show cooldown via ability icons (dimmed after use)
- Component overhead negligible (8 bytes per actor)
- Dual validation necessary (optimization + security)

---

## Implementation Notes

**Location:** `common/components/gcd.rs`

**Initialization:**
- `spawner.rs::spawn_npc` → insert `Gcd::new()`
- `renet.rs::do_manage_connections` (player spawn) → insert `Gcd::new()`

**Validation:**
- `server/systems/abilities::execute_ability` → check before execution
- `server/systems/behaviour/use_ability.rs` → check before emitting Try::UseAbility (optimization)

**Network:** Not synced (server-authoritative)

---

## Validation Criteria

**Functional:**
- All actors spawn with `Gcd::new()`
- Server validates GCD before allowing abilities
- GCD prevents spam (<0.5s between abilities)
- Players and NPCs both enforced

**Performance:**
- 1000 actors checking GCD: < 1ms
- Component overhead: negligible (8 bytes per actor)

---

## References

- **RFC-006:** AI Behavior and Ability Integration
- **ADR-002:** Combat Foundation (GCD type enum)
- **ADR-009:** Ability System (execute_ability integration)

## Date

2025-10-30
