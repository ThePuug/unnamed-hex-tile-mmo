# ADR-023: Coordinated Hex Assignment via Engagement Entity

## Status

Proposed

## Context

The spatial difficulty system (RFC-014, SOW-014) spawns multi-NPC engagements where each NPC independently paths to the player and attacks on cooldown. This creates two problems:

1. **Simultaneous arrival:** Multiple NPCs path to the same adjacent hex, arriving and attacking at the same time. The reaction queue receives a burst of threats with no gaps.
2. **Position competition:** NPCs compete for the same adjacent hex, causing clumping and blocking each other instead of surrounding the player.

The engagement entity already exists as the parent for spawned NPCs (SOW-014). Currently it serves only as a grouping container for spawning and cleanup. This ADR extends the engagement entity to coordinate NPC positioning.

**Related:**
- [RFC-018: NPC Engagement Coordination](../01-rfc/018-npc-engagement-coordination.md) — Full problem statement and design
- [Engagement Coordination Design Doc](../00-spec/engagement-coordination.md) — Worked examples and tuning knobs
- [RFC-014: Spatial Difficulty System](../01-rfc/014-spatial-difficulty-system.md) — Engagement entity origin
- [ADR-012: AI TargetLock Behavior Tree Integration](012-ai-targetlock-behavior-tree-integration.md) — NPC AI
- [ADR-024: Per-Archetype Positioning Strategy](024-per-archetype-positioning-strategy.md) — How strategies choose hexes

## Decision

The engagement entity maintains a **hex assignment map** that assigns each child NPC a unique approach hex. NPCs path to their assigned hex rather than directly to the player. Assignments are recalculated when the player changes tiles.

### Core Mechanism

**Hex Assignment Component:**

The engagement entity gains a component tracking assigned hexes:

```rust
struct HexAssignment {
    assignments: HashMap<Entity, Qrz>,  // NPC → assigned hex
    target_player: Entity,               // player being engaged
}
```

**Assignment Algorithm:**

```
on engagement_create OR player_tile_change OR npc_death:
  1. player_tile = player.position.tile
  2. adjacent = qrz::neighbors(player_tile)  // 6 hexes
  3. available = adjacent.filter(|h| is_passable(h) && !is_occupied_by_non_engagement(h))
  4. melee_npcs = engagement.children.filter(|e| archetype.is_melee())
  5. ranged_npcs = engagement.children.filter(|e| archetype.is_ranged())
  6. strategy = determine_strategy(melee_npcs)  // see ADR-024
  7. primary_assignments = strategy.assign(melee_npcs, available)
  8. excess_npcs = melee_npcs[available.len()..]
  9. secondary_assignments = assign_secondary(excess_npcs, player_tile)
  10. ranged_assignments = assign_ranged(ranged_npcs, player_tile)  // orbital/perimeter
  11. update HexAssignment component
```

**NPC Pathfinding Integration:**

NPCs already use Chase AI that paths toward a target location. The change:
- **Before:** NPC paths to `player.position.tile` (all NPCs target same hex)
- **After:** NPC paths to `hex_assignment.get(npc_entity)` (each NPC targets unique hex)

When an NPC reaches its assigned hex AND is adjacent to the player, it can attack (subject to recovery timer — see attack-recovery loop in design doc).

**Reassignment Triggers:**

| Trigger | Action |
|---------|--------|
| Engagement created | Initial hex assignment |
| Player tile changes | Full reassignment based on new position |
| NPC killed | Freed hex available; secondary NPC advances |
| NPC enters recovery | No reassignment (NPC stays in position) |

**Secondary Positions:**

When more melee NPCs exist than available adjacent hexes:
- Excess NPCs assigned to hexes 2 tiles from the player, aligned with their preferred face
- When an adjacent hex opens (NPC dies, face becomes available), the closest secondary NPC advances to claim it
- Secondary NPCs cannot attack (not adjacent to player)

