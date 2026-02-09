# SOW-018: NPC Engagement Coordination

## Status

**Planned** - 2026-02-09

## References

- **RFC-018:** [NPC Engagement Coordination](../01-rfc/018-npc-engagement-coordination.md)
- **ADR-023:** [Coordinated Hex Assignment](../02-adr/023-coordinated-hex-assignment.md)
- **ADR-024:** [Per-Archetype Positioning Strategy](../02-adr/024-per-archetype-positioning-strategy.md)
- **Spec:** [Engagement Coordination Design Doc](../00-spec/engagement-coordination.md)
- **Branch:** (proposed)
- **Implementation Time:** 13–19 hours

---

## Implementation Plan

### Phase 1: Attack-Recovery Loops

**Goal:** Add per-NPC recovery timers that gate attack initiation, creating natural gaps between threats from each NPC

**Deliverables:**
- Recovery timer component on NPC entities
- Per-archetype recovery duration constants (base and variance)
- Behavior tree integration: NPC checks recovery state before attacking
- Randomized recovery duration within archetype range on each attack completion
- Updated NPC attack flow: attack → set recovery timer → wait → ready

**Architectural Constraints:**
- Recovery timer is NPC-side state (not player-side — independent of GlobalRecovery from ADR-017)
- Recovery duration randomized uniformly within archetype range on each attack
- Berserker: 1.0–2.0s, Juggernaut: 3.0–5.0s, Defender: 4.0–6.0s
- Kiter has no explicit recovery timer (flee/reposition phase serves as implicit recovery)
- NPC cannot initiate new attacks while recovery timer > 0
- Recovery timer ticks down in server FixedUpdate (125ms timestep)
- Recovery timer persists through NPC repositioning (moving doesn't reset recovery)
- Constants isolated as named `const` values per archetype

**Success Criteria:**
- Single Berserker attacks, then pauses 1.0–2.0s before next attack
- Single Juggernaut attacks, then pauses 3.0–5.0s before next attack
- Single Defender attacks, then pauses 4.0–6.0s before next attack
- Kiter attack cadence unchanged (implicit recovery from flee phase)
- Recovery durations vary between attacks (randomized, not fixed)
- Two identical-archetype NPCs that attack simultaneously desync within 2–3 cycles
- Recovery timer does not interfere with GlobalRecovery (player lockout) — independent systems
- `cargo test` passes

**Duration:** 3–4 hours

**Dependencies:** None (standalone)

---

### Phase 2: Hex Assignment Infrastructure

**Goal:** Extend the engagement entity with a hex assignment map that assigns each child NPC a unique approach hex, and integrate with NPC pathfinding

**Deliverables:**
- `HexAssignment` component on engagement entities
- Assignment algorithm: calculate player-adjacent hexes, filter by passability, assign unique hex per melee NPC
- NPC pathfinding target change: path to assigned hex instead of player tile
- Reassignment trigger on player tile change
- Reassignment trigger on NPC death (freed hex)
- Secondary position assignment for excess NPCs (2 hexes from player)
- Secondary NPC advancement when adjacent hex opens

**Architectural Constraints:**
- `HexAssignment` stores `HashMap<Entity, Qrz>` mapping NPC → assigned hex
- Assignment recalculated on: engagement creation, player tile change, NPC death
- Available hexes = adjacent hexes that are passable AND not occupied by non-engagement entities
- Each melee NPC gets a unique adjacent hex (no duplicates)
- Maximum 6 melee NPCs on adjacent hexes (hex geometry limit)
- Excess NPCs assigned secondary positions at 2 hex distance, aligned with preferred face
- NPC pathfinding target changes from `player.position.tile` to `hex_assignment[npc_entity]`
- NPC can attack only when on its assigned hex AND adjacent to player
- Reassignment triggers only on tile change (not sub-tile movement)
- Server-authoritative — client sees results via NPC position updates (no new network messages)

**Success Criteria:**
- Engagement creation assigns unique hex per melee NPC
- No two melee NPCs share the same assigned hex
- Player tile change triggers hex reassignment for all engagement NPCs
- NPC death frees its hex — next recalculation can assign it to another NPC
- NPCs path to assigned hex, not directly to player tile
- NPC on assigned adjacent hex can attack; NPC not yet on assigned hex cannot
- With 7+ melee NPCs, excess holds at secondary position (2 hexes out)
- Secondary NPC advances to freed adjacent hex when one opens
- Terrain blocking reduces available adjacent hexes (fewer assignment options)
- `cargo test` passes

**Duration:** 4–6 hours

**Dependencies:** None (standalone, but Phase 1 recovery timer complements this)

---

### Phase 3: Per-Archetype Positioning Strategies

**Goal:** Implement the four positioning strategies (surround/cluster/perimeter/orbital) so that each archetype creates a distinct spatial pattern around the player

**Deliverables:**
- `PositioningStrategy` enum (Surround, Cluster, Perimeter, Orbital)
- Archetype → strategy mapping (Berserker → Cluster, Juggernaut → Surround, Defender → Perimeter, Kiter → Orbital)
- Surround strategy: maximize angular spread across available adjacent hexes
- Cluster strategy: minimize angular spread, prefer adjacent faces on same side
- Perimeter strategy: assign hexes at 2–3 hex range, not adjacent
- Orbital strategy: assign hexes at 3–6 hex range with line of sight
- Mixed-archetype composition resolution (Cluster first, then Surround fills remaining)
- Angular distance calculation on hex ring (0–5 index, wrap at 6)

**Architectural Constraints:**
- Strategy determines hex preference ordering, not hex availability (availability determined by Phase 2 infrastructure)
- Surround: preference = maximize `min(angular_distance(candidate, already_assigned))` for each candidate
- Cluster: preference = minimize angular distance to already-assigned faces; cap at 3 adjacent faces
- Perimeter: candidate hexes at distance 2–3 from player; spread evenly; do NOT compete for adjacent hexes
- Orbital: candidate hexes at distance 3–6 from player; maintain line of sight; do NOT compete for adjacent hexes
- In mixed engagements: Cluster assigned first → Surround fills remaining adjacent → Perimeter and Orbital assigned independently at range
- Strategy enum variant per archetype (not configurable per NPC at MVP)
- Angular distance on hex ring: `min(|a - b|, 6 - |a - b|)` where a, b are 0-indexed neighbor positions

**Success Criteria:**
- 3 Juggernauts (Surround) assigned to faces ~120° apart
- 3 Berserkers (Cluster) assigned to 3 adjacent faces on same side
- 2 Defenders (Perimeter) assigned to hexes at 2–3 range, not adjacent to player
- 2 Kiters (Orbital) assigned to hexes at 3–6 range with line of sight
- Mixed group (1 Juggernaut + 2 Berserkers): Berserkers cluster on 2 adjacent faces, Juggernaut takes face opposite cluster
- Mixed group (2 Juggernauts + 1 Kiter): Juggernauts spread 180° apart, Kiter orbits at range
- Defenders and Kiters never occupy adjacent hexes (different zone)
- Strategy assignment visible in NPC movement patterns (testable via NPC target positions)
- `cargo test` passes

**Duration:** 3–5 hours

**Dependencies:** Phase 2 (hex assignment infrastructure must exist before strategies can select hexes)

---

### Phase 4: Integration and Polish

**Goal:** Wire all systems together, verify end-to-end behavior with multi-NPC engagements, and tune initial parameter values

**Deliverables:**
- End-to-end integration: recovery + hex assignment + strategies working together
- Player movement response: repositioning visibly changes NPC approach timing
- Terrain interaction: reduced available faces when player is near impassable terrain
- Edge case handling: 0 available faces, 1 NPC engagement, 6+ NPC engagement
- Initial parameter tuning pass based on playtesting
- Integration tests for multi-NPC scenarios

**Architectural Constraints:**
- Recovery timer AND hex assignment both gate NPC attacks (NPC must be on assigned hex AND recovery complete)
- Reassignment must not reset recovery timers (repositioning doesn't grant free attacks)
- Player tile change triggers reassignment but does not disrupt in-progress NPC attacks
- 0 available faces: all NPCs hold at secondary positions (player surrounded by terrain)
- 1 NPC engagement: no coordination needed, NPC paths to nearest adjacent hex
- Secondary position advancement respects strategy (advancing NPC takes the face that best fits its strategy)
- No new network messages required (NPC position changes visible via existing MovementIntent broadcasts)

**Success Criteria:**
- 3 Juggernauts arrive at different times (staggered by path length) and attack with recovery gaps
- 3 Berserkers cluster from one direction; player stepping away forces re-close from same direction
- Mixed group creates compound positional puzzle (per worked examples in design doc)
- Player backing against cliff reduces melee engagement to available faces (not all 6)
- Engagement with 7+ melee NPCs: 6 on adjacent hexes, excess at secondary positions
- NPC killed → secondary NPC advances to freed hex within 1–2 server ticks
- Player repositioning visibly changes when the next NPC threat arrives
- No crash or panic on edge cases (0 faces, solo NPC, terrain-locked player)
- `cargo test` passes
- Manual playtest confirms "threats arrive staggered, not simultaneously"

**Duration:** 3–4 hours

**Dependencies:** Phase 1, Phase 2, Phase 3 (all systems must exist for integration)

---

## Acceptance Criteria

**Functional:**
- Every NPC follows attack → recovery → attack loop with per-archetype timing
- Engagement entity assigns unique approach hex per melee NPC
- NPCs path to assigned hex before attacking
- Per-archetype strategies produce correct spatial patterns (surround/cluster/perimeter/orbital)
- Mixed-archetype groups compose strategies without conflict
- Player tile change triggers hex reassignment for all engagement NPCs
- Excess NPCs hold secondary positions and advance when hexes open
- Recovery timer independent of player GlobalRecovery (no interaction)

**UX:**
- Threats arrive staggered (never all simultaneously)
- Recovery gaps perceptible between attacks from each NPC
- Player positioning visibly changes NPC approach timing
- Each archetype creates a distinct spatial pattern recognizable in gameplay
- Terrain exploitation reduces melee pressure (fewer available faces)
- No regressions in existing single-NPC combat or Kiter behavior

**Performance:**
- Recovery timer: one `f32` tick per NPC per FixedUpdate — negligible
- Hex assignment: O(n) where n ≤ 6 per engagement, runs only on tile change — negligible
- Strategy computation: O(n²) where n ≤ 6 per engagement — negligible
- No per-frame overhead added by any system
- Memory: ~100 bytes per engagement (HexAssignment + strategy metadata)

**Code Quality:**
- Per-archetype recovery constants named and isolated (not inline magic numbers)
- Strategy enum with clear mapping to archetypes
- Hex assignment and strategy functions are testable in isolation (pure logic)
- Integration tests covering: single-archetype groups, mixed-archetype groups, terrain constraints, secondary positions
- Worked example values from design doc verified in tests
- Existing NPC tests unaffected (single-NPC behavior unchanged)

---

## Discussion

*This section will be populated during implementation with questions, decisions, and deviations.*

### Implementation Note: Phase Dependencies

Phases 1 and 2 are architecturally independent and can be implemented in any order or in parallel. Phase 3 depends on Phase 2 (strategies need the hex assignment infrastructure to assign hexes into). Phase 4 requires all three prior phases.

Recommended order: Phase 1 + Phase 2 (parallel) → Phase 3 → Phase 4.

### Implementation Note: Relationship to SOW-017

SOW-017 (Combat Balance) and SOW-018 (Engagement Coordination) are fully independent. They solve complementary problems — SOW-017 handles power scaling, SOW-018 handles action economy. They can be implemented in either order. Together they make multi-NPC encounters balanced across both level and count.

---

## Acceptance Review

*This section will be populated after implementation is complete.*

---

## Sign-Off

**Reviewed By:** —
**Date:** —
**Decision:** —
