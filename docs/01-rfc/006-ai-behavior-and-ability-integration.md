# RFC-006: AI Behavior and Ability Integration

## Status

**Approved** - 2025-10-30

## Feature Request

### Player Need

From the combat system spec: **"Wild Dog attack pattern"** - NPCs must attack players to create combat pressure.

**Current Problem:**
Without AI ability usage:
- Wild Dogs pathfind to players but never attack
- No combat threats generated (reaction queue empty)
- Combat system untestable (no damage to react to)
- Player can't validate Dodge ability (no threats to clear)

**We need a system that:**
- NPCs use abilities (BasicAttack) when in range
- NPCs face their targets (heading-based targeting)
- NPCs maintain target commitment (no mid-chase abandonment)
- Attack cooldowns prevent spam (GCD enforcement)
- Sustained pressure fills reaction queue (enables testing)

### Desired Experience

Players should experience:
- **Sustained pressure:** Wild Dogs attack every ~1 second
- **Target commitment:** Dog chases you until you're dead or out of range
- **Predictable timing:** Attack speed consistent (enables reactive play)
- **Queue accumulation:** 2-3 threats build up if you don't react
- **Combat intensity:** Must use Dodge or take damage

### Specification Requirements

From `docs/00-spec/combat-system.md`:

**Wild Dog Attack Pattern:**
1. Detect player within aggro radius (20 hexes)
2. Face toward player, pathfind to adjacent hex
3. When adjacent and facing, attack every 2-3 seconds
4. Attack enters player's reaction queue
5. If player moves, turn to face and pursue

**MVP Requirements:**
- GCD component (cooldown tracking)
- TargetLock component (sticky targeting, prevents abandonment)
- Behavior tree nodes (FaceTarget, UseAbilityIfAdjacent, FindOrKeepTarget)
- Wild Dog attacks every ~1 second (creates queue pressure)

### MVP Scope

**Phase 1 includes:**
- GCD component for cooldown tracking
- TargetLock component for sticky target acquisition (MANDATORY)
- FindOrKeepTarget behavior node (maintains lock until invalid)
- FaceTarget behavior node (updates heading)
- UseAbilityIfAdjacent behavior node (emits abilities)
- Wild Dog behavior tree (complete attack pattern)
- Integration with ADR-009 directional targeting

**Phase 1 excludes:**
- Multiple abilities per NPC (BasicAttack only)
- Ranged enemies (melee only)
- Complex AI states (aggro tables, fleeing)
- Boss patterns (telegraphs, phases)

### Priority Justification

**CRITICAL** - This completes the combat loop. Without AI attacks, players cannot test reaction queue, Dodge ability, or damage mitigation. Blocks combat playtesting.

**Blocks:**
- Reaction queue validation (ADR-006/007/008)
- Dodge ability testing (ADR-006/007/008)
- Combat balance tuning

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Behavior Tree Nodes with GCD and TargetLock Components**

#### Core Mechanism

**Behavior Tree Integration:**
- New nodes: `FindOrKeepTarget`, `FaceTarget`, `UseAbilityIfAdjacent`
- Reuses existing bevy_behave system
- Per-NPC configurability (different templates)

**GCD Component:**
```rust
pub struct Gcd {
    pub gcd_type: Option<GcdType>,
    pub expires_at: Duration,
}
```
- Tracks cooldown state
- Validated at behavior node and server execution
- Prevents ability spam

**TargetLock Component:**
```rust
pub struct TargetLock {
    pub locked_target: Entity,
    pub max_chase_distance: u32,  // Leash (0 = infinite)
}
```
- Sticky target acquisition (persists until invalid)
- Prevents mid-chase abandonment
- Releases only on death/despawn/leash distance

**Benefits:**
- Fastest to implement (follows existing behavior patterns)
- Sufficient for simple "attack every ~1s" MVP
- Enables sustained pressure (validates reaction queue)
- Integrates with ADR-009 directional targeting

#### Performance Projections

**Behavior Tree Overhead:**
- 100 Wild Dogs attacking: < 10% CPU
- Behavior tree ticks: 60fps maintained

**GCD Queries:**
- 1000 actors checking GCD: < 1ms

