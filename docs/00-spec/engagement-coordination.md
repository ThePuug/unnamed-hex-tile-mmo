# NPC Engagement Coordination

## Overview

NPC engagement coordination solves the **queue overload problem** in multi-enemy combat. When multiple NPCs stand in range and attack on cooldown, the reaction queue fills with a constant stream of threats that have no gaps — leading to unavoidable damage regardless of player skill or level advantage.

Two interlocking systems address this:

1. **Attack-Recovery Loops** — Every NPC alternates between attacking and recovering, creating natural gaps in threat generation
2. **Hex-Based Coordinated Positioning** — The engagement entity assigns each NPC an approach hex, creating organic stagger through pathing distance

Together, these systems make multi-NPC encounters tactical puzzles rather than unavoidable damage streams.

---

## System 1: Attack-Recovery Loops

### Core Concept

Every NPC follows a behavior loop: **attack → recovery → attack**. The recovery phase is a non-threatening period where the NPC has attacked and is not generating new threats. Recovery duration is the primary tuning lever for encounter pacing.

### Recovery Duration by Archetype

| Archetype | Recovery Range | Loop Character | Queue Pressure Profile |
|-----------|---------------|----------------|----------------------|
| **Berserker** | 1–2 seconds | Fast, aggressive | High per-NPC, brief gaps between bursts |
| **Juggernaut** | 3–5 seconds | Slow, heavy | Low frequency, high damage per threat |
| **Defender** | 4–6 seconds | Reactive, passive | Very low unprovoked cadence; pressure from Counter reflection, not queue volume |
| **Kiter** | Implicit (flee phase) | Mobile, evasive | Gaps from repositioning; attack → flee → reposition → attack |

### Randomized Variance

Recovery duration is randomized within the archetype's range (e.g., Juggernaut recovers in 3–5 seconds, uniformly distributed). This prevents re-synchronization: even if two Juggernauts attack simultaneously, their next attacks desync after one cycle.

**Why randomized:**
- Without variance, NPCs that happen to attack together will attack together forever
- With variance, even a small range (±1 second) guarantees desynchronization within 2–3 cycles
- Creates organic, unpredictable combat rhythm rather than metronomic patterns

### Per-Archetype Loop Details

**Berserker (1–2s recovery):**
- Attacks immediately when in range
- Short recovery creates rapid loop — the "pressure" archetype
- Individual Berserker is manageable; cluster of 3 is intense
- Recovery is pure pause (no movement or abilities during recovery)

**Juggernaut (3–5s recovery):**
- Hits hard when it attacks — big threat value per queue entry
- Long recovery means lots of breathing room between hits
- Player can use recovery windows to deal damage or reposition
- Even 3 Juggernauts have manageable cadence thanks to long recovery

**Defender (4–6s recovery):**
- Rarely attacks unprovoked — longest recovery of all melee archetypes
- Primary queue pressure comes from Counter (reflected damage when player attacks)
- The puzzle is "can you burst this Defender without eating reflected damage?"
- Passive but punishing — dares the player to engage

**Kiter (implicit recovery):**
- Recovery is the flee/reposition phase (already implemented via Kite AI)
- Attack → flee → reposition at orbital range → attack
- No explicit recovery timer needed — movement duration IS the recovery
- Naturally has the most variance (path distance varies)

---

## System 2: Hex-Based Coordinated Positioning

### Core Concept

NPCs within an engagement are coordinated by the engagement entity (which already exists as their parent, per ADR-014). The engagement assigns each NPC an **approach hex** — one of the six hexes adjacent to the player (for melee) or a hex at appropriate range (for ranged).

### Organic Stagger Through Pathing

NPCs must path to their assigned hex before they can attack. The NPC with a direct line arrives first; others take longer paths to reach their assigned face. This creates natural desynchronization without artificial delays — arrival time is a function of hex pathing distance.

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

### Hex Reservation

The engagement coordinates hex assignment so NPCs don't compete for the same face:
- Each melee NPC is assigned a unique adjacent hex
- Maximum 6 melee NPCs can simultaneously engage one player (hex grid geometry)
- If fewer than 6 faces are available (terrain, other entities), excess NPCs queue at secondary positions (2 hexes out)
- This is coordinated behavior — these NPCs spawned as a pack and act like one

### Reassignment on Player Movement

When the player moves, the engagement reassigns approach hexes based on the player's new position and current NPC positions:
- An NPC that was 1 hex from its target face might now be 2+ hexes away
- NPCs must repath to new assigned hexes
- This resets stagger timing — player can actively manipulate engagement tempo

**Reassignment frequency:** On player tile change (not sub-tile movement). Reassignment only fires when `Position.tile` changes, avoiding thrashing from sub-tile sliding.

### Secondary Positions

When all 6 adjacent hexes are occupied (or fewer are available), excess NPCs hold at secondary positions:
- Wait 2 hexes from player, assigned to a preferred face
- When an adjacent hex opens (NPC killed, NPC enters recovery and steps back), the secondary NPC advances to claim it
- Secondary NPCs don't attack (not in range) — they're queued reinforcements

---

## System 3: Per-Archetype Positioning Strategy

The engagement assigns approach hexes differently based on archetype composition.

### Juggernaut — Surround

The engagement spreads Juggernauts evenly around available faces.

**Assignment logic:**
- 2 Juggernauts → opposite faces (180° apart)
- 3 Juggernauts → faces at ~120° apart
- 4 Juggernauts → faces at ~90° apart
- 5–6 Juggernauts → evenly distributed across available faces

**Fantasy:** Heavy, methodical, closing in from all sides. Maximum coverage cuts off escape routes. Punishes players who don't reposition.

**Counterplay:** Move through the gap between two Juggernauts, forcing all of them to repath. Long recovery means the player has time to exploit gaps.

### Berserker — Cluster

The engagement assigns adjacent faces, approaching from roughly the same direction.

**Assignment logic:**
- All Berserkers assigned to 2–3 adjacent faces on the same side of the player
- If 3+ Berserkers, they cluster on one "hemisphere" (3 adjacent faces)
- Similar path lengths from the same direction → arrive close together in time

**Fantasy:** A pack charging in. Intense burst pressure from one direction. The player has clear escape on the opposite side.

**Counterplay:** Step away from the cluster direction — Berserkers must re-close as a pack. Short recovery means they're dangerous up close but can be kited.

### Defender — Loose Perimeter

Defenders spread out but don't aggressively close to melee range.

**Assignment logic:**
- Defenders hold at 2–3 hex range, not competing for adjacent hexes
- Only close to melee range opportunistically (when player approaches them)
- Spread evenly around the perimeter at their preferred range

**Fantasy:** A defensive line daring you to engage. Counter makes attacking them punishing. Low queue pressure — the threat is reflected damage, not incoming attacks.

**Counterplay:** Ignore Defenders and focus other targets. Or burn them down and eat the Counter damage. Don't half-commit.

### Kiter — Orbital

Kiters maintain range and don't compete for melee faces.

**Assignment logic:**
- Kiters orbit at 3–6 hex range
- Maintain line of sight while staying away from melee
- Reposition when player closes distance (flee behavior)
- In mixed groups, Kiters layer on top of melee coordination without interference

**Fantasy:** Harassing fire from range. Forces the player to choose: deal with melee threats or chase down the Kiters?

**Counterplay:** Close distance to force flee behavior, temporarily removing queue pressure. Or position so terrain blocks their line of sight.

### Mixed-Archetype Composition

Mixed groups create compound positional puzzles:

| Composition | Positional Puzzle |
|-------------|-------------------|
| Juggernauts + Kiters | Juggernauts surround and close from all sides while Kiters orbit at range. Player must break through Juggernaut ring to reach Kiters. |
| Berserkers + Defender | Berserkers cluster-charge from one direction while Defender holds the opposite side. Player chooses: kite the Berserkers (exposing back to Defender Counter range) or push through Defender. |
| Berserkers + Kiters | Fast closing pressure + ranged harassment. Berserkers cluster from one side, Kiters orbit. Kill Berserkers first (high queue pressure) or neutralize Kiters (remove ranged annoyance)? |
| All four | Full tactical puzzle. Each archetype occupies its role. Player must prioritize based on build and terrain. |

