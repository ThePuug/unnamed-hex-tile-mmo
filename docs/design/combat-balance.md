# Combat Balance

## Overview

This document defines the interlocking systems that address combat balance at scale. The core problem: **linear stat scaling breaks down against multiplicative threat counts**. Three level-0 NPCs overwhelm a level-10 player because each NPC contributes independently to queue pressure while the defender's stats only grow linearly.

Five systems work together to solve this:

| System | Addresses | Mechanism |
|--------|-----------|-----------|
| 1. Super-linear scaling | Stats too weak at high levels | Polynomial multiplier on derived stats |
| 2. Commitment-ratio queue | Queue capacity loses meaning | Investment ratio thresholds |
| 3. Reaction window gap | No level advantage in reactions | Wider windows against weaker enemies |
| 4. Dismiss mechanic | No way to triage threats | Skip front threat at full damage |
| 5. NPC engagement coordination | Unavoidable damage streams | Attack-recovery loops + coordinated positioning |

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
| 0 | 1.00 | 1.00 | 1.00 |
| 1 | 1.15 | 1.32 | 1.12 |
| 2 | 1.31 | 1.69 | 1.24 |
| 3 | 1.48 | 2.10 | 1.37 |
| 5 | 1.84 | 3.06 | 1.64 |
| 7 | 2.22 | 4.20 | 1.92 |
| 10 | 2.83 | 6.25 | 2.30 |

### Worked Example: Level-10 vs Three Level-0 NPCs (After Scaling)

| Entity | Level | HP (after multiplier) | Damage (after multiplier) |
|--------|-------|-----------------------|---------------------------|
| NPC A | 0 | 100 × 1.00 = **100** | 20 × 1.00 = **20** |
| NPC B | 0 | 100 × 1.00 = **100** | 20 × 1.00 = **20** |
| NPC C | 0 | 100 × 1.00 = **100** | 20 × 1.00 = **20** |
| Player | 10 | 200 × 2.83 = **566** | 40 × 6.25 = **250** |

Combined NPC damage: 60/tick vs player's 566 HP. Player has ~9.4 ticks of survivability (up from 3.3). Player's 250 damage kills each NPC in 1 hit (100 HP each). Balance restored.

---

## System 2: Queue Capacity by Commitment Tier

Queue capacity is driven by Focus → Concentration commitment tier. The commitment tier system uses percentage-based thresholds: `derived_value / (total_level × 10) × 100`.

### Threshold Table

| Commitment Tier | Threshold | Queue Slots | Interpretation |
|-----------------|-----------|-------------|----------------|
| Tier 0 | < 20% | 1 | No Focus commitment — one threat at a time |
| Tier 1 | ≥ 20% | 2 | Moderate investment — can juggle two threats |
| Tier 2 | ≥ 40% | 3 | Major investment — comfortable multi-threat management |
| Tier 3 | ≥ 60% | 4 | Focus specialist — maximum queue capacity |

### Why Ratio-Based

Raw scaling gives the same slots regardless of level, making investment feel flat. Ratio scaling rewards proportional commitment — 2 Focus points at level 10 means less commitment than 2 Focus points at level 3.

---

## System 3: Reaction Window Level Gap

### Formula

```
reaction_window = instinct_base × gap_multiplier

gap_multiplier = clamp(1.0 + max(0, defender_level - attacker_level) × 0.15, 1.0, 3.0)
```

Where:
- `instinct_base` = base reaction window from Instinct attribute
- `defender_level` = level of the entity with the reaction queue
- `attacker_level` = level of the entity whose threat is being timed
- Minimum multiplier: 1.0 (no penalty for fighting stronger enemies)
- Maximum multiplier: 3.0 (cap prevents infinite windows)

### Worked Examples

Assume `instinct_base = 1.5 seconds`:

| Defender Level | Attacker Level | Gap | Multiplier | Window |
|----------------|----------------|-----|------------|--------|
| 10 | 0 | +10 | 2.50 | 3.75s |
| 10 | 5 | +5 | 1.75 | 2.63s |
| 10 | 10 | 0 | 1.00 | 1.50s |
| 5 | 10 | -5 | 1.00 (floored) | 1.50s |

