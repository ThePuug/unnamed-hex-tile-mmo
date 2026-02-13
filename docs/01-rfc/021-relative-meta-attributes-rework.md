# RFC: Relative Meta-Attribute Opposition System

## Status

**Approved** - 2026-02-13

## Summary

Define the relative scaling mode's meta-attributes, their concrete stats, and opposition pairings. Relative stats are the "build benefit" layer — they compare raw stat values between entities with no level scaling. This RFC establishes the three opposition pairs, their mechanical interactions, and the design principle governing how opposition works at this layer.

## Context

The attribute system decomposes each of the six attributes (Might, Grace, Vitality, Focus, Instinct, Presence) into three scaling modes:

- **Absolute** — progression metric. Super-linear scaling with level.
- **Relative** — build benefit. Raw stat difference between entities, no level scaling.
- **Commitment** — build identity. Percentage of total budget invested, discrete tiers.

Each scaling mode has its own opposition map, distinct from the others:

- **Absolute opposition** (from axis/spectrum/shift): Might↔Grace, Vitality↔Focus, Instinct↔Presence
- **Relative opposition** (this RFC): Might↔Focus, Grace↔Instinct, Vitality↔Presence
- **Commitment opposition**: Might↔Presence, Grace↔Vitality, Instinct↔Focus

Rotating oppositions across layers means no single "counter-build" exists for any attribute. An entity must invest across multiple axes to oppose another on all three layers simultaneously, which is impossible under a fixed budget. This creates a richer build space than mirroring the same opposition at every level.

## Design Principle

All three relative pairs follow the same structural rule: **two independent stats that create emergent opposition on a shared mechanical layer.** Neither stat directly modifies or cancels the other. Both are calculated independently. The opposition emerges from the math when both interact with the same system.

## The Three Pairs

### Impact (Might) vs Composure (Focus)

**Mechanical layer:** Recovery timeline.

| Meta-Attribute | Stat | Mechanism |
|----------------|------|-----------|
| Impact | Recovery pushback | Each hit extends the target's recovery timer by an amount proportional to Impact |
| Composure | Recovery reduction | Passively reduces all recovery timers by an amount proportional to Composure |

**Interaction:** Impact is event-driven and offensive — each hit pushes back the target's ability to act. Composure is passive and defensive — it globally shortens recovery regardless of incoming hits. A high-Impact attacker forces longer gaps between the target's actions. A high-Composure defender keeps their action tempo despite incoming hits. Neither stat modifies the other; both independently affect the same recovery timeline.

### Finesse (Grace) vs Cunning (Instinct)

**Mechanical layer:** Lockout-vs-window equation.

| Meta-Attribute | Stat | Mechanism |
|----------------|------|-----------|
| Finesse | Synergy recovery reduction | Reduces the recovery time between chained skills (synergies), tightening burst sequences |
| Cunning | Reaction window | Extends the time available to react to each incoming threat in the queue |

**Interaction:** Skills have synergies — certain ability chains (e.g. Lunge → Overpower) grant reduced recovery on the follow-up. Finesse deepens this discount, compressing burst sequences. Counter has a 1.5s recovery (lockout). A chained attack that arrives during lockout is only reactable if the defender's Cunning-derived reaction window outlasts the remaining lockout. The equation: `chain_gap + reaction_window > lockout`. Finesse reduces chain_gap. Cunning extends reaction_window. They oppose each other directly on the lockout timeline without either stat modifying the other.

**Strategic tradeoff for the attacker:** Synergy chains are predictable — an aware opponent knows what follows a Lunge. Using the chain trades unpredictability for burst tempo. This makes Finesse a calculated risk, not a free advantage.

### Dominance (Presence) vs Toughness (Vitality)

**Mechanical layer:** Sustain ratio.

| Meta-Attribute | Stat | Mechanism |
|----------------|------|-----------|
| Dominance | Healing reduction | Reduces the effectiveness of incoming healing on all entities in the vicinity. Operates as an aura; when multiple Dominance sources overlap, only the strongest effect applies (worst-effect-wins, no stacking) |
| Toughness | Mitigation | Flat damage reduction per hit received |

**Interaction:** Sustain ratio is the relationship between incoming damage and incoming healing. Toughness improves sustain by reducing damage per hit — each heal covers more hits. Dominance degrades sustain by reducing healing effectiveness — each heal restores less. Neither stat modifies the other.