**Key Properties:**
- Each melee NPC gets a unique adjacent hex (no competition/clumping)
- Pathing distance to assigned hex creates organic arrival stagger
- Reassignment on tile change lets player manipulate NPC paths through movement
- Maximum 6 simultaneous melee engagers (hex geometry limit)
- Graceful degradation when terrain blocks adjacent hexes

## Rationale

**Why engagement entity (not individual NPC AI):**
- Individual NPCs can't coordinate without shared state — they'd all pick the same optimal hex
- The engagement entity already exists as their parent (SOW-014 infrastructure)
- Centralized assignment prevents conflicts and enables strategy patterns
- Clean separation: engagement decides WHERE NPCs go, individual AI decides HOW they get there

**Why reassign on tile change (not per frame):**
- Tile changes are discrete events (player crosses hex boundary)
- Sub-tile movement doesn't change which hexes are adjacent
- Prevents reassignment thrashing during smooth movement
- Server FixedUpdate (125ms) + tile-change trigger = natural throttle

**Why secondary positions at 2 hexes (not further):**
- 2 hexes = one move from adjacent. NPC can advance quickly when hex opens
- Further back creates long approach times (feels like NPCs aren't engaging)
- Closer than 2 would create collisions with adjacent NPCs

**Why filter non-engagement occupants:**
- Other players or engagement groups shouldn't have their hexes claimed
- Engagement-internal NPCs are tracked by the assignment map (not filtered)
- Environmental blocking (terrain, obstacles) handled by passability check

## Consequences

**Positive:**
- NPCs arrive at different times (natural stagger from different path lengths)
- No hex competition or clumping (each NPC has unique assignment)
- Player movement meaningfully affects NPC behavior (reassignment on tile change)
- Maximum melee pressure capped at 6 (hex geometry) with secondary queue for excess
- Engagement entity gains meaningful coordination role (not just a container)
- Clean extension point for per-archetype strategies (ADR-024)

**Negative:**
- Adds state to engagement entity (HexAssignment component)
- Reassignment computation on every player tile change (amortized cost)
- NPCs may appear to "know" optimal positions (coordinated behavior looks intelligent)
- Edge cases when terrain severely limits available hexes (0–2 faces)

**Mitigations:**
- HexAssignment is lightweight (~50 bytes for 6 NPCs, one HashMap)
- Reassignment is O(n) where n ≤ 6 NPCs per engagement — negligible
- Coordinated behavior is the design intent (pack AI, not individual AI)
- 0-face edge case: all NPCs hold at secondary positions until player moves

## Implementation Notes

**Files Affected:**
- Engagement entity components — New `HexAssignment` component
- NPC AI system — Read assigned hex as pathfinding target (was: player tile)
- Engagement spawning — Initialize HexAssignment on engagement creation
- Player movement handling — Trigger reassignment on tile change event
- NPC death handling — Trigger reassignment when child NPC despawns

**Integration Points:**
- Uses `qrz::neighbors()` for adjacent hex calculation (existing qrz library)
- Uses terrain passability checks (existing Map resource)
- Uses NNTree for occupancy checks (existing spatial queries)
- Extends existing Chase/Kite AI target parameter (not a new AI system)

**System Ordering:**
- Hex assignment runs BEFORE NPC AI tick (NPCs need assignment before deciding where to path)
- Reassignment runs AFTER player position update (needs new tile to calculate adjacency)
- Both run on server only — client sees results via NPC position broadcasts

## References

- [RFC-018: NPC Engagement Coordination](../01-rfc/018-npc-engagement-coordination.md)
- [Engagement Coordination Design Doc](../00-spec/engagement-coordination.md)
- [RFC-014: Spatial Difficulty System](../01-rfc/014-spatial-difficulty-system.md)
- [ADR-012: AI TargetLock Behavior Tree Integration](012-ai-targetlock-behavior-tree-integration.md)
- [ADR-024: Per-Archetype Positioning Strategy](024-per-archetype-positioning-strategy.md)

## Date

2026-02-09
