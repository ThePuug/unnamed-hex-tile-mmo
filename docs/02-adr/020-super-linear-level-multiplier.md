# ADR-020: Super-Linear Level Multiplier

## Status

Accepted

## Context

The current stat derivation in `resources.rs` uses linear formulas: each attribute point adds a fixed amount to derived stats (HP, damage, armor, etc.). This works for 1v1 combat at similar levels, but breaks down in multi-enemy scenarios.

The core problem: **linear stat scaling does not keep pace with multiplicative threat counts**. Three level-0 NPCs together deal 3× damage and fill the reaction queue 3× faster, but a level-10 player only has ~2× the stats. The level advantage is insufficient.

Additionally, reaction windows (System 3 from RFC-017) should reward level advantage: a high-level defender fighting low-level threats should have more time to react, reflecting experience and skill superiority.

**References:**
- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md) — Systems 1 and 3
- [Combat Balance Design Doc](../00-spec/combat-balance.md) — Full formula reference
- [ADR-005: Derived Combat Stats](005-derived-combat-stats.md) — Existing linear derivation
- [Attribute System Spec](../00-spec/attribute-system.md) — Attribute definitions

## Decision

Apply a **polynomial level multiplier** after existing linear stat derivation. Additionally, apply a **level gap modifier** to reaction window durations.

### Core Mechanism

**Level Multiplier Formula:**

```rust
fn level_multiplier(level: u8, k: f32, p: f32) -> f32 {
    (1.0 + level as f32 * k).powf(p)
}
```

Applied as:
```
effective_stat = linear_derived_stat × level_multiplier(level, k, p)
```

**Stat Category Constants:**

| Category | k | p | Level 0 | Level 5 | Level 10 |
|----------|------|------|---------|---------|----------|
| HP / Survivability | 0.10 | 1.5 | 1.00 | 1.84 | 2.83 |
| Damage / Offense | 0.15 | 2.0 | 1.00 | 3.06 | 6.25 |
| Reaction stats | 0.10 | 1.2 | 1.00 | 1.64 | 2.30 |

**Level Gap Reaction Window Modifier:**

```rust
fn gap_multiplier(defender_level: u8, attacker_level: u8) -> f32 {
    let gap = defender_level.saturating_sub(attacker_level) as f32;
    (1.0 + gap * WINDOW_SCALING_FACTOR).min(WINDOW_MAX_MULTIPLIER)
}
// WINDOW_SCALING_FACTOR = 0.15
// WINDOW_MAX_MULTIPLIER = 3.0
```

Applied when inserting threats into the reaction queue:
```
reaction_window = instinct_base_window × gap_multiplier(defender_level, attacker_level)
```

**Key properties:**
- Level 0 multiplier = 1.0 for all stat categories (backward compatible)
- Existing linear formulas are preserved — multiplier is applied after
- Reaction window gap only benefits the defender (no penalty for fighting up)
- Gap multiplier capped at 3.0× to prevent infinite windows

## Rationale

**Why polynomial, not exponential:**
- Polynomial growth `(1 + kx)^p` is bounded and predictable
- Exponential `e^(kx)` grows too fast at high levels
- Polynomial with p > 1 gives the desired super-linear curve without runaway scaling

**Why separate k/p per stat category:**
- HP scales moderately (p=1.5): danger should persist even at high levels
- Damage scales aggressively (p=2.0): offensive power rewards are satisfying
- Reaction stats scale gently (p=1.2): human reaction time has natural limits

**Why reaction window gap uses a separate formula:**
- Level multiplier affects base stat values (static)
- Gap multiplier is per-threat and depends on both entities (dynamic)
- Separating them allows independent tuning

**Why no penalty when outleveled:**
- Fighting stronger enemies is already punished by their super-linear damage
- Shorter reaction windows would compound punishment excessively
- Floor at 1.0 keeps combat playable even when overmatched

## Consequences

**Positive:**
- Level progression feels meaningful (each level compounds)
- Multi-enemy encounters scale correctly (high-level player survives proportionally)
- Reaction windows reward level advantage (trivial threats are easy to manage)
- Backward compatible (level-0 behavior unchanged)

**Negative:**
- Introduces tuning complexity (6 constants for stat scaling + 2 for gap multiplier)
- High-level entities significantly outclass low-level ones (potential balance concerns in PvP)
- Test values need updating (any test with specific stat expectations)

**Mitigations:**
- All constants are named and isolated for easy adjustment
- PvP can use separate scaling if needed (future consideration)
- Test utilities can accept multiplier parameters

## Implementation Notes

**Files Affected:**
- `src/common/systems/combat/resources.rs` — Apply multiplier after existing linear derivation
- `src/common/components/mod.rs` — ActorAttributes may need level field accessible for multiplier
- `src/server/systems/reaction_queue.rs` (or equivalent) — Apply gap multiplier on threat insertion
- Test files — Update expected values to account for multiplier

**Integration Points:**
- Multiplier applied in the same function that currently derives stats from attributes
- Gap multiplier applied when creating QueueEntry timer duration
- Both run on server; client uses replicated values

**System Ordering:**
- Level multiplier: runs once on spawn or level-up (not per frame)
- Gap multiplier: runs once per threat insertion (not per frame)

## References

- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md)
- [Combat Balance Design Doc](../00-spec/combat-balance.md)
- [ADR-005: Derived Combat Stats](005-derived-combat-stats.md)
- [Attribute System Spec](../00-spec/attribute-system.md)

## Date

2026-02-09