Worked example: 1 attack deals 60 damage, 1 heal restores 180 HP. Without opposition, 1 heal covers 3 hits. Dominance reduces healing by 33% (heal restores 120 HP). Toughness reduces incoming damage by 33% (hit deals 40 damage). Net result: 1 heal still covers 3 hits. The opposition is symmetric.

**Dominance as aura:** Unlike the other relative stats which operate per-interaction between attacker and defender, Dominance radiates from the Presence entity and affects all nearby hostiles. This is intentional — Presence is definitionally about affecting others by existing. The per-entity contest is preserved: each entity in the aura independently contests Dominance with their own Toughness.

**Worst-effect-wins:** When multiple Dominance auras overlap, only the strongest applies. This prevents stacking (a team of Presence builds would otherwise nullify all opposing healing) and naturally diversifies team composition — one Presence player is a strategic asset, two is redundant.

**Design intent for Presence:** Dominance makes the Presence entity a priority target not through taunt or aggro mechanics, but through rational strategic pressure. Leaving a high-Dominance entity alive degrades the opposing team's sustain. The correct response is to kill them — not because the game tells you to, but because the math demands it.

## Complete Relative Table

| Attribute | Meta-Attribute | Stat | Opposed By |
|-----------|---------------|------|------------|
| Might | Impact | Recovery pushback | Composure |
| Grace | Finesse | Synergy recovery reduction | Cunning |
| Vitality | Toughness | Mitigation | Dominance |
| Focus | Composure | Recovery reduction | Impact |
| Instinct | Cunning | Reaction window | Finesse |
| Presence | Dominance | Healing reduction (aura, worst-effect-wins) | Toughness |

## Full Attribute Table (Current State)

| Attribute | Absolute | Relative | Commitment |
|-----------|----------|----------|------------|
| Might | Force (Technique) | Impact (Composure) | Ferocity (Intensity) |
| | Damage | Recovery pushback | *open* |
| Grace | Technique (Force) | Finesse (Cunning) | Poise (Grit) |
| | *open* | Synergy recovery reduction | Evasion |
| Vitality | Constitution (Discipline) | Toughness (Dominance) | Grit (Poise) |
| | HP | Mitigation | *open* |
| Focus | Discipline (Constitution) | Composure (Impact) | Concentration (Flow) |
| | *open* | Recovery reduction | Queue capacity |
| Instinct | Intuition (Gravitas) | Cunning (Finesse) | Flow (Concentration) |
| | *open* | Reaction window | *open* |
| Presence | Gravitas (Intuition) | Dominance (Toughness) | Intensity (Ferocity) |
| | *open* | Healing reduction (aura) | Cadence |

## Open Design Space

This RFC does not address:

- Concrete stats for open absolute cells (Technique, Discipline, Intuition, Gravitas)
- Concrete stats for open commitment cells (Ferocity, Grit, Flow)
- Crit chance (previously on Precision, removed when Grace relative was reworked to Finesse) — needs a new home or may be cut
- Flow commitment rework (time-window-based queue visibility as alternative to Concentration's slot-based model) — separate RFC
- Healing system design and implementation
- Specific numerical values, curves, or coefficients for any stat — these are tuning knobs for playtesting

## Approval

**Status:** Approved

**Implementation Decisions:**
- 1C: Implement minimal healing system alongside this RFC
- 2C: Remove crit system entirely
- 3C: Finesse uses contest formula (not multiplier or addition)
- 4: Impact extends by 25% of max recovery duration
- 5A: Dominance aura uses 5 hex radius
- 6B: Phased implementation (Impact/Composure → Finesse/Cunning → Dominance/Toughness)

**Next Steps:**
1. ADR-031 created (architectural decision documentation)
2. SOW-021 created (3-phase implementation plan)
3. Ready for DEVELOPER role implementation

**Date:** 2026-02-13

## References

- ADR-031: Relative Meta-Attribute Opposition System (architectural decision from this RFC)
- SOW-021: Relative Meta-Attributes Implementation (3-phase implementation plan)
- ADR-003: Reaction Queue
- ADR-012: Universal Lockout
- ADR-029: Relative Stat Contests (contest_modifier formula)
- ADR-030: Reaction Queue Window Mechanic
- Triumvirate spec (Origin/Approach/Resilience)
- Axis/Spectrum/Shift allocation model (unchanged, this RFC operates downstream)