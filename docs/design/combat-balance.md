# Combat Balance

## Overview

This document defines the interlocking systems that address combat balance at scale. The core problem: **linear stat scaling breaks down against multiplicative threat counts**. Three level-0 NPCs overwhelm a level-10 player because each NPC contributes independently to queue pressure while the defender's stats only grow linearly.

Five systems work together to solve this:

| System | Addresses | Mechanism |
|--------|-----------|-----------|
| 1. Super-linear scaling | Stats too weak at high levels | Polynomial multiplier on derived stats |
| 2. Visibility window | Queue perception loses meaning | Investment ratio thresholds |
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

Three stat categories scale independently, each with its own k/p constants:

| Stat Category | Scaling Intent | Rationale |
|---------------|----------------|-----------|
| HP / Survivability | Moderate | Preserves danger from equal-level foes |
| Damage / Offense | Moderate | Balanced with HP growth to preserve level advantage without runaway |
| Reaction stats | Gentle | Reaction windows are bounded by human limits |

**Key constraint:** HP exponent must be ≥ Damage exponent to ensure higher-level fights have more exchanges, not fewer. See code constants in `ActorAttributes::hp_level_multiplier()` and `damage_level_multiplier()`.

### Design Intent

A high-level player fighting multiple low-level NPCs should feel appropriately dominant — super-linear scaling ensures the player's stats outpace the multiplicative threat count. At equal levels, the multiplier is symmetric and combat plays as designed.

---

## System 2: Queue Visibility Window by Commitment Tier

The queue is unbounded — all threats enter regardless. **Visibility window** is driven by Focus → Concentration commitment tier, determining how many threats the player can see and interact with. Threats outside the window still resolve on their timers but cannot be reacted to.

The commitment tier system uses percentage-based thresholds: `derived_value / (total_level × 10) × 100`. Higher Concentration tier → larger visibility window → more threats the player can perceive and respond to.

### Why Ratio-Based

Raw scaling gives the same window regardless of level, making investment feel flat. Ratio scaling rewards proportional commitment — 2 Focus points at level 10 means less commitment than 2 Focus points at level 3.

---

## System 3: Reaction Window Scaling

### Two-Step Timer Calculation

Reaction window duration is computed in two steps:

1. **Gap window** (level difference): `gap_window(defender_level, attacker_level)` uses Gaussian decay (`gap_factor`) to set the base window. Equal levels get the full base window; large level gaps compress it toward zero. This is a one-directional effect — the beneficiary's window is reduced when outleveled, never penalized further.

2. **Reaction contest** (Cunning vs Finesse): `reaction_contest_factor(cunning, finesse)` applies a baseline+bonus multiplier. At equal stats the window is unchanged; Cunning advantage extends it (up to a cap). This ensures a playable floor — the window never shrinks below the gap-derived base.

### Design Intent

- Higher-level defenders fighting lower-level threats get generous reaction windows (threats feel trivial to manage)
- Equal-level combat uses the base window (primary balancing target)
- When outleveled, the gap window compresses — combined with the attacker's super-linear damage, fighting up is punishing but playable
- Cunning investment rewards build choices even across level gaps (build benefit, not level scaling)

---

## System 4: Dismiss Mechanic

### Behavior

**Dismiss** resolves the front threat in the player's reaction queue immediately at full unmitigated damage.

| Property | Value |
|----------|-------|
| Target | Front of own reaction queue |
| Damage taken | Full unmitigated (bypasses passive modifiers) |
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

| Archetype | Loop Character | Queue Pressure Profile |
|-----------|----------------|----------------------|
| **Berserker** | Fast, aggressive | High per-NPC, brief gaps between bursts |
| **Juggernaut** | Slow, heavy | Low frequency, high damage per threat |
| **Defender** | Reactive, passive | Very low unprovoked cadence; pressure from Counter reflection |
| **Kiter** | Mobile, evasive | Gaps from repositioning; attack → flee → reposition → attack |

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

### Scenario: High-Level Player vs Multiple Low-Level NPCs

All five systems combine: super-linear scaling gives dominant stats, visibility window manages queue perception, generous reaction windows make threats trivial to manage, dismiss freely skips weak threats, and NPC coordination prevents overwhelming burst. The encounter should feel appropriately dominant.

### Scenario: Player vs Higher-Level NPC

Fighting above your level is punishing but not impossible. Super-linear damage makes the NPC hit hard, gap window compresses reaction time, and dismissing is costly. Player must use reaction abilities carefully.

### Scenario: Equal-Level Combat

All multipliers are symmetric. Visibility window depends on Focus investment. No gap bonus. Dismiss is a meaningful tactical choice — full unmitigated damage from an equal-level foe is significant. Equal-level combat plays as designed.

---

## Tuning Knobs

All constants are isolated in code for easy adjustment via playtesting. Key tuning areas:

| Area | Constants | Location |
|------|-----------|----------|
| Level multiplier (HP, Damage, Reaction) | k/p per stat category | `ActorAttributes::*_level_multiplier()` |
| Gap window base | Base duration | `gap_window()` in `queue.rs` |
| Contest scaling | Delta normalization, sqrt curve | `contest_factor()`, `reaction_contest_factor()` in `damage.rs` |
| NPC recovery | Per-archetype min/max ranges | `NpcRecovery` in archetype spawn data |
| Commitment thresholds | 20/40/60% tier boundaries | `CommitmentTier::calculate()` |

---

## Mechanism Quick Reference

```
# System 1: Super-Linear Stat Scaling
level_multiplier(level, k, p) = (1 + level × k)^p
effective_stat = linear_stat × level_multiplier

# System 2: Queue Visibility Window (Concentration commitment tier)
commitment_pct = focus_derived / (total_level × 10) × 100
window = Concentration tier → visibility count

# System 3: Reaction Window
base_window = gap_window(defender_level, attacker_level)  // Gaussian decay
multiplier = reaction_contest_factor(cunning, finesse)    // Baseline+bonus
reaction_window = base_window × multiplier

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