---

## Positional Player Agency

The hex grid enables meaningful player positioning decisions:

### Active Tempo Management

- **Move toward a Juggernaut** to force its neighbors to repath to new faces — buys time on the queue
- **Step away from a Berserker cluster** to force them all to re-close from the same direction, keeping the escape route open
- **Close distance on Kiters** to force them into flee behavior, temporarily removing their queue pressure

### Terrain Exploitation

- **Back against impassable terrain** to reduce approach angles from 6 to 3–4. Caps simultaneous melee pressure but limits escape routes. Risk/reward trade-off.
- **Use chokepoints** (narrow passages between terrain) to funnel NPCs into 1–2 approach angles, drastically reducing simultaneous threats
- **Elevation advantages** — cliff edges block certain faces, reducing melee exposure

### Engagement Shaping

- **Split attention** by positioning between two groups, forcing the engagement to reassign hexes frequently
- **Peel NPCs** by moving away — NPCs with different movement speeds create natural separation over distance
- **Trap Kiters** by backing toward terrain that blocks their orbital path

---

## Worked Examples

### Example 1: Three Juggernauts (Surround)

**Setup:** Engagement spawns 3 Juggernauts. Engagement assigns faces at 120° spread.

**Phase 1 — Approach:**
- Juggernaut A has direct path (1 hex away) → arrives first, attacks
- Juggernaut B paths around (3 hexes) → arrives ~2 ticks later, attacks
- Juggernaut C takes longest route (4 hexes) → arrives last, attacks

**Phase 2 — Staggered combat:**
- J-A attacks, enters recovery (3–5s). J-B arrives, attacks. J-C arrives, attacks.
- By the time J-B and J-C enter recovery, J-A may be ready to attack again.
- Randomized recovery ensures the cycle never perfectly synchronizes.

**Phase 3 — Player repositions:**
- Player steps toward J-A (closest). J-A's assigned hex shifts to the new adjacent hex.
- J-B and J-C must repath to new faces relative to player's new position.
- Recovery timers continue ticking — some NPCs repath during recovery, arriving ready to attack.

**Queue pressure:** Player faces 1 threat at a time initially, scaling to 1–2 as NPCs complete approach. With 3–5s recovery, there are always gaps between attacks from any single Juggernaut. The puzzle is managing timing, not surviving constant damage.

### Example 2: Three Berserkers (Cluster)

**Setup:** Engagement spawns 3 Berserkers. Engagement assigns 3 adjacent faces on same side.

**Phase 1 — Rush:**
- All three path from roughly the same direction — arrive close together
- First arrival attacks immediately; others arrive within 1–2 ticks
- Burst of 3 threats in rapid succession fills queue fast

**Phase 2 — Burst and breathe:**
- All three enter recovery (1–2s) — brief gap with no new threats
- Gap is short but real — time for one reaction or dismiss
- Then the burst hits again, but randomized recovery means not all three fire simultaneously

**Phase 3 — Player kites:**
- Player steps to opposite side of cluster — all 3 Berserkers must re-close
- Creates distance → approach gap → another staggered arrival
- If player doesn't move, pressure is intense but predictable

**Queue pressure:** High burst, brief gaps. The escape route (opposite side) is always clear. Player agency comes from choosing when to stand and fight vs. when to kite.

### Example 3: Mixed Group (1 Juggernaut, 1 Berserker, 1 Kiter)

**Setup:** Engagement spawns mixed group.

**Positioning:**
- Juggernaut assigned face opposite the Berserker (surround instinct)
- Berserker takes closest face (cluster instinct, but only one so it charges direct)
- Kiter orbits at range (3–6 hexes), not competing for faces