**Interpretation:** A level-10 defender fighting level-0 threats gets 2.5× more reaction time. When `defender_level < attacker_level`, the multiplier floors at 1.0 — the attacker's super-linear damage scaling is punishment enough.

---

## System 4: Dismiss Mechanic

### Behavior

**Dismiss** resolves the front threat in the player's reaction queue immediately at full unmitigated damage.

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

### System Interactions

| System | Interaction |
|--------|-------------|
| GlobalRecovery | Dismiss does NOT create recovery — always available even during lockout |
| Synergies | Dismiss does NOT trigger or consume synergies |
| Deflect | Complementary — deflect clears all, dismiss clears one at cost |
| Counter | Dismiss is inferior — counter negates damage, dismiss takes full damage |
| Auto-Attack | Independent — auto-attack continues during/after dismiss |

---

## System 5: NPC Engagement Coordination

### The Queue Overload Problem

When multiple NPCs stand in range and attack on cooldown, the reaction queue fills with a constant stream of threats that have no gaps — leading to unavoidable damage regardless of player skill or level advantage. Two interlocking mechanisms solve this.

### Attack-Recovery Loops

Every NPC follows a behavior loop: **attack → recovery → attack**. The recovery phase is a non-threatening period where the NPC has attacked and is not generating new threats.

**Recovery Duration by Archetype:**

| Archetype | Recovery Range | Loop Character | Queue Pressure Profile |
|-----------|---------------|----------------|----------------------|
| **Berserker** | 1–2 seconds | Fast, aggressive | High per-NPC, brief gaps between bursts |
| **Juggernaut** | 3–5 seconds | Slow, heavy | Low frequency, high damage per threat |
| **Defender** | 4–6 seconds | Reactive, passive | Very low unprovoked cadence; pressure from Counter reflection |
| **Kiter** | Implicit (flee phase) | Mobile, evasive | Gaps from repositioning; attack → flee → reposition → attack |

Recovery duration is **randomized** within the archetype's range. This prevents re-synchronization: even if two Juggernauts attack simultaneously, their next attacks desync after one cycle.

### Hex-Based Coordinated Positioning

NPCs within an engagement are coordinated by the engagement entity (their parent). The engagement assigns each NPC a unique **approach hex**, creating organic stagger through pathing distance — the NPC with a direct line arrives first, others take longer paths.

```
Example: 3 Juggernauts, player at center

      [J-C]              J-A: 1 hex away → arrives tick 1
     /     \              J-B: 3 hexes → arrives tick 3
  [  ]     [  ]           J-C: 4 hexes → arrives tick 4
   |  [PLR]  |
  [  ]     [J-B]          Natural stagger: player faces
     \     /               threats sequentially, not
      [J-A]                simultaneously
```

**Reassignment triggers:** Engagement created, player tile change, NPC killed. Sub-tile movement does not trigger reassignment (prevents thrashing).

**Secondary positions:** When all 6 adjacent hexes are occupied, excess NPCs hold at 2 hexes from the player. When an adjacent hex opens, the closest secondary NPC advances to claim it.

### Per-Archetype Positioning Strategy

Each archetype has a distinct spatial pattern:

| Strategy | Archetype | Zone | Pattern | Fantasy |
|----------|-----------|------|---------|---------|
| **Surround** | Juggernaut | Adjacent (1 hex) | Maximum spread | Closing noose, cuts off escape |
| **Cluster** | Berserker | Adjacent (1 hex) | Minimum spread | Pack charge from one direction |
| **Perimeter** | Defender | 2–3 hexes | Even spread | Defensive line, dares engagement |
| **Orbital** | Kiter | 3–6 hexes | Maintain range | Harassing fire from range |

In mixed-archetype engagements, strategies compose without conflict because they target different spatial zones. Cluster NPCs are assigned first (they need adjacency), then Surround fills remaining faces.

### Mixed-Archetype Compositions

| Composition | Positional Puzzle |
|-------------|-------------------|
| Juggernauts + Kiters | Juggernauts surround from all sides while Kiters orbit at range. Player must break through ring to reach Kiters. |
| Berserkers + Defender | Berserkers cluster-charge from one direction while Defender holds opposite side. Kite Berserkers or push through Defender? |
| Berserkers + Kiters | Fast closing pressure + ranged harassment. Kill Berserkers first (high queue pressure) or neutralize Kiters? |
| All four | Full tactical puzzle. Each archetype in its role. Prioritize based on build and terrain. |

### Positional Player Agency

- **Move toward a Juggernaut** — forces its neighbors to repath, buys time on the queue
- **Step away from Berserker cluster** — forces them to re-close, keeping escape route open
- **Close distance on Kiters** — forces flee behavior, temporarily removing queue pressure
- **Back against terrain** — reduces approach angles from 6 to 3–4 (risk/reward trade-off)
- **Use chokepoints** — funnels NPCs into 1–2 approach angles

---

## Interaction Matrix

How the five systems combine in practice:

### Scenario: Level-10 Player vs 3× Level-0 NPCs

1. **Super-linear scaling:** Player has 566 HP, 250 damage vs NPC's 100 HP, 20 damage each
2. **Queue capacity:** If player invested in Focus, queue holds 2-4 threats; if not, 1 slot with overflow
3. **Reaction window:** Each NPC threat gets 2.5× reaction window (level gap +10)
4. **Dismiss:** Player can dismiss trivial 20-damage threats instantly to keep queue open
5. **Coordination:** NPCs arrive staggered (different path lengths), attack-recovery gaps allow breathing room

**Combined effect:** Player tanks 60 damage/tick (trivial vs 566 HP), has generous reaction windows, can dismiss freely, and kills each NPC in one hit. Level-10 vs level-0 feels appropriately dominant.

### Scenario: Level-5 Player vs 1× Level-10 NPC

1. **Super-linear scaling:** NPC has 2.83× HP multiplier and 6.25× damage multiplier
2. **Queue capacity:** Based on player's Focus ratio (unchanged by enemy level)
3. **Reaction window:** No bonus reaction time (multiplier = 1.0)
4. **Dismiss:** Available but costly (NPC damage is high after multiplier)
5. **Coordination:** Single NPC — no coordination benefit

**Combined effect:** Fighting above your level is punishing but not impossible. Player must use reaction abilities carefully — dismissing is expensive.

### Scenario: Level-10 vs Level-10 (Equal Combat)

All multipliers are symmetric. Queue capacity depends on Focus investment. No gap bonus. Dismiss is a tactical choice — full unmitigated damage from an equal-level foe is significant. Equal-level combat unchanged from core design intent.

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
| `WINDOW_SCALING_FACTOR` | 0.15 | 0.05 – 0.25 | Bonus per level gap |
| `WINDOW_MAX_MULTIPLIER` | 3.0 | 2.0 – 5.0 | Cap on window bonus |
| Berserker recovery | 1.0–2.0s | 0.5–3.0s | Berserker queue pressure |
| Juggernaut recovery | 3.0–5.0s | 2.0–8.0s | Juggernaut breathing room |
| Defender recovery | 4.0–6.0s | 3.0–10.0s | Defender passive cadence |
| Cluster max faces | 3 | 2–4 | Berserker spread cap |

---

## Formula Quick Reference

```
# System 1: Super-Linear Stat Scaling
level_multiplier(level, k, p) = (1 + level × k)^p
effective_stat = linear_stat × level_multiplier

# System 2: Queue Capacity (Concentration commitment tier)
commitment_pct = focus_derived / (total_level × 10) × 100
slots = T0(<20%) → 1, T1(≥20%) → 2, T2(≥40%) → 3, T3(≥60%) → 4

# System 3: Reaction Window Level Gap
gap_multiplier = clamp(1.0 + max(0, defender_level - attacker_level) × 0.15, 1.0, 3.0)
reaction_window = instinct_base × gap_multiplier

# System 4: Dismiss
damage_taken = full_unmitigated_threat_damage
lockout = none, cost = none

# System 5: NPC Coordination
recovery_duration = uniform_random(archetype_min, archetype_max)
reassignment_trigger = player_tile_change | npc_death | engagement_create
```

---

**Related Design Documents:**
- [Combat System](combat.md) — Core combat mechanics these systems balance
- [Attribute System](attributes.md) — Commitment tiers, relative contests, absolute scaling
