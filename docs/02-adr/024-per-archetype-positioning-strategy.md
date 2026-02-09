# ADR-024: Per-Archetype Positioning Strategy

## Status

Proposed

## Context

ADR-023 establishes that the engagement entity assigns each NPC a unique approach hex. The remaining question is **how** hexes are assigned — specifically, what spatial pattern each archetype should form around the player.

Without per-archetype strategies, all NPCs would use the same assignment algorithm (e.g., nearest available hex). This produces identical spatial behavior regardless of archetype composition: Berserkers and Juggernauts would spread identically, losing the distinct combat identity established by RFC-014.

The spatial difficulty system (RFC-014) defines four archetypes with distinct combat fantasies:
- **Berserker:** Aggressive burst, fast, charges in
- **Juggernaut:** Heavy, methodical, closes in from all sides
- **Defender:** Reactive, holds position, punishes aggression
- **Kiter:** Ranged, mobile, maintains distance

Each archetype's positioning strategy should reinforce its combat fantasy and create distinct positional puzzles for the player.

**Related:**
- [RFC-018: NPC Engagement Coordination](../01-rfc/018-npc-engagement-coordination.md) — Problem statement
- [Engagement Coordination Design Doc](../00-spec/engagement-coordination.md) — Worked examples
- [ADR-023: Coordinated Hex Assignment](023-coordinated-hex-assignment.md) — Assignment infrastructure
- [RFC-014: Spatial Difficulty System](../01-rfc/014-spatial-difficulty-system.md) — Archetype definitions

## Decision

Each archetype has a distinct **positioning strategy** that determines hex preference ordering during assignment. The engagement entity selects hexes for each NPC based on its archetype's strategy. In mixed-archetype engagements, strategies compose without conflict because they target different spatial zones.

### Core Mechanism

**Strategy Trait:**

Each strategy produces a sorted preference list of candidate hexes for a given NPC:

```rust
enum PositioningStrategy {
    Surround,   // Juggernaut
    Cluster,    // Berserker
    Perimeter,  // Defender
    Orbital,    // Kiter
}
```

**Strategy Implementations:**

**Surround (Juggernaut):**
```
Maximize angular distance from already-assigned hexes.

For each candidate hex:
  score = min(angular_distance(candidate, already_assigned_hex))

Prefer: hex that is farthest from all other assigned hexes
Effect: 2 Juggernauts → opposite faces (180°)
        3 Juggernauts → 120° apart
        4 Juggernauts → 90° apart
```

**Cluster (Berserker):**
```
Minimize angular distance from already-assigned hexes.

For each candidate hex:
  score = -min(angular_distance(candidate, already_assigned_hex))
  // First Berserker: prefer closest face to NPC's current position

Prefer: hex adjacent to already-assigned hexes (same hemisphere)
Effect: 3 Berserkers → 3 adjacent faces, one direction
Cap: max 3 adjacent faces (cluster doesn't wrap around)
```

**Perimeter (Defender):**
```
Target zone: 2–3 hexes from player (not adjacent).

Candidate hexes = hexes at distance 2–3 from player tile
Prefer: even spread across available perimeter hexes
Do not compete for adjacent hexes (melee face slots)
```

**Orbital (Kiter):**
```
Target zone: 3–6 hexes from player (ranged distance).

Candidate hexes = hexes at distance 3–6 from player tile
  with line of sight to player
Prefer: maintain current orbital position (minimize repositioning)
Do not compete for adjacent hexes (melee face slots)
```

**Mixed-Archetype Composition:**

In engagements with multiple archetypes, assignment proceeds by spatial zone priority:

```
1. Surround/Cluster strategies → compete for adjacent hexes (melee zone)
2. Perimeter strategy → takes hexes at 2–3 range (no melee conflict)
3. Orbital strategy → takes hexes at 3–6 range (no melee conflict)
```

When both Surround and Cluster NPCs exist in the same engagement:
- Cluster NPCs assigned first to adjacent faces (they need adjacency for cluster effect)
- Surround NPCs fill remaining faces, maximizing spread from the cluster
- Effect: Berserkers charge from one side, Juggernauts close from the opposite — compound positional puzzle

**Angular Distance Calculation:**

On a hex grid, the 6 adjacent hexes can be indexed 0–5 around the center. Angular distance between two hexes = minimum steps around the ring (wrapping at 6):

```
angular_distance(hex_a, hex_b) = min(|index_a - index_b|, 6 - |index_a - index_b|)

Max distance = 3 (opposite faces)
Min distance = 1 (adjacent faces)
```

### Properties Table

