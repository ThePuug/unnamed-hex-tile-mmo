# RFC-018: NPC Engagement Coordination

## Status

**Draft** - 2026-02-09

## Feature Request

### Player Need

From player perspective: **Multi-enemy combat should feel like a tactical puzzle with attackable gaps, not an unavoidable damage stream** - Currently, 2–3 melee NPCs standing in range and attacking on cooldown produce a constant stream of threats with no gaps, leading to queue overload regardless of player skill.

**Current Problem:**
Without engagement coordination:
- Multiple melee NPCs all attack on cooldown simultaneously, producing constant queue pressure with zero gaps
- Player reaction queue fills faster than it can be processed — damage is unavoidable, not a skill issue
- Only the Kiter archetype has natural downtime (flee phase); Berserker, Juggernaut, and Defender have none
- NPCs compete for the same hex adjacent to the player, causing clumping and simultaneous arrival
- Player positioning has no effect on encounter pacing — moving doesn't change when threats arrive
- Encounters feel like "stand and absorb damage" rather than "read the situation and act"

**We need a system that:**
- Creates natural gaps between threats from any single NPC (attack-recovery loops)
- Staggers NPC arrival through coordinated hex positioning (pathing distance creates time gaps)
- Gives each archetype a distinct positioning strategy that extends its combat identity
- Rewards player positioning (moving changes NPC pathing, reassigns hexes, resets stagger timing)
- Scales gracefully from 1 NPC (trivial) to 6 NPCs (maximum hex engagement)

### Desired Experience

Players should experience:
- **Readable combat:** Threats arrive one at a time initially, building to manageable pairs/triples as NPCs close in. Never all at once.
- **Breathing room:** Recovery gaps between attacks from each NPC give time to react, dismiss, or reposition between threats.
- **Positional agency:** Moving toward or away from NPCs visibly changes when the next threat arrives. Backing into terrain trades escape routes for fewer approach angles.
- **Archetype identity:** Berserkers feel like a charging pack (cluster from one direction). Juggernauts feel like a closing noose (surround from all sides). Defenders dare you to attack first. Kiters harass from range.
- **Tactical depth:** Mixed-archetype groups create compound puzzles where player must prioritize targets and positions based on the composition.

### Specification Requirements

**Attack-Recovery Loops:**
- Every NPC follows attack → recovery → attack cycle
- Recovery duration varies by archetype (Berserker: 1–2s, Juggernaut: 3–5s, Defender: 4–6s)
- Recovery has randomized variance within range to prevent re-synchronization
- NPCs do not generate threats during recovery phase
- Kiter uses flee/reposition phase as implicit recovery (already implemented)

**Hex-Based Coordinated Positioning:**
- Engagement entity (existing parent from ADR-014) assigns each NPC an approach hex
- Melee NPCs assigned unique adjacent hexes (max 6 faces, one per NPC)
- NPCs must path to assigned hex before attacking — pathing distance creates natural stagger
- Hex assignments reassigned when player tile changes (not sub-tile movement)
- Excess NPCs (beyond available faces) hold at secondary positions (2 hexes out)

**Per-Archetype Positioning Strategy:**
- Juggernaut: Surround — even distribution across available faces (120° for 3, 90° for 4)
- Berserker: Cluster — adjacent faces on same side (2–3 faces max)
- Defender: Loose perimeter — hold at 2–3 hex range, don't compete for melee faces
- Kiter: Orbital — maintain 3–6 hex range, orbit player position
- Mixed groups combine strategies (Juggernauts surround while Kiters orbit)

### MVP Scope

**Phase 1 includes:**
- Attack-recovery loop with per-archetype recovery durations
- Engagement-coordinated hex assignment for melee NPCs
- Per-archetype positioning strategy (surround/cluster/perimeter/orbital)
- Hex reassignment on player tile change
- Secondary positioning for excess NPCs

**Phase 1 excludes:**
- Data-driven recovery tuning (constants hardcoded for MVP)
- Dynamic strategy adaptation (engagement doesn't change strategy mid-fight)
- Formation commands or player-directed NPC positioning
- Cross-engagement coordination (two engagement groups don't coordinate with each other)
- Kiter orbital path planning (reuse existing flee AI)

### Priority Justification

**HIGH PRIORITY** - Multi-NPC combat is fundamentally broken without pacing; all encounter testing is invalid until threats arrive staggered.

**Why high priority:**
- Queue overload from simultaneous attacks makes multi-NPC combat feel like unavoidable damage, undermining the reaction queue design (ADR-003/006)
- Spatial difficulty system (RFC-014) spawns multi-NPC engagements but they play identically regardless of composition
- Combat balance overhaul (RFC-017) solves the power scaling problem; this solves the complementary action economy problem
- Archetype positioning strategies are the primary differentiator between directional encounters — without them, North/East/South/West feel the same
- Player positioning agency is the core hex grid advantage over non-spatial combat systems

**Benefits:**
- Multi-NPC combat becomes tractable (stagger + gaps = manageable queue pressure)
- Each archetype has distinct spatial identity (surround vs cluster vs perimeter vs orbital)
- Player positioning decisions matter (move to repath NPCs, use terrain, exploit gaps)
- Foundation for difficulty tuning (recovery duration is the primary knob)
- Engagement entity gains meaningful coordination role (not just a parent container)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Two-System Engagement Coordination**

#### Core Mechanism

**System 1 — Attack-Recovery Loops:**

Each NPC tracks a recovery timer. After executing an attack, the NPC enters recovery for a randomized duration within its archetype's range. During recovery, the NPC does not initiate new attacks.

```
State machine per NPC:
  READY → (attack) → RECOVERING → (timer expires) → READY

Recovery duration = uniform_random(archetype_min, archetype_max)
```

| Archetype | Min (s) | Max (s) | Avg threats/min (solo) |
|-----------|---------|---------|----------------------|
| Berserker | 1.0 | 2.0 | ~40 |
| Juggernaut | 3.0 | 5.0 | ~15 |
| Defender | 4.0 | 6.0 | ~12 |

**System 2 — Hex-Based Coordinated Positioning:**

The engagement entity maintains a hex assignment map: `HashMap<Entity, Qrz>` mapping each child NPC to its assigned approach hex. Assignments are recalculated on:
- Engagement creation (initial assignment)
- Player tile change (reassignment)
- NPC death (freed hex reassigned)

```
Assignment algorithm (per recalculation):
  1. Get player tile and 6 adjacent hexes
  2. Filter available hexes (passable terrain, not blocked)
  3. Apply archetype strategy to determine ideal hex per NPC
  4. Assign: closest available hex matching strategy preference
  5. Excess NPCs → secondary positions (2 hexes out, preferred face)
```

**Archetype Strategy Integration:**

Each strategy produces a preferred hex ordering:
- Surround: maximize angular spread from already-assigned hexes
- Cluster: minimize angular spread (prefer adjacent to already-assigned)
- Perimeter: prefer hexes at 2–3 range (don't enter adjacent)
- Orbital: prefer hexes at 3–6 range (maintain distance)

#### Performance Projections

- **Recovery timer:** One `f32` per NPC, ticked per FixedUpdate — negligible
- **Hex assignment:** 6 adjacent hex lookups + strategy sort per recalculation. Runs only on player tile change (not per frame). For 6 NPCs: ~50 operations per trigger — negligible
- **Path recalculation:** NPCs already have pathfinding (Chase/Kite AI). Changing target hex triggers existing pathfinding — no new pathfinding code needed
- **Memory:** One `HashMap<Entity, Qrz>` per engagement (~50 bytes for 6 NPCs)

**Development Time:**
- Phase 1 (Attack-recovery loops): 3–4 hours
- Phase 2 (Hex assignment infrastructure): 4–6 hours
- Phase 3 (Per-archetype strategies): 3–5 hours
- Phase 4 (Integration and reassignment): 3–4 hours
- **Total: 13–19 hours**

#### Technical Risks

**1. Pathfinding Integration**
- *Risk:* Existing Chase/Kite AI may not accept arbitrary target hexes cleanly
- *Mitigation:* AI already paths to a target location. Change target from "player hex" to "assigned hex". Minimal integration surface.
- *Impact:* Low — if pathfinding can target any hex (it can), this is a parameter change

**2. Reassignment Thrashing**
- *Risk:* Player moving rapidly causes constant hex reassignment and NPC path changes
- *Mitigation:* Reassignment triggers only on tile change (not sub-tile). Server tick rate (125ms) naturally throttles. Can add minimum reassignment interval if needed.
- *Impact:* Low — tile-change-only trigger is coarse enough

**3. Recovery Timer vs Lockout Interaction**
- *Risk:* Recovery timer may conflict with existing GlobalRecovery lockout from abilities
- *Mitigation:* Recovery timer is a separate NPC-side concept (when the NPC chooses to attack next). GlobalRecovery is a player-side concept (when the player can act next). No conflict — they operate on different entities.
- *Impact:* None — independent systems

**4. Hex Availability in Complex Terrain**
- *Risk:* Some player positions may have very few available adjacent hexes (surrounded by cliffs)
- *Mitigation:* Strategy degrades gracefully — if only 2 faces available, surround uses both. If 0 faces available, NPCs hold secondary positions. Edge case, not blocker.
- *Impact:* Low — graceful degradation is built into the algorithm

### System Integration

**Affected Systems:**
- `src/server/systems/ai.rs` (or equivalent) — Recovery timer, attack gating
- `src/common/systems/behaviour/` — NPC behavior state machine
- Engagement entity components — Hex assignment map, strategy type
- Existing Chase/Kite AI — Target hex parameter change
- `src/server/systems/combat.rs` — NPC attack initiation (gated by recovery state)

**Compatibility:**
- ✅ Engagement entity already exists as NPC parent (ADR-014 / SOW-014)
- ✅ Archetype already tracked per NPC (RFC-014 implemented)
- ✅ Chase/Kite AI already paths to target locations (parameter change only)
- ✅ Hex adjacency utilities already exist in qrz library
- ✅ NNTree (spatial queries) available for finding adjacent entities
- ✅ Recovery timer is independent of GlobalRecovery (ADR-017) — no conflict
- ✅ Reaction queue pacing (ADR-003/006) benefits directly from staggered threats

### Alternatives Considered

#### Alternative 1: Fixed Delay Between NPC Attacks (Global Cooldown Per Engagement)

Engagement tracks a global "last attack" timestamp. NPCs can only attack if engagement cooldown has elapsed.

**Rejected because:**
- Creates perfectly periodic threats (metronomic, predictable after first cycle)
- Doesn't create positional variety (NPCs attack in order regardless of position)
- Doesn't scale with NPC count (3 NPCs have same cadence as 6)
- Loses per-archetype pacing (Berserker and Juggernaut have same attack rate)

#### Alternative 2: Queue-Aware NPC Attack Suppression

NPCs check player queue occupancy before attacking. If queue is near capacity, they wait.

**Rejected because:**
- NPCs shouldn't have omniscient knowledge of player queue state
- Feels artificial ("why did this NPC stop attacking when I'm full?")
- Removes pressure variation (queue is always near-full, never overloaded or empty)
- Doesn't create positional gameplay (no reason to reposition)

#### Alternative 3: Random Attack Probability Per Tick

Each NPC has a per-tick probability of attacking. Lower probability = fewer simultaneous attacks.

**Rejected because:**
- Unpredictable for both player and designers (hard to tune)
- No spatial component (NPCs attack from wherever they happen to be)
- Can produce unlucky bursts (all NPCs roll "attack" on same tick)
- Doesn't create per-archetype identity

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** The two systems address different timescales of the same problem. Attack-recovery loops control **per-NPC threat cadence** (how often one NPC attacks). Hex positioning controls **cross-NPC synchronization** (whether NPCs attack at the same time). Both are needed — recovery alone still allows simultaneous first attacks; positioning alone still allows rapid re-attacks.

**Extensibility:**
- Recovery duration can become level-dependent (higher-level NPCs attack faster/slower)
- Hex assignment strategy can be extended with new archetypes without changing infrastructure
- Engagement coordination logic is a clean extension point for future group AI behaviors (formations, flanking, retreat)
- Cross-engagement coordination could be added later (multiple engagement groups don't overlap hexes)
- Recovery duration could scale with combat balance (RFC-017 level multiplier could affect NPC attack pacing)

**Interaction with existing systems:**
- Recovery timer adds state to NPC behavior tree (new node: `WaitForRecovery`)
- Hex assignment extends engagement entity (new component: `HexAssignment`)
- Both systems are server-authoritative — client sees results via position updates
- No new network messages needed — NPCs already broadcast position via MovementIntent

**Relationship to RFC-017 (Combat Balance Overhaul):**
- RFC-017 solves the **power scaling problem** (stats vs threat count)
- RFC-018 solves the **action economy problem** (threats per second vs reaction capacity)
- Together they make multi-NPC combat balanced across both dimensions
- Can be implemented independently in either order

### PLAYER Validation

**From player perspective:**

**Retained Concepts:**
- ✅ Combat should be "Conscious but Decisive" (spec philosophy) — stagger gives time to be conscious
- ✅ Player positioning should matter (hex grid design intent) — repositioning changes NPC behavior
- ✅ Each archetype should feel distinct (RFC-014 design) — positioning strategies deliver this
- ✅ Reaction queue should be manageable by skilled players (ADR-003/006) — stagger enables this

**Success Criteria:**
- 3 Juggernauts arrive at different times (not simultaneously)
- 3 Berserkers cluster from one direction with clear escape on opposite side
- Moving toward an NPC visibly changes when other NPCs arrive
- Recovery gaps are perceptible (player notices "safe" moments between attacks)
- Mixed-archetype groups feel distinct from same-archetype groups
- Terrain reduces approach angles (backing against wall limits melee engagement)

---

## Approval

**Status:** Draft

**Approvers:**
- ARCHITECT: ⏳ Pending
- PLAYER: ⏳ Pending

**Scope Constraint:** Fits in one SOW (13–19 hours for 4 phases)

**Dependencies:**
- RFC-014: Spatial difficulty system (implemented — provides archetypes and engagement entity)
- ADR-012: AI TargetLock behavior tree integration (implemented — NPC AI infrastructure)
- ADR-003/006: Reaction queue (implemented — queue this system paces threats into)
- ADR-017: Universal lockout (implemented — NPC recovery is independent of lockout)

**Next Steps:**
1. Create design doc (`docs/00-spec/engagement-coordination.md`) with full worked examples ✅
2. Create ADR-023 (coordinated hex assignment), ADR-024 (per-archetype positioning strategy)
3. Create SOW-018 with phased implementation plan

**Date:** 2026-02-09
