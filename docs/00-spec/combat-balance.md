# Combat Balance: Level Scaling, Queue Capacity, Reaction Windows, and Dismiss

## Overview

This document defines the four interlocking systems that address combat balance at scale. The core problem: **linear stat scaling breaks down against multiplicative threat counts**. Three level-0 NPCs overwhelm a level-10 player because each NPC contributes independently to queue pressure while the defender's stats only grow linearly.

**Related Documents:**
- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md)
- [ADR-020: Super-Linear Level Multiplier](../02-adr/020-super-linear-level-multiplier.md)
- [ADR-021: Commitment-Ratio Queue Capacity](../02-adr/021-commitment-ratio-queue-capacity.md)
- [ADR-022: Dismiss Mechanic](../02-adr/022-dismiss-mechanic.md)
- [SOW-017: Combat Balance Implementation](../03-sow/017-combat-balance-implementation.md)
- [ADR-005: Derived Combat Stats](../02-adr/005-derived-combat-stats.md)
- [ADR-006: Server-Authoritative Reaction Queue](../02-adr/006-server-authoritative-reaction-queue.md)
- [ADR-017: Universal Lockout + Synergy Architecture](../02-adr/017-universal-lockout-synergy-architecture.md)
- [Attribute System Spec](attribute-system.md)
- [Combat System Spec](combat-system.md)

---

## Problem Analysis

### The Linear Scaling Problem

Current stat derivation is linear: each attribute point adds a fixed amount to derived stats. A level-10 entity with 10 points in Might has roughly 2× the damage of a level-0 entity with 0 points.

But combat threat is multiplicative:
- 1 level-0 NPC: 1× damage output, 1 threat in queue
- 3 level-0 NPCs: 3× damage output, 3 threats in queue simultaneously
- Queue fills 3× faster, timer pressure multiplied

**Concrete example (current system):**

| Entity | Level | HP (linear) | Damage (linear) |
|--------|-------|-------------|-----------------|
| NPC A | 0 | 100 | 20 |
| NPC B | 0 | 100 | 20 |
| NPC C | 0 | 100 | 20 |
| Player | 10 | ~200 | ~40 |

Combined NPC damage: 60/tick vs player's 200 HP. Player has ~3.3 ticks of survivability. The 2× stat advantage is overwhelmed by 3× threat count.

### The Solution: Four Interlocking Systems

| System | Addresses | Mechanism |
|--------|-----------|-----------|
| 1. Super-linear scaling | Stats too weak at high levels | Polynomial multiplier on derived stats |
| 2. Commitment-ratio queue | Queue capacity loses meaning | Investment ratio thresholds |
| 3. Reaction window gap | No level advantage in reactions | Wider windows against weaker enemies |
| 4. Dismiss mechanic | No way to triage threats | Skip front threat at full damage |

---

## System 1: Super-Linear Stat Scaling

### Formula

```
effective_stat = linear_stat × level_multiplier(level, k, p)

level_multiplier(level, k, p) = (1 + level × k)^p
```

Where:
- `linear_stat` = value from existing linear derivation (unchanged)
- `level` = entity level (0–10 for MVP)
- `k` = growth rate constant (how much each level contributes)
- `p` = exponent (controls curve shape — higher = more super-linear)

### Stat Category Parameters

| Stat Category | k | p | Rationale |
|---------------|------|------|-----------|
| HP / Survivability | 0.10 | 1.5 | Moderate scaling preserves danger from equal-level foes |
| Damage / Offense | 0.15 | 2.0 | Aggressive scaling rewards offensive investment |
| Reaction stats | 0.10 | 1.2 | Gentle scaling — reaction windows are already bounded by human limits |

### Worked Examples: Level Multiplier