| Strategy | Target Zone | Spread Pattern | Face Competition | Fantasy |
|----------|-------------|----------------|-----------------|---------|
| Surround | Adjacent (1 hex) | Maximum spread | Yes (melee faces) | Closing noose, cuts off escape |
| Cluster | Adjacent (1 hex) | Minimum spread | Yes (melee faces) | Pack charge from one direction |
| Perimeter | 2–3 hexes | Even spread | No | Defensive line, dares engagement |
| Orbital | 3–6 hexes | Maintain range | No | Harassing fire from range |

## Rationale

**Why four distinct strategies (not a parameterized spread):**
- Each strategy creates a qualitatively different puzzle, not just a numerical variation
- Surround vs. Cluster is a spatial pattern difference (spread vs. concentrate), not a degree
- Perimeter and Orbital operate in entirely different hex zones (no common parameter)
- Discrete strategies map cleanly to discrete archetypes

**Why Cluster caps at 3 faces:**
- Hex grid has 6 faces; 3 adjacent = one hemisphere
- More than 3 wraps past the midpoint, becoming a surround pattern
- Cap preserves the "clear escape on opposite side" design intent

**Why Perimeter and Orbital don't compete for melee faces:**
- Defenders and Kiters aren't melee-primary — they shouldn't take melee slots from Berserkers/Juggernauts
- Separate zones mean adding Defenders or Kiters never reduces melee NPC access to adjacent hexes
- Simplifies assignment (melee strategies don't need to account for ranged NPCs)

**Why Cluster assigns first in mixed groups:**
- Cluster NPCs need adjacent faces specifically (their strategy depends on adjacency)
- Surround NPCs are effective in any spread pattern (they adapt to remaining faces)
- Assigning Cluster first preserves the cluster effect; assigning Surround first may scatter the cluster's preferred faces

**Why angular distance (not Euclidean):**
- Hex adjacency is discrete (6 positions), not continuous
- Angular distance on the hex ring captures "how spread out" better than world-space distance
- Two hexes at 180° (opposite faces) have angular distance 3 — maximum spread
- Simpler to compute and reason about than Euclidean distance between world positions

## Consequences

**Positive:**
- Each archetype creates a visually and tactically distinct spatial pattern
- Mixed-archetype groups produce compound puzzles (cluster from one side + surround from the other)
- Strategies compose without conflict (different zones, predictable priority ordering)
- Player can read the spatial pattern and infer archetype composition from NPC positions
- Extension-friendly: new archetypes only need a new strategy enum variant

**Negative:**
- Strategy computation adds logic to hex assignment (more complex than "nearest hex")
- Surround + Cluster in same engagement requires priority ordering (added complexity)
- Perimeter and Orbital strategies need hex distance calculations beyond adjacent ring
- Strategy enum couples positioning to archetype (archetype change = strategy change)

**Mitigations:**
- Strategy computation is O(n²) where n ≤ 6 melee NPCs per engagement — negligible
- Priority ordering is a simple rule (Cluster first, then Surround fills remainder)
- Distance calculations use existing qrz library functions
- Coupling is intentional — archetype IS the strategy determinant in this design

## Implementation Notes

**Files Affected:**
- Engagement components — `PositioningStrategy` enum per NPC or per archetype mapping
- Hex assignment system (ADR-023) — Strategy-aware hex selection
- Archetype definitions — Map archetype → strategy (Berserker → Cluster, etc.)

**Integration Points:**
- Uses `qrz` library for hex distance, neighbor enumeration, and ring calculations
- Extends ADR-023 hex assignment algorithm (step 6: strategy selection)
- Reads archetype component from each NPC entity (already tracked per RFC-014)
- No new network messages — strategy is server-side only, results visible via NPC positions

**Strategy Resolution Order (Mixed Engagements):**
```
1. Identify all child NPCs and their archetypes
2. Group by strategy type: Cluster | Surround | Perimeter | Orbital
3. Assign Cluster NPCs to adjacent hexes first (need adjacency most)
4. Assign Surround NPCs to remaining adjacent hexes (maximize spread from Cluster)
5. Assign Perimeter NPCs to 2–3 hex ring (independent of melee assignments)
6. Assign Orbital NPCs to 3–6 hex ring (independent of melee assignments)
```

## References

- [RFC-018: NPC Engagement Coordination](../01-rfc/018-npc-engagement-coordination.md)
- [Engagement Coordination Design Doc](../00-spec/engagement-coordination.md)
- [ADR-023: Coordinated Hex Assignment](023-coordinated-hex-assignment.md)
- [RFC-014: Spatial Difficulty System](../01-rfc/014-spatial-difficulty-system.md)
- [ADR-012: AI TargetLock Behavior Tree Integration](012-ai-targetlock-behavior-tree-integration.md)

## Date

2026-02-09