**Network Bandwidth:**
- 10 Dogs attacking: ~500 bytes/sec (ability events)
- GCD not synced (server-authoritative)

#### Technical Risks

**1. Target Switching**
- *Risk:* Behavior tree failures cause target re-selection
- *Impact:* Dogs abandon chase mid-combat (no queue pressure)
- *Mitigation:* TargetLock component (MANDATORY for MVP)
- *Frequency:* Eliminated with sticky targeting

**2. Heading Update Timing**
- *Risk:* Pathfinding overrides heading, facing check fails
- *Impact:* UseAbilityIfAdjacent fails (no attack)
- *Mitigation:* FaceTarget runs twice (before and after pathfinding)
- *Frequency:* Rare with dual FaceTarget pattern

**3. Behavior Tree Complexity**
- *Risk:* 7-step sequence (high failure rate)
- *Impact:* Dogs don't complete attacks
- *Mitigation:* TargetLock prevents most failures
- *Target:* >80% sequence completion rate

### System Integration

**Affected Systems:**
- Spawner (add GCD, TargetLock to NPCs)
- Abilities (validate GCD before execution)
- Behavior tree (new nodes)
- Targeting (reuse ADR-009 directional targeting)

**Compatibility:**
- ✅ ADR-002: GCD infrastructure
- ✅ ADR-006/007/008: Reaction queue
- ✅ ADR-009: Directional targeting
- ✅ ADR-010: Damage pipeline

### Alternatives Considered

#### Alternative 1: Separate ECS System

System runs every frame, checks conditions, emits abilities.

**Rejected because:**
- Behavior split between tree (movement) and system (combat)
- More complex state management
- Slower to implement
- Behavior tree already manages Target/Dest state

#### Alternative 2: Hybrid Intent Component

Behavior tree sets `WantsToAttack` component, system executes.

**Rejected because:**
- Overkill for simple MVP
- Extra indirection (component + system)
- Behavior tree integration simpler

#### Alternative 3: Time-Based TargetLock Expiry

Lock expires after 10 seconds, can switch targets.

**Rejected because:**
- Adds unnecessary complexity (time tracking)
- Worse gameplay (arbitrary abandonment)
- Simpler: Persist until target invalid

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** TargetLock component is MANDATORY for functional combat. Without it, behavior tree failures cause target switching, breaking sustained pressure.

**Pattern Recognition:**
- Similar to player manual targeting (TAB cycling, from ADR-009)
- Applied to NPCs (sticky automatic targeting)
- Enables reliable combat testing

**Extensibility:**
Foundation enables future features:
- Ranged abilities (UseAbilityAtRange node)
- Boss patterns (complex behavior trees)
- Aggro tables (threat-based targeting)
- Flee behavior (low health condition)

### PLAYER Validation

**UX Requirements:**
- ✅ Dogs chase reliably (no mid-chase abandonment)
- ✅ Attack speed consistent (~1 second)
- ✅ Combat pressure (queue fills, must Dodge)
- ✅ Predictable timing (can learn attack pattern)

**Combat Feel:**
- Intense: Sustained attacks create urgency
- Fair: Predictable timing allows reaction
- Challenging: Must manage queue or take damage

**Acceptance Criteria:**
- ✅ Dog locks to player, ignores closer players
- ✅ 2 Dogs fill queue within 3 seconds
- ✅ Behavior tree success rate >80%
- ✅ Attack speed ~1 second (enables pressure)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- PLAYER: ✅ Enables combat testing, creates pressure
- ARCHITECT: ✅ Clean integration, reuses existing systems

**Scope Constraint:** Fits in one SOW (estimated 9-10 days across 7 phases)

**Dependencies:**
- ADR-002: GCD infrastructure
- ADR-006/007/008: Reaction queue
- ADR-009: Directional targeting
- ADR-010: Damage pipeline

**Next Steps:**
1. ARCHITECT creates ADR-011 (GCD component) and ADR-012 (AI behavior integration)
2. ARCHITECT creates SOW-006 with 7-phase implementation plan
3. DEVELOPER begins Phase 1 (GCD and TargetLock components)

**Date:** 2025-10-30