| Level | HP Multiplier (k=0.1, p=1.5) | Damage Multiplier (k=0.15, p=2.0) | Reaction Multiplier (k=0.1, p=1.2) |
|-------|-------------------------------|-------------------------------------|--------------------------------------|
| 0 | (1.00)^1.5 = **1.00** | (1.00)^2.0 = **1.00** | (1.00)^1.2 = **1.00** |
| 1 | (1.10)^1.5 = **1.15** | (1.15)^2.0 = **1.32** | (1.10)^1.2 = **1.12** |
| 2 | (1.20)^1.5 = **1.31** | (1.30)^2.0 = **1.69** | (1.20)^1.2 = **1.24** |
| 3 | (1.30)^1.5 = **1.48** | (1.45)^2.0 = **2.10** | (1.30)^1.2 = **1.37** |
| 5 | (1.50)^1.5 = **1.84** | (1.75)^2.0 = **3.06** | (1.50)^1.2 = **1.64** |
| 7 | (1.70)^1.5 = **2.22** | (2.05)^2.0 = **4.20** | (1.70)^1.2 = **1.92** |
| 10 | (2.00)^1.5 = **2.83** | (2.50)^2.0 = **6.25** | (2.00)^1.2 = **2.30** |

### Worked Example: Level-10 vs Three Level-0 NPCs (After Scaling)

| Entity | Level | HP (after multiplier) | Damage (after multiplier) |
|--------|-------|-----------------------|---------------------------|
| NPC A | 0 | 100 × 1.00 = **100** | 20 × 1.00 = **20** |
| NPC B | 0 | 100 × 1.00 = **100** | 20 × 1.00 = **20** |
| NPC C | 0 | 100 × 1.00 = **100** | 20 × 1.00 = **20** |
| Player | 10 | 200 × 2.83 = **566** | 40 × 6.25 = **250** |

Combined NPC damage: 60/tick vs player's 566 HP. Player has ~9.4 ticks of survivability (up from 3.3). Player's 250 damage kills each NPC in 1 hit (100 HP each). Balance restored.

---

## System 2: Queue Capacity by Commitment Ratio

> **Note:** This formula is being generalized by [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md) into a unified commitment tier system ([ADR-027](../02-adr/027-commitment-tiers.md)). The threshold concept and slot mapping are preserved, but the input changes from `focus_reach / (total_level × 7)` to `commitment_tier(focus, total_budget)` with thresholds at 30/45/60%.

### Formula

```
commitment_ratio = focus_reach / (total_level × 7)
```

Where:
- `focus_reach` = total Focus attribute investment (sum of axis + spectrum contributions)
- `total_level` = entity's current level
- `7` = number of attribute axes (Might, Grace, Vitality, Focus, Instinct, Presence + 1 floating)

**Note:** The constant 7 represents maximum points distributed per level. Adjust if attribute system changes.

### Threshold Table

| Commitment Ratio | Queue Slots | Interpretation |
|------------------|-------------|----------------|
| < 33% | 1 | Minimal Focus investment — one threat at a time |
| 33% – 49% | 2 | Moderate investment — can juggle two threats |
| 50% – 65% | 3 | Major investment — comfortable multi-threat management |
| ≥ 66% | 4 | Focus specialist — maximum queue capacity |

### Worked Examples

**Level 5 Berserker (Focus 0 of 5 total points):**
- `commitment_ratio = 0 / (5 × 7) = 0%` → **1 slot**
- Berserker invests in Might/Instinct, gets minimal queue capacity

**Level 5 Defender (Focus 3 of 5 total points):**
- `commitment_ratio = 3 / (5 × 7) = 8.6%` → **1 slot**
- Note: Even with 60% of points in Focus, the ratio against total pool is low

**Level 10 Focus Specialist (Focus 7 of 10 total points):**
- `commitment_ratio = 7 / (10 × 7) = 10%` → **1 slot**

**Design Note:** The denominator `total_level × 7` represents the total attribute pool. If the actual implementation uses `focus_reach` relative to the Focus axis maximum (not total pool), the thresholds would need adjustment. The key insight is **ratio-based, not absolute** — the exact denominator depends on how `focus_reach` is defined in the attribute system.

### Why Ratio-Based