**Phase 1 — Approach:**
- Berserker closes fastest (aggressive, takes closest face) → attacks first
- Juggernaut takes longer route to opposite face → arrives 2–3 ticks later
- Kiter immediately starts harassing from range

**Phase 2 — Positional puzzle:**
- Player faces Berserker first (fast close, short recovery → frequent threats)
- Juggernaut arrives from behind — now threats from two directions
- Kiter adds ranged threats from a third angle
- Player must choose: face Berserker (expose back to Juggernaut), reposition (Berserker catches up), or rush Kiter (leave melee threats behind)

**Queue pressure:** Layered and diverse. Berserker: frequent small threats. Juggernaut: infrequent big threats. Kiter: intermittent ranged threats. No single archetype overwhelms, but together they demand prioritization.

### Example 4: Terrain Interaction

**Setup:** Player backs against cliff edge. Only 4 hexes accessible.

**Engagement response:**
- Engagement can only assign 4 faces maximum
- If 3 Juggernauts present: one assigned adjacent hex, two assigned to the 3 remaining faces; if only 4 faces total, one Juggernaut takes secondary position (2 hexes out, waits for opening)
- Berserker cluster limited to fewer adjacent faces — cluster effect partially disrupted

**Trade-off:**
- Player caps simultaneous melee pressure (max 4, not 6)
- But escape routes are limited (cliff behind, NPCs in front)
- If one NPC is killed, secondary position NPC advances
- Terrain is a tool, not a solution

---

## Tuning Knobs

### Recovery Duration

| Archetype | Min Recovery | Max Recovery | Unit |
|-----------|-------------|-------------|------|
| Berserker | 1.0 | 2.0 | seconds |
| Juggernaut | 3.0 | 5.0 | seconds |
| Defender | 4.0 | 6.0 | seconds |
| Kiter | N/A (flee phase) | N/A | — |

### Positioning Parameters

| Parameter | Value | Description |
|-----------|-------|-------------|
| Surround spread | Even distribution | Juggernauts spread across available faces equally |
| Cluster max faces | 3 | Maximum adjacent faces Berserkers occupy |
| Defender perimeter range | 2–3 hexes | Distance Defenders hold from player |
| Kiter orbital range | 3–6 hexes | Min/max range for Kiter positioning |
| Reassignment trigger | Player tile change | When engagement recalculates hex assignments |
| Secondary position range | 2 hexes | Distance excess NPCs wait from player |

### Interaction with Combat Balance Systems

These systems interact with the combat balance overhaul (RFC-017):

- **Super-linear stat scaling** — Higher-level player has more HP to absorb threats during stagger gaps
- **Commitment-ratio queue capacity** — More queue slots means more staggered threats can coexist without overflow
- **Reaction window level gap** — Wider windows against weaker NPCs compound with recovery gaps (more time per threat AND fewer threats per second)
- **Dismiss mechanic** — Dismiss is most useful when threats arrive staggered (dismiss low-priority, react to high-priority)

The coordination system handles the **action economy problem** (how many threats per second). The balance systems handle the **power scaling problem** (how strong each threat is). Together they make multi-NPC combat tractable across both count and level.

---

## References

- [RFC-014: Spatial Difficulty System](../01-rfc/014-spatial-difficulty-system.md) — Archetype definitions
- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md) — Complementary balance systems
- [ADR-003: Component-Based Resource Separation](../02-adr/003-component-based-resource-separation.md) — Reaction queue
- [ADR-012: AI TargetLock Behavior Tree Integration](../02-adr/012-ai-targetlock-behavior-tree-integration.md) — NPC AI
- [ADR-014: Combat HUD Layered Architecture](../02-adr/014-combat-hud-layered-architecture.md) — Engagement entity as parent
- [ADR-017: Universal Lockout + Synergy Architecture](../02-adr/017-universal-lockout-synergy-architecture.md) — Ability pacing
- [Combat System Spec](combat-system.md) — Core combat mechanics
