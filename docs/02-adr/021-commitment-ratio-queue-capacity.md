# ADR-021: Commitment-Ratio Queue Capacity

## Status

Accepted

## Context

The current queue capacity formula scales with raw Focus attribute points: more Focus points → more queue slots. This creates two problems as levels increase:

1. **Flat investment value:** A level-3 entity with 2 Focus points gets the same queue capacity as a level-10 entity with 2 Focus points. The investment doesn't scale with progression.

2. **Unbounded at high levels:** A level-10 entity can invest 7+ points in Focus, getting excessive queue capacity that trivializes reaction queue management.

The queue capacity should reflect **how much of their build an entity commits to Focus**, not the raw point count. A character who puts 50% of their points into Focus is a Focus specialist regardless of level.

**References:**
- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md) — System 2
- [Combat Balance Design Doc](../00-spec/combat-balance.md) — Full formula reference
- [ADR-006: Server-Authoritative Reaction Queue](006-server-authoritative-reaction-queue.md) — Queue architecture
- [Attribute System Spec](../00-spec/attribute-system.md) — Attribute investment model

## Decision

Replace raw-Focus queue capacity scaling with a **commitment ratio** formula using threshold-based slot allocation.

### Core Mechanism

**Formula:**

```rust
fn calculate_queue_capacity(focus_reach: f32, total_level: u8) -> u8 {
    if total_level == 0 {
        return 1; // Level 0 always gets 1 slot
    }
    let ratio = focus_reach / (total_level as f32 * 7.0);
    match ratio {
        r if r >= 0.66 => 4,
        r if r >= 0.50 => 3,
        r if r >= 0.33 => 2,
        _ => 1,
    }
}
```

Where:
- `focus_reach` = total Focus attribute investment
- `total_level` = entity's current level
- `7.0` = number of attribute axes (normalizing constant)

**Threshold Table:**

| Commitment Ratio | Queue Slots | Build Archetype |
|------------------|-------------|-----------------|
| < 33% | 1 | Non-Focus build (melee, damage, etc.) |
| 33% – 49% | 2 | Secondary Focus investment |
| 50% – 65% | 3 | Primary Focus investment |
| ≥ 66% | 4 | Focus specialist |

## Rationale

**Why ratio-based, not raw:**
- Raw points lose relative meaning as levels increase (2 Focus at level 3 is significant; 2 Focus at level 10 is trivial)
- Ratio captures build identity: "what fraction of my power goes to Focus?"
- Ratio naturally adjusts as entities level up without formula changes

**Why threshold-based, not continuous:**
- Queue slots are discrete (can't have 2.7 slots)
- Clear breakpoints give players concrete goals ("I need 33% Focus for 2 slots")
- Simpler to reason about than continuous formulas
- Matches existing UI (discrete slot count display)

**Why 7 as the normalizing constant:**
- Represents the number of attribute axes in the system
- Each level distributes 1 point across any axis
- `total_level × 7` approximates the theoretical maximum pool

**Why level-0 special case:**
- Division by zero protection
- Level-0 entities have no investment to measure
- 1 slot is reasonable for level-0 entities

## Consequences

**Positive:**
- Build identity matters for queue capacity (Focus specialist vs generalist)
- Investment scales naturally with level (no rebalancing needed)
- Clear player-facing thresholds for decision-making
- Prevents queue capacity inflation at high levels

**Negative:**
- Existing Focus-based capacity formula must be replaced (not additive)
- Threshold cliffs: going from 32% to 33% doubles capacity
- Low-level entities may get fewer slots than current system (rebalance existing content)

**Mitigations:**
- Threshold values are named constants, easily tuned
- Cliff effects are intentional design (clear breakpoints are player-friendly)
- NPC archetypes from RFC-014 can be tuned to hit desired thresholds

## Implementation Notes

**Files Affected:**
- `src/common/systems/combat/queue.rs` — Replace `calculate_queue_capacity` function
- Test files — Update capacity expectations for ratio-based formula

**Integration Points:**
- Same call site as current capacity calculation
- Input changes from `focus_points: i32` to `focus_reach: f32, total_level: u8`
- Output unchanged: `u8` slot count

**Edge Cases:**
- Level 0: always 1 slot (special case)
- Focus 0 at any level: 0/N = 0% → 1 slot
- All points in Focus: e.g., level 5 with Focus 5 → 5/(5×7) = 14.3% → 1 slot (only full specialists hit higher thresholds)

## References

- [RFC-017: Combat Balance Overhaul](../01-rfc/017-combat-balance-overhaul.md)
- [Combat Balance Design Doc](../00-spec/combat-balance.md)
- [ADR-006: Server-Authoritative Reaction Queue](006-server-authoritative-reaction-queue.md)
- [Attribute System Spec](../00-spec/attribute-system.md)

## Date

2026-02-09