| Level | Focus Points | Raw Scaling (old) | Ratio Scaling (new) |
|-------|-------------|--------------------|--------------------|
| 3 | 2 | 2 slots (decent) | 2/(3×7) = 9.5% → 1 slot |
| 5 | 2 | 2 slots (same) | 2/(5×7) = 5.7% → 1 slot |
| 10 | 2 | 2 slots (same) | 2/(10×7) = 2.9% → 1 slot |
| 10 | 7 | 7 slots (excessive) | 7/(10×7) = 10% → 1 slot |

Raw scaling gives the same slots regardless of level, making investment feel flat. Ratio scaling rewards proportional commitment.

---

## System 3: Reaction Window Level Gap

### Formula

```
reaction_window = instinct_base × gap_multiplier

gap_multiplier = clamp(1.0 + max(0, defender_level - attacker_level) × scaling_factor, 1.0, 3.0)
```

Where:
- `instinct_base` = base reaction window from Instinct attribute (existing formula)
- `defender_level` = level of the entity with the reaction queue
- `attacker_level` = level of the entity whose threat is being timed
- `scaling_factor` = bonus per level of advantage (default: 0.15 = 15% per level)
- Minimum multiplier: 1.0 (no penalty for fighting stronger enemies — the damage itself is the penalty)
- Maximum multiplier: 3.0 (cap prevents infinite windows)

### Worked Examples

Assume `instinct_base = 1.5 seconds` (mid-range Instinct):

| Defender Level | Attacker Level | Gap | Multiplier | Window |
|----------------|----------------|-----|------------|--------|
| 10 | 0 | +10 | 1 + 10×0.15 = **2.50** | 3.75s |
| 10 | 5 | +5 | 1 + 5×0.15 = **1.75** | 2.63s |
| 10 | 10 | 0 | **1.00** | 1.50s |
| 5 | 10 | -5 | **1.00** (floored) | 1.50s |
| 0 | 10 | -10 | **1.00** (floored) | 1.50s |

**Interpretation:** A level-10 defender fighting level-0 threats gets 2.5× more reaction time. This makes trivial threats manageable even when queue is full.

### Degradation When Outleveled

When `defender_level < attacker_level`, the multiplier floors at 1.0. The defender isn't penalized with shorter windows — the attacker's super-linear damage scaling (System 1) is punishment enough.

---

## System 4: Dismiss Mechanic

### Behavior

**Dismiss** is a new combat verb that resolves the front threat in the player's reaction queue immediately at full unmitigated damage.

| Property | Value |
|----------|-------|
| Target | Front of own reaction queue |
| Damage taken | Full unmitigated (no armor, no resistance) |
| Lockout (GlobalRecovery) | None — does not trigger recovery |
| GCD interaction | None — always available |
| Stamina/Mana cost | None |
| Animation | Minimal (queue item removed immediately) |

### When to Dismiss

Dismiss is a **triage tool**, not a defensive ability:
- Queue full of weak threats blocking a dangerous one → dismiss weak threats to reach the dangerous one
- Timer about to expire anyway → dismiss proactively to free bandwidth for reaction abilities
- Taking full damage is acceptable → skip the queue slot

### When NOT to Dismiss

- Against dangerous threats (full unmitigated damage hurts)
- When reaction abilities are available (deflect/counter negate more damage)
- When queue isn't under pressure (no reason to rush)

### Network Protocol

New message variant in the Try enum:
```
Try::Dismiss { ent: Entity }
```

Where `ent` is the entity whose front threat should be dismissed. Server validates that the entity has a reaction queue with at least one entry, resolves front threat at full damage, and broadcasts the result.

### Interaction with Other Systems

| System | Interaction |
|--------|-------------|
| GlobalRecovery (ADR-017) | Dismiss does NOT create recovery — always available even during lockout |
| Synergies (ADR-017) | Dismiss does NOT trigger or consume synergies |
| Deflect | Dismiss is complementary — deflect clears all, dismiss clears one at cost |
| Counter | Dismiss is inferior — counter negates damage, dismiss takes full damage |
| Auto-Attack | Independent — auto-attack continues during/after dismiss |

---

## Interaction Matrix

How the four systems combine in practice:

### Scenario: Level-10 Player vs 3× Level-0 NPCs

1. **System 1 (Super-linear scaling):** Player has 566 HP, 250 damage vs NPC's 100 HP, 20 damage each
2. **System 2 (Queue capacity):** If player invested in Focus, queue holds 2-4 threats; if not, 1 slot with overflow
3. **System 3 (Reaction window):** Each NPC threat gets 2.5× reaction window (level gap +10)
4. **System 4 (Dismiss):** Player can dismiss trivial 20-damage threats instantly to keep queue open

**Combined effect:** Player tanks 60 damage/tick (trivial vs 566 HP), has generous reaction windows, can dismiss freely, and kills each NPC in one hit. Level-10 vs level-0 feels appropriately dominant.

### Scenario: Level-5 Player vs 1× Level-10 NPC

1. **System 1:** NPC has 2.83× HP multiplier and 6.25× damage multiplier; player has 1.84× and 3.06×
2. **System 2:** Queue capacity based on player's Focus ratio (unchanged by enemy level)
3. **System 3:** No bonus reaction time (defender below attacker, multiplier = 1.0)
4. **System 4:** Dismiss available but costly (NPC damage is high after multiplier)

**Combined effect:** Fighting above your level is punishing but not impossible. The level-10 NPC hits much harder and survives much longer. Player must use reaction abilities carefully — dismissing is expensive.

### Scenario: Level-10 vs Level-10 (Equal Combat)

1. **System 1:** Both have same multiplier — combat is symmetric
2. **System 2:** Queue capacity depends on each entity's Focus investment
3. **System 3:** No gap bonus (multiplier = 1.0 for both)
4. **System 4:** Dismiss is a tactical choice — full unmitigated damage from an equal-level foe is significant

**Combined effect:** Equal-level combat unchanged from current design intent.

---

## Tuning Knobs

All constants are isolated for easy adjustment via playtesting.

| Knob | Default | Valid Range | Affects |
|------|---------|-------------|---------|
| `HP_K` | 0.10 | 0.05 – 0.20 | HP growth rate per level |
| `HP_P` | 1.5 | 1.0 – 2.5 | HP curve shape (1.0 = linear) |
| `DAMAGE_K` | 0.15 | 0.05 – 0.25 | Damage growth rate per level |
| `DAMAGE_P` | 2.0 | 1.0 – 3.0 | Damage curve shape |
| `REACTION_K` | 0.10 | 0.05 – 0.15 | Reaction stat growth rate |
| `REACTION_P` | 1.2 | 1.0 – 2.0 | Reaction stat curve shape |
| `QUEUE_THRESHOLD_1` | 0.33 | 0.20 – 0.40 | Min ratio for 2 slots |
| `QUEUE_THRESHOLD_2` | 0.50 | 0.40 – 0.60 | Min ratio for 3 slots |
| `QUEUE_THRESHOLD_3` | 0.66 | 0.55 – 0.80 | Min ratio for 4 slots |
| `WINDOW_SCALING_FACTOR` | 0.15 | 0.05 – 0.25 | Bonus per level gap |
| `WINDOW_MAX_MULTIPLIER` | 3.0 | 2.0 – 5.0 | Cap on window bonus |

---

## Formula Quick Reference

```
# System 1: Super-Linear Stat Scaling
level_multiplier(level, k, p) = (1 + level × k)^p
effective_stat = linear_stat × level_multiplier

# System 2: Queue Capacity
commitment_ratio = focus_reach / (total_level × 7)
slots = match commitment_ratio {
    r if r >= 0.66 => 4,
    r if r >= 0.50 => 3,
    r if r >= 0.33 => 2,
    _ => 1,
}

# System 3: Reaction Window Level Gap
gap_multiplier = clamp(1.0 + max(0, defender_level - attacker_level) × 0.15, 1.0, 3.0)
reaction_window = instinct_base × gap_multiplier

# System 4: Dismiss
damage_taken = full_unmitigated_threat_damage
lockout = none
cost = none
```

---

**Document Version:** 1.0
**Last Updated:** 2026-02-09
**Maintained By:** Development team
