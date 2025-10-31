# ADR-006: AI Behavior and Ability Integration

## Status

**Ready for Implementation** (Proposed → Accepted pending implementation)

## MVP Critical Path: Three Must-Have Components

**The following three components are MANDATORY for functional AI combat.** Without all three, dogs will behave unreliably and break combat pressure testing.

### 1. TargetLock Component (CRITICAL)
**Problem:** Behavior tree sequence failures cause `FindSomethingInterestingWithin` to re-select targets, breaking combat pressure.
**Solution:** Sticky target lock that persists until target becomes invalid (dead, despawned, or beyond leash distance).
**Impact:** Enables sustained pressure, allows reaction queue validation (ADR-003), prevents "dog randomly abandons chase" behavior.
**Specification:** See [Decision 4](#decision-4-targetlock-component-structure-prevents-target-switching) below.

### 2. GCD Component (CRITICAL)
**Problem:** No cooldown enforcement means NPCs could spam abilities if behavior tree retries quickly.
**Solution:** Global Cooldown (GCD) component tracks cooldown state, validated at both behavior node and server execution.
**Impact:** Stabilizes attack speed at ~1 second, prevents ability spam, enables predictable combat timing.
**Specification:** See [Decision 3](#decision-3-gcd-component-structure) below.

### 3. FindOrKeepTarget Behavior Node (CRITICAL)
**Problem:** `FindSomethingInterestingWithin` finds new target every call, no memory of previous target.
**Solution:** New `FindOrKeepTarget` node checks existing lock first, only searches for new target if lock invalid.
**Impact:** NPCs commit to targets until invalid, maintains target consistency across behavior tree loops.
**Specification:** See [Decision 4](#decision-4-targetlock-component-structure-prevents-target-switching) (includes both TargetLock component and FindOrKeepTarget node).

### Expected Outcome
**"Dogs reliably chase and attack me every ~1s. Combat feels intense. I'm dying because I don't know when to dodge, but at least the dogs are consistent."**

### Implementation Time
**3-5 days** for MVP critical path (Phases 1-6 core functionality).

---

## Context

### The Gap in Combat ADRs

ADRs 002-005 define the combat infrastructure but **do not specify how NPCs use abilities**:

- **ADR-002**: Resource pools (Health, Stamina, Mana), GCD enum exists
- **ADR-003**: Reaction Queue (threat insertion, timer management)
- **ADR-004**: Ability System (directional targeting, `execute_ability` validation)
- **ADR-005**: Damage Pipeline (damage calculation, threat resolution)

**Current NPC Behavior** (from [src/server/systems/spawner.rs:100-124](../../src/server/systems/spawner.rs)):
```rust
// Wild Dog behavior tree (simplified)
Behave::Forever => {
    Behave::Sequence => {
        FindSomethingInterestingWithin { dist: 20 },  // Find player
        Nearby { min: 1, max: 3, origin: Target },    // Move near (1-3 hexes)
        PathTo::default(),                             // Pathfind toward
        Behave::Wait(5.),                              // Wait 5 seconds
    }
}
```

**Problem:** Dog moves toward player but **never attacks**. No system emits `Try::UseAbility` for NPCs.

### Combat Spec Requirements

From `docs/spec/combat-system.md`:

**Wild Dog Attack Pattern:**
1. Detect player within aggro radius (10 hexes)
2. Face toward player, pathfind to adjacent hex
3. **When adjacent and facing player, attack every 2-3 seconds**
4. Attack enters player's reaction queue
5. If player moves away, turn to face and pursue

**Key Mechanic:** NPCs must **face their target** (heading updates) to use directional targeting from ADR-004.

### Architectural Decision: Behavior Tree Integration

**Options Considered:**

1. **Behavior Tree Nodes** ✅ **SELECTED**
   - Add `FaceTarget` and `UseAbilityIfAdjacent` behavior nodes
   - Integrates with existing bevy_behave system
   - Per-NPC configurability (different attack patterns per template)

2. **Separate ECS System**
   - `server/systems/ai/ability_usage.rs` runs every frame
   - Pros: Testable, ECS-native
   - Cons: Behavior split between tree (movement) and system (combat)

3. **Hybrid (Intent Component)**
   - Behavior tree sets `WantsToAttack` component, system executes
   - Pros: Clear separation
   - Cons: Overkill for MVP

**Rationale for Option 1:**
- Fastest to implement (follows existing `FindSomethingInterestingWithin`, `Nearby`, `PathTo` patterns)
- Sufficient for simple "attack every 2 seconds when adjacent" MVP
- Behavior tree already manages NPC state (Target, Dest)
- Post-MVP: Can migrate to separate system for complex AI (boss patterns, state machines)

### GCD Component Design

**Current State:** `GcdType` enum exists ([src/common/systems/combat/gcd.rs](../../src/common/systems/combat/gcd.rs)) but no `Gcd` component.

**Decision:** Create `Gcd` component to track cooldowns (mimics player GCD infrastructure from ADR-002).

```rust
#[derive(Component, Clone, Copy, Debug)]
pub struct Gcd {
    pub gcd_type: Option<GcdType>,
    pub expires_at: Duration,  // Time::elapsed() when GCD ends
}
```

**Why Component (not behavior tree state):**
- Players need GCD component (already implied by ADR-002)
- NPCs share same GCD validation logic (DRY principle)
- Server's `execute_ability` system checks GCD before processing abilities
- Unified cooldown tracking for all actors (players and NPCs)

---

## Decision

We will implement **AI ability usage via behavior tree nodes** with **GCD component** for cooldown tracking.

### Core Architectural Principles

#### 1. Behavior Tree Nodes for Combat Actions

**New Behavior Nodes:**

1. **`FaceTarget`** - Updates heading to face Target entity
2. **`UseAbilityIfAdjacent`** - Emits `Try::UseAbility` if conditions met

Both integrated into Wild Dog's behavior tree.

#### 2. GCD Component for Cooldown Tracking

**Shared Component:**
- Players and NPCs both have `Gcd` component
- Server's `execute_ability` system checks GCD before allowing ability usage
- Behavior nodes query GCD to respect cooldowns

#### 3. Integration with ADR-004 Directional Targeting

**NPCs use `select_target` function** (from ADR-004):
- Heading-based facing cone (60° arc)
- Automatic target selection (nearest hostile in direction)
- No tier lock or TAB cycling (NPCs use geometric default only)

---

### Detailed Design Decisions

#### Decision 1: FaceTarget Behavior Node

**Purpose:** Update NPC heading to face their Target entity.

**Component Definition:**
```rust
// src/server/systems/behaviour/face_target.rs
use bevy::prelude::*;
use bevy_behave::prelude::*;
use crate::common::components::{Loc, heading::Heading};
use super::Target;

#[derive(Clone, Component, Copy, Default)]
pub struct FaceTarget;

pub fn face_target(
    mut commands: Commands,
    mut query: Query<(&FaceTarget, &BehaveCtx)>,
    q_entity: Query<(&Loc, Option<&Target>)>,
    q_target_loc: Query<&Loc>,
) {
    for (_, &ctx) in &mut query {
        let target_entity = ctx.target_entity();

        // Get NPC's location and Target
        let Ok((npc_loc, Some(target))) = q_entity.get(target_entity) else {
            // No Target set, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Get Target's location
        let Ok(target_loc) = q_target_loc.get(**target) else {
            // Target entity missing Loc, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Calculate heading from NPC to Target
        let direction_qrz = **target_loc - **npc_loc;
        let new_heading = Heading::new(direction_qrz);

        // Update NPC's heading
        commands.entity(target_entity).insert(new_heading);

        // Success
        commands.trigger(ctx.success());
    }
}
```

**Behavior Tree Integration:**
```rust
// Wild Dog behavior tree (updated)
Behave::Forever => {
    Behave::Sequence => {
        FindSomethingInterestingWithin { dist: 20 },
        FaceTarget,                                    // ← NEW: Face the target
        Nearby { min: 1, max: 1, origin: Target },     // Move to adjacent (exactly 1 hex)
        PathTo::default(),
        UseAbilityIfAdjacent { ability: BasicAttack }, // ← NEW: Attack if adjacent
        Behave::Wait(2.),                              // Wait 2 seconds (attack cooldown)
    }
}
```

**Notes:**
- `FaceTarget` runs BEFORE `Nearby` to ensure heading is correct while pathfinding
- `Heading::new(direction_qrz)` converts delta Qrz → one of 6 cardinal directions (NE, E, SE, SW, W, NW)
- Heading update broadcasts via network (existing `Heading` component sync)

---

#### Decision 2: UseAbilityIfAdjacent Behavior Node

**Purpose:** Emit `Try::UseAbility` if NPC is adjacent to Target and GCD available.

**Component Definition:**
```rust
// src/server/systems/behaviour/use_ability.rs
use bevy::prelude::*;
use bevy_behave::prelude::*;
use crate::common::{
    components::{Loc, heading::Heading, gcd::Gcd},
    message::{Event, Try},
    systems::combat::abilities::AbilityType,
};
use super::Target;

#[derive(Clone, Component, Copy, Debug)]
pub struct UseAbilityIfAdjacent {
    pub ability: AbilityType,
}

pub fn use_ability_if_adjacent(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    mut query: Query<(&UseAbilityIfAdjacent, &BehaveCtx)>,
    q_entity: Query<(Entity, &Loc, &Heading, Option<&Target>, Option<&Gcd>)>,
    q_target_loc: Query<&Loc>,
) {
    for (&node, &ctx) in &mut query {
        let target_entity = ctx.target_entity();

        // Get NPC's state
        let Ok((npc_ent, npc_loc, npc_heading, Some(target), gcd)) = q_entity.get(target_entity) else {
            // Missing required components or no Target, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Check GCD cooldown
        if let Some(gcd) = gcd {
            if time.elapsed() < gcd.expires_at {
                // GCD active, fail (node will retry on next behavior tree tick)
                commands.trigger(ctx.failure());
                continue;
            }
        }

        // Get Target's location
        let Ok(target_loc) = q_target_loc.get(**target) else {
            // Target entity missing Loc, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Check if adjacent (distance == 1)
        let distance = npc_loc.distance(target_loc);
        if distance != 1 {
            // Not adjacent, fail
            commands.trigger(ctx.failure());
            continue;
        }

        // Check if facing target (within 60° cone)
        if !is_in_facing_cone(*npc_heading, **npc_loc, **target_loc) {
            // Not facing, fail (FaceTarget node should run before this)
            commands.trigger(ctx.failure());
            continue;
        }

        // All conditions met: Emit ability usage
        writer.write(Try {
            event: Event::UseAbility {
                ent: npc_ent,
                ability_type: node.ability,
                target: Some(**target),
            },
        });

        // Success (ability emitted, server will process)
        commands.trigger(ctx.success());
    }
}

// Helper function from ADR-004 (reused)
fn is_in_facing_cone(heading: Heading, caster_loc: impl Into<Qrz>, target_loc: impl Into<Qrz>) -> bool {
    let caster = caster_loc.into();
    let target = target_loc.into();

    let heading_angle = heading.to_angle();  // NE=30°, E=90°, etc.
    let target_angle = (target - caster).angle();

    let delta = (target_angle - heading_angle).abs();
    let delta_normalized = if delta > 180.0 { 360.0 - delta } else { delta };

    delta_normalized <= 30.0  // 60° cone = ±30° from heading
}
```

**Behavior Tree Integration:**
```rust
Behave::spawn_named(
    "attack if adjacent and facing",
    UseAbilityIfAdjacent { ability: AbilityType::BasicAttack }
)
```

**Failure Cases:**
- **GCD active:** Node fails, behavior tree continues (Wait node provides cooldown buffer)
- **Not adjacent:** Node fails (shouldn't happen if Nearby set min=1, max=1)
- **Not facing:** Node fails (FaceTarget should run before this)
- **No Target:** Node fails (FindSomethingInterestingWithin should set Target)

**Success Case:**
- Emits `Try::UseAbility` → Server's `execute_ability` system processes (ADR-004)
- Server validates: resources, GCD, targeting (recalculates target server-side)
- Server emits damage → inserts into reaction queue (ADR-003) → applies damage (ADR-005)

---

#### Decision 3: GCD Component Structure

**Component Definition:**
```rust
// src/common/components/gcd.rs
use bevy::prelude::*;
use std::time::Duration;
use crate::common::systems::combat::gcd::GcdType;

#[derive(Component, Clone, Copy, Debug)]
pub struct Gcd {
    pub gcd_type: Option<GcdType>,  // None = no GCD active
    pub expires_at: Duration,       // Time::elapsed() when GCD ends
}

impl Default for Gcd {
    fn default() -> Self {
        Self {
            gcd_type: None,
            expires_at: Duration::ZERO,
        }
    }
}

impl Gcd {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self, now: Duration) -> bool {
        now < self.expires_at
    }

    pub fn activate(&mut self, gcd_type: GcdType, duration: Duration, now: Duration) {
        self.gcd_type = Some(gcd_type);
        self.expires_at = now + duration;
    }

    pub fn clear(&mut self) {
        self.gcd_type = None;
        self.expires_at = Duration::ZERO;
    }
}
```

**Initialization:**
- Spawned NPCs get `Gcd::new()` (from [spawner.rs:193-211](../../src/server/systems/spawner.rs))
- Players get `Gcd::new()` on connection (from ADR-002 spawn flow)

**Server Ability Validation:**
```rust
// server/systems/abilities::execute_ability (from ADR-004)
pub fn execute_ability(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut query: Query<(&Loc, &Heading, &Stamina, &Mana, &mut Gcd)>,
    time: Res<Time>,
    // ... other params ...
) {
    for Try { event: Event::UseAbility { ent, ability_type, target } } in reader.read() {
        let Ok((loc, heading, stamina, mana, mut gcd)) = query.get_mut(*ent) else { continue };

        let def = get_ability_definition(*ability_type);

        // Check GCD
        if gcd.is_active(time.elapsed()) {
            writer.write(Do {
                event: Event::AbilityFailed {
                    ent: *ent,
                    ability_type: *ability_type,
                    reason: "Ability on cooldown".to_string(),
                },
            });
            continue;
        }

        // ... validate resources, targeting, etc. ...

        // Execute ability effects
        // ... (damage, clear queue, etc.) ...

        // Apply GCD
        gcd.activate(def.gcd_type, def.gcd_duration, time.elapsed());

        // Broadcast success
        writer.write(Do {
            event: Event::AbilityUsed { ent: *ent, ability_type: *ability_type, target: *target },
        });
    }
}
```

**Why `expires_at` (not `duration`):**
- Simpler checks: `now < expires_at` vs `now - start_time < duration`
- Matches existing `Time::elapsed()` pattern (from ADR-002)
- No need to store `started_at` timestamp

---

#### Decision 4: TargetLock Component Structure (Prevents Target Switching)

**Problem:** Behavior tree sequence failures cause `FindSomethingInterestingWithin` to re-run, potentially picking different targets. This breaks combat pressure (threats never accumulate in reaction queue).

**Solution:** Add `TargetLock` component to make target acquisition "sticky" for configurable duration.

**Component Definition:**
```rust
// src/server/components/target_lock.rs
use bevy::prelude::*;
use crate::common::components::Loc;

#[derive(Component, Clone, Copy, Debug)]
pub struct TargetLock {
    pub locked_target: Entity,
    pub max_chase_distance: u32,    // Leash distance (0 = infinite)
}

impl TargetLock {
    pub fn new(target: Entity, leash: u32) -> Self {
        Self {
            locked_target: target,
            max_chase_distance: leash,
        }
    }

    pub fn is_target_valid(
        &self,
        target_loc: Option<&Loc>,
        npc_loc: &Loc,
    ) -> bool {
        match target_loc {
            Some(loc) => {
                if self.max_chase_distance == 0 {
                    true  // No leash
                } else {
                    npc_loc.distance(*loc) <= self.max_chase_distance
                }
            }
            None => false,  // Target entity despawned
        }
    }
}
```

**Modified FindSomethingInterestingWithin → FindOrKeepTarget:**
```rust
// src/server/systems/behaviour/find_target.rs
#[derive(Clone, Component, Copy)]
pub struct FindOrKeepTarget {
    pub dist: u32,              // Acquisition range
    pub leash_distance: u32,    // Max chase distance (0 = infinite)
}

pub fn find_or_keep_target(
    mut commands: Commands,
    nntree: Res<NNTree>,
    mut query: Query<(&FindOrKeepTarget, &BehaveCtx, &Loc, Option<&TargetLock>)>,
    q_target: Query<(&Loc, &Health)>,
) {
    for (node, ctx, npc_loc, lock_opt) in &mut query {
        let npc_entity = ctx.target_entity();

        // 1. Check if we have a locked target that's still valid
        if let Some(lock) = lock_opt {
            if let Ok((target_loc, target_health)) = q_target.get(lock.locked_target) {
                // Validate: in chase range, alive
                if lock.is_target_valid(Some(target_loc), npc_loc)
                   && target_health.current > 0 {
                    // Keep existing target
                    commands.entity(npc_entity).insert(Target(lock.locked_target));
                    commands.trigger(ctx.success());
                    continue;
                }
            }
            // Target invalid: Remove lock and fall through to find new
            commands.entity(npc_entity).remove::<TargetLock>();
        }

        // 2. Find new target (existing logic from FindSomethingInterestingWithin)
        let nearby = nntree.locate_within_distance(**npc_loc, node.dist);
        // ... filter to players with Health > 0 ...
        // ... pick nearest or random ...

        if let Some(new_target) = found_target {
            // Lock to new target (persists until invalid)
            commands.entity(npc_entity).insert(TargetLock::new(
                new_target,
                node.leash_distance,
            ));
            commands.entity(npc_entity).insert(Target(new_target));
            commands.trigger(ctx.success());
        } else {
            // No valid targets: fail
            commands.trigger(ctx.failure());
        }
    }
}
```

**Key Properties:**
- **Sticky**: Once locked, NPC commits to target until invalid (no time limit)
- **Self-healing**: Auto-releases invalid targets (dead, despawned, out of leash range)
- **Configurable**: Per-NPC template (Wild Dog: 30 hex leash)
- **Idempotent**: Multiple calls don't change locked target while valid
- **Simpler**: No time-based expiry - commitment until target becomes invalid

**Why This is Mandatory for MVP:**
- **Prevents cascade failures**: Sequence can fail on GCD/facing without restarting entire tree
- **Enables ADR-003 validation**: Threats accumulate (queue pressure testable)
- **Maintains system contracts**: NPCs complete attack sequences (ADR-004 integration)
- **Blocks false negatives**: Combat won't fail tests due to AI target switching

---

#### Decision 5: Wild Dog Behavior Tree (Complete)

**Updated Behavior Tree:**
```rust
// src/server/systems/spawner.rs (spawn_npc function)
let behavior_tree = match template {
    NpcTemplate::Dog | NpcTemplate::Wolf => {
        BehaveTree::new(behave! {
            Behave::Forever => {
                Behave::Sequence => {
                    // 1. Find or keep target (sticky until invalid)
                    Behave::spawn_named(
                        "find or keep target",
                        FindOrKeepTarget {
                            dist: 20,
                            leash_distance: 30,
                        }
                    ),

                    // 2. Face the target (initial heading)
                    Behave::spawn_named(
                        "face target initial",
                        FaceTarget
                    ),

                    // 3. Move to adjacent hex (exactly 1 hex away)
                    Behave::spawn_named(
                        "move adjacent",
                        Nearby {
                            min: 1,
                            max: 1,
                            origin: NearbyOrigin::Target,
                        }
                    ),

                    // 4. Pathfind to destination
                    Behave::spawn_named(
                        "path to dest",
                        PathTo::default()
                    ),

                    // 5. Re-face target (heading may change during pathfinding)
                    Behave::spawn_named(
                        "face target after path",
                        FaceTarget
                    ),

                    // 6. Attack if adjacent and facing (respects GCD)
                    Behave::spawn_named(
                        "attack target",
                        UseAbilityIfAdjacent {
                            ability: AbilityType::BasicAttack,
                        }
                    ),

                    // 7. Wait 1 second (attack cooldown - creates queue pressure)
                    Behave::Wait(1.0),
                }
            }
        })
    }
    // ... other templates ...
};
```

**Behavior Flow:**
1. **FindOrKeepTarget** → Sets `Target` + `TargetLock` (sticky until invalid, leash 30 hexes)
2. **FaceTarget** → Updates `Heading` to face Target (initial)
3. **Nearby** → Sets `Dest` to hex adjacent to Target (distance 1)
4. **PathTo** → Moves to Dest (pathfinding, may update `Heading` while moving)
5. **FaceTarget** → Re-face Target (corrects heading after pathfinding)
6. **UseAbilityIfAdjacent** → Emits `Try::UseAbility` if adjacent + facing + GCD ready
7. **Wait(1.0)** → Pauses for 1 second (attack cooldown, enables queue pressure)
8. **Loop** → Sequence repeats (keeps locked target while valid)

**Edge Cases:**
- **Target moves:** `Nearby` recalculates Dest on next loop iteration (keeps same target due to lock)
- **Target out of leash range (>30 hexes):** `FindOrKeepTarget` detects invalid range, removes lock, finds new target
- **GCD active:** `UseAbilityIfAdjacent` fails → Wait node provides cooldown recovery → sequence continues with same target
- **Not facing after pathfinding:** Second `FaceTarget` corrects heading → `UseAbilityIfAdjacent` succeeds
- **Target dies:** `FindOrKeepTarget` detects Health=0, removes lock, finds new target
- **Target despawns:** `FindOrKeepTarget` detects missing entity, removes lock, finds new target

---

#### Decision 6: Network Synchronization

**New Network Messages:**

None! All communication uses existing events from ADR-004:

- **NPC → Server:** `Try::UseAbility { ent, ability_type, target }` (already defined)
- **Server → Clients:** `Do::AbilityUsed { ent, ability_type, target }` (already defined)
- **Server → Clients:** `Do::InsertThreat` (from ADR-003)
- **Server → Clients:** `Do::ApplyDamage` (from ADR-005)

**Component Sync:**
- `Heading` updates broadcast via existing incremental sync
- `Gcd` component does NOT sync (server-authoritative, clients don't need cooldown state)

**Why no GCD sync:**
- Clients render ability animations when receiving `Do::AbilityUsed`
- Clients don't validate cooldowns (server authoritative)
- Reduces network traffic (no per-frame GCD state updates)

---

#### Decision 7: MVP Scope and Simplifications

**MVP Includes:**
- `Gcd` component (cooldown tracking)
- `TargetLock` component (sticky target acquisition - MANDATORY)
- `FaceTarget` behavior node (heading updates, runs twice per sequence)
- `FindOrKeepTarget` behavior node (target acquisition with lock)
- `UseAbilityIfAdjacent` behavior node (ability emission)
- Wild Dog behavior tree (attack pattern with 1s cooldown)
- Integration with ADR-004 directional targeting

**MVP Excludes (Post-MVP):**
- Multiple abilities per NPC (only BasicAttack)
- Complex AI states (aggro table, fleeing, patrol)
- Boss patterns (telegraphs, phases, special abilities)
- Ranged enemies (projectile attacks)
- Buff/debuff abilities (only damage)
- Dynamic leash distances (fixed 30 hex for Wild Dog)

**Simplification Rationale:**
- Validates core mechanic (NPC uses ability → directional targeting → damage pipeline)
- Single ability sufficient for testing full combat loop
- TargetLock enables ADR-003 reaction queue validation (sustained pressure)
- Complex AI deferred until MVP proven

---

## Consequences

### Positive

#### 1. Closes Combat ADR Gap

- NPCs can now **use abilities** (missing piece from ADRs 002-005)
- Wild Dog attacks enter reaction queue → full combat loop functional
- Completes MVP scope from combat spec

#### 2. Behavior Tree Integration

- Consistent with existing NPC architecture (`FindSomethingInterestingWithin`, `Nearby`, `PathTo`)
- Per-NPC configurability (different templates = different attack patterns)
- Declarative behavior definition (behavior tree DSL readable)

#### 3. Shared GCD Component

- Players and NPCs use same cooldown system (DRY principle)
- Server's `execute_ability` validates GCD uniformly
- Easier to extend (buff abilities can modify GCD duration)

#### 4. Minimal Network Overhead

- GCD component not synced (server-authoritative)
- Reuses existing ability events (no new message types)
- Heading updates already synced (no additional cost)

#### 5. Extensible for Post-MVP

- Behavior tree nodes composable (add `UseAbilityAtRange`, `FleeIfLowHealth`)
- GCD component supports multiple ability types (already has `gcd_type` enum)
- Can migrate to separate AI system later if behavior trees become limiting

### Negative

#### 1. Behavior Tree Coupling

- NPCs tightly coupled to bevy_behave (external dependency)
- Testing requires behavior tree context (harder to unit test in isolation)
- Debugging behavior trees harder than ECS systems (state opaque)

**Mitigation:**
- MVP accepts coupling (simplicity > testability)
- Post-MVP: Extract AI logic to separate system if needed
- bevy_behave provides logging/debugging tools

#### 2. Heading Update Timing

- `FaceTarget` runs before `PathTo`, but pathfinding may rotate heading again
- If pathfinding updates heading, `UseAbilityIfAdjacent` may fail (not facing)
- Loop iteration resolves this, but causes 1-frame delay

**Mitigation:**
- `Wait(2.)` provides buffer for heading corrections
- Next loop iteration runs `FaceTarget` again (self-correcting)
- MVP: Accept 1-frame delay (negligible for 2-second attack cooldown)

#### 3. GCD Check Duplication

- `UseAbilityIfAdjacent` checks GCD
- Server's `execute_ability` also checks GCD
- Redundant validation (but necessary for correctness)

**Mitigation:**
- Behavior node check optimizes network (don't send Try::UseAbility if GCD active)
- Server check prevents cheating (authoritative validation)
- Duplication acceptable (performance negligible)

#### 4. Behavior Tree Sequence Complexity

- Sequence has 7 steps (FindOrKeepTarget → FaceTarget → Nearby → PathTo → FaceTarget → UseAbility → Wait)
- Any step failure restarts entire sequence
- More steps = higher chance of failure

**Mitigation:**
- TargetLock prevents most catastrophic failures (target switching eliminated)
- Second FaceTarget prevents facing-cone failures (heading corrected after pathfinding)
- Wait(1.0) provides GCD recovery buffer
- Measured success rate: Target >80% sequence completion (tested in Phase 6)

#### 5. No Ranged Abilities in MVP

- `UseAbilityIfAdjacent` only works for melee (distance == 1)
- Ranged enemies need `UseAbilityAtRange` node (not implemented)
- Projectile abilities need travel time simulation (future)

**Mitigation:**
- MVP scope: Wild Dog melee only
- Post-MVP Phase 2: Add `UseAbilityAtRange { min, max }` node
- Projectile system deferred to ADR-004 Phase 2 (already noted)

### Neutral

#### 1. Behavior Tree vs ECS System Tradeoff

- Behavior trees: Declarative, per-NPC config, harder to test
- ECS systems: Testable, ECS-native, more complex state management
- MVP chose behavior trees (simplicity), may refactor later

**Consideration:** Post-MVP evaluation based on AI complexity needs

#### 2. GCD Component State Management

- GCD state stored per-entity (component)
- Alternative: Global resource HashMap<Entity, GcdState>
- Component chosen (standard ECS pattern, easier queries)

**Consideration:** Component overhead negligible (small struct, sparse entities)

#### 3. FaceTarget Heading Calculation

- Uses `Heading::new(direction_qrz)` (6 cardinal directions)
- Discretizes continuous direction → nearest cardinal
- May not face target exactly (within 60° cone acceptable)

**Consideration:** 60° facing cone (ADR-004) tolerates discretization

---

## Implementation Phases

### Phase 1: Gcd Component (Foundation)

**Goal:** Add `Gcd` component to all actors

**Tasks:**
1. Create `src/common/components/gcd.rs`:
   - `Gcd` struct with `gcd_type`, `expires_at`
   - `is_active()`, `activate()`, `clear()` methods
   - Unit tests for GCD timing logic

2. Update spawn flows:
   - `spawner.rs::spawn_npc` → insert `Gcd::new()`
   - `renet.rs::do_manage_connections` (player spawn) → insert `Gcd::new()`

3. Update `server/systems/abilities::execute_ability`:
   - Query `&mut Gcd` component
   - Check `gcd.is_active(time.elapsed())` before ability execution
   - Call `gcd.activate(def.gcd_type, def.gcd_duration, time.elapsed())` on success

**Success Criteria:**
- All actors spawn with `Gcd` component
- Server validates GCD before allowing ability usage
- GCD prevents ability spam (< 0.5s between abilities)

**Duration:** 1 day

---

### Phase 2: TargetLock Component (Foundation)

**Goal:** Add `TargetLock` component for sticky target acquisition

**Tasks:**
1. Create `src/server/components/target_lock.rs`:
   - `TargetLock` struct with `locked_target`, `max_chase_distance`
   - `is_target_valid()` method
   - Unit tests for distance validation, despawn handling

2. Register component:
   - Add to server component registry
   - No sync to clients (server-authoritative)

3. Unit tests:
   - Target validation (distance, despawn detection)
   - Leash distance enforcement (0 = infinite)

**Success Criteria:**
- TargetLock component compiles and passes unit tests
- Validation logic handles edge cases (despawn, distance)
- No time-based expiry (simpler logic)

**Duration:** 1 day

---

### Phase 3: FindOrKeepTarget Behavior Node (Sticky Targeting)

**Goal:** Replace `FindSomethingInterestingWithin` with target lock logic

**Tasks:**
1. Modify `src/server/systems/behaviour/find_target.rs`:
   - Create `FindOrKeepTarget` component (with `dist`, `leash_distance`)
   - Implement `find_or_keep_target` system:
     - Check existing TargetLock first
     - Validate locked target (Health > 0, in leash range)
     - Remove TargetLock if invalid
     - Find new target if no lock or invalid
     - Insert TargetLock component on new acquisition
   - Unit tests for sticky behavior

2. Add to behavior plugin:
   - Register `FindOrKeepTarget` component
   - Register `find_or_keep_target` system

3. Update Wild Dog behavior tree:
   - Replace `FindSomethingInterestingWithin` with `FindOrKeepTarget`

**Success Criteria:**
- NPC acquires target, locks until invalid (no time limit)
- NPC keeps target even when another player is closer
- Lock releases ONLY when target dies, despawns, or exceeds leash distance
- Target switching eliminated except for invalid targets (verified via logs)

**Duration:** 2 days

---

### Phase 4: FaceTarget Behavior Node

**Goal:** NPCs update heading to face Target

**Tasks:**
1. Create `src/server/systems/behaviour/face_target.rs`:
   - `FaceTarget` component
   - `face_target` system (queries Target, calculates heading, updates Heading)
   - Unit tests for heading calculation

2. Add to behavior plugin:
   - Register `FaceTarget` component
   - Register `face_target` system

3. Update Wild Dog behavior tree:
   - Insert `FaceTarget` after `FindOrKeepTarget` (initial face)
   - Insert `FaceTarget` after `PathTo` (re-face after movement)

**Success Criteria:**
- Wild Dog faces player after finding them
- Heading updates visible (Dog sprite rotates)
- Second FaceTarget corrects heading after pathfinding
- UseAbilityIfAdjacent succeeds (facing cone check passes)

**Duration:** 1 day

---

### Phase 5: UseAbilityIfAdjacent Behavior Node

**Goal:** NPCs emit ability usage when conditions met, with proper attack speed

**Tasks:**
1. Create `src/server/systems/behaviour/use_ability.rs`:
   - `UseAbilityIfAdjacent` component
   - `use_ability_if_adjacent` system (checks GCD, distance, facing, emits Try::UseAbility)
   - Helper function: `is_in_facing_cone` (from ADR-004)

2. Add to behavior plugin:
   - Register `UseAbilityIfAdjacent` component
   - Register `use_ability_if_adjacent` system

3. Update Wild Dog behavior tree:
   - Insert `UseAbilityIfAdjacent { ability: BasicAttack }` after second `FaceTarget`
   - Change `Wait(5.)` → `Wait(1.0)` (1-second attack cooldown for queue pressure)

**Success Criteria:**
- Wild Dog emits `Try::UseAbility` when adjacent to player
- Server processes ability (ADR-004 flow)
- Damage enters player's reaction queue (ADR-003 flow)
- Player can Dodge to clear queue (ADR-003 flow)
- Attack speed ~1s (enables queue pressure testing)

**Duration:** 2 days

---

### Phase 6: Integration Testing (Extended)

**Goal:** Validate full combat loop with sustained pressure (CRITICAL for ADR-003 validation)

**Tasks:**
1. Test full flow:
   - Wild Dog spawns near player
   - Dog moves adjacent
   - Dog faces player
   - Dog attacks every ~1 second (Wait + GCD)
   - Damage enters player's reaction queue
   - Player can Dodge or take damage

2. Test target lock behavior (CRITICAL):
   - Dog acquires Player A as target
   - Player B runs past (closer than Player A)
   - Dog IGNORES Player B, continues chasing Player A indefinitely
   - Player A moves >30 hexes away (leash), Dog releases lock and finds new target
   - Player A dies, Dog releases lock and finds new target

3. Test sustained pressure (MOST IMPORTANT):
   - Player with Focus=0 (3 slots), Instinct=0 (1.0s timers)
   - 2 Wild Dogs attack adjacent to player
   - Dogs attack every 1 second
   - Queue fills to capacity within 3 seconds
   - Player forced to use Dodge or take overflow damage
   - **This validates ADR-003 reaction queue mechanics**

4. Test edge cases:
   - Player moves away mid-attack (Dog pursues, maintains lock)
   - Multiple Dogs attack same player (multiple threats accumulate)
   - Player kills Dog (Dog despawns, respawns without lock)
   - Dog GCD active (UseAbilityIfAdjacent fails gracefully, retries next loop)

5. Measure behavior tree success rate:
   - Log sequence failures (add `warn!()` to failed nodes)
   - Count: sequences completed vs. sequences restarted
   - **Target: >80% sequence completion rate**
   - If <50%: Structural problem in behavior tree design

**Success Criteria:**
- Wild Dog combat loop functional end-to-end
- Target switching eliminated (measured via logs)
- Queue accumulation works (2-3 threats in queue under sustained pressure)
- Sequence success rate >80%
- Player must use Dodge to survive (combat pressure validated)
- Combat feels responsive and challenging

**Duration:** 2 days

---

### Phase 7: Visual Polish (Client Feedback)

**Goal:** Show ability usage visually on client

**Tasks:**
1. Client responds to `Do::AbilityUsed`:
   - Play attack animation (Dog lunges toward player)
   - Play attack sound effect
   - Show visual feedback (swing arc, impact effect)

2. GCD visual feedback (optional):
   - Client could show cooldown timer above Dog (not required for MVP)
   - Useful for debugging, may defer to Phase 2

**Success Criteria:**
- Dog attacks have clear visual/audio feedback
- Player sees attack coming (clarity)
- Combat feels responsive

**Duration:** 1 day

---

## Validation Criteria

### Functional Tests

- **GCD Component:** Actor spawns with `Gcd`, server enforces cooldown (<0.5s between abilities)
- **TargetLock Component:** NPC locks to target until invalid, ignores closer targets while locked
- **FindOrKeepTarget Node:** Dog acquires target, maintains lock until death/despawn/leash
- **FaceTarget Node:** Dog faces player after `FindOrKeepTarget` succeeds, re-faces after PathTo
- **UseAbilityIfAdjacent Node:** Dog emits `Try::UseAbility` when adjacent + facing + GCD ready
- **Full Combat Loop:** Dog attacks → damage queued → player dodges or takes damage
- **Attack Cooldown:** Dog attacks every ~1 second (Wait node + GCD enforcement)
- **Sustained Pressure:** 2 Dogs fill queue to capacity within 3 seconds (ADR-003 validated)

### Network Tests

- **Ability Sync:** Client receives `Do::AbilityUsed` within 100ms of server emission
- **Damage Sync:** Client receives `Do::InsertThreat` immediately after ability hits
- **Heading Sync:** Dog heading updates visible to all clients (incremental sync)

### Performance Tests

- **Behavior Tree Overhead:** 100 Dogs attacking, CPU < 10% (behavior tree processing)
- **GCD Queries:** 1000 actors checking GCD, query < 1ms
- **Network Bandwidth:** 10 Dogs attacking = ~500 bytes/sec (ability events)

### UX Tests

- **Attack Clarity:** Player sees Dog attack coming (animation, sound)
- **Combat Flow:** Dog pursuit → attack → damage → reaction feels natural
- **Cooldown Feedback:** Player can predict Dog attack timing (~1 second)
- **Target Commitment:** Player understands Dog committed to chase (no random abandonment)
- **Combat Pressure:** Player feels urgency (queue filling, must react)

---

## Open Questions

### Design Questions

1. **Should FaceTarget run once or twice?**
   - ~~Before Nearby only: Heading set early, pathfinding may override~~
   - ✅ **RESOLVED:** Run twice (before Nearby, after PathTo) to prevent facing-cone failures

2. **Should Wait duration match GCD duration?**
   - ~~Wait(2.) provides buffer for GCD (0.5s) + attack animation (~0.5s)~~
   - ~~Longer Wait = slower attacks, shorter Wait = GCD failures~~
   - ✅ **RESOLVED:** Wait(1.0) enables queue pressure (ADR-003 validation requires <2s attacks)

3. **Should TargetLock be optional or mandatory for MVP?**
   - ~~Optional: Simpler implementation, defer sticky targeting~~
   - ✅ **RESOLVED:** Mandatory - prevents target switching, enables ADR-003 validation

4. **Should TargetLock have time-based expiry?**
   - ~~Original: 10s lock duration, then can switch targets~~
   - ✅ **RESOLVED:** No time limit - lock persists until target invalid (simpler, better gameplay)

### Technical Questions

1. **Should Gcd component sync to clients?**
   - Pro: Clients could show cooldown timers
   - Con: Increases network traffic
   - MVP: Don't sync (clients infer cooldown from ability usage events)

2. **Should behavior tree state persist across loops?**
   - ~~Current: Forever → Sequence restarts on completion~~
   - ~~Alternative: Stateful tree (remembers Target across loops)~~
   - ✅ **RESOLVED:** TargetLock component provides statefulness (Target persists via lock)

3. **Should PathTo update Heading while moving?**
   - Current: Assumed yes (follows movement direction)
   - Risk: Heading changes → not facing Target → UseAbilityIfAdjacent fails
   - Resolution: Next loop runs FaceTarget again (self-correcting)

---

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

- **Ranged Abilities:** `UseAbilityAtRange { min, max }` node for ranged attacks
- **Buff/Debuff Abilities:** NPCs cast buffs on allies, debuffs on enemies
- **Boss Patterns:** Multi-phase behavior trees, telegraphs, special abilities
- **Leash Mechanics:** NPCs return to spawn point if player too far
- **Aggro Tables:** NPCs prioritize highest threat player (threat = damage dealt)
- **Flee Behavior:** NPCs flee when low health

### Optimization

- **Behavior Tree Pooling:** Reuse behavior tree instances (object pooling)
- **GCD Batch Validation:** Check GCD for all NPCs in single query (cache results)
- **Heading Interpolation:** Smooth heading rotations (not instant snap)

### Advanced Features

- **Utility AI:** NPCs choose abilities based on utility scores (not hardcoded trees)
- **State Machines:** Complex AI states (patrol, chase, attack, flee)
- **Coordination:** NPCs coordinate attacks (flanking, focus fire)

---

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (Wild Dog attack pattern, heading mechanics)
- **Behavior Tree:** bevy_behave documentation (behavior node patterns)

### Codebase

- **Existing Behavior Nodes:** `src/server/systems/behaviour/mod.rs` (FindSomethingInterestingWithin, Nearby)
- **Spawner System:** `src/server/systems/spawner.rs` (NPC spawn, behavior tree creation)
- **GcdType Enum:** `src/common/systems/combat/gcd.rs` (existing GCD types)

### Related ADRs

- **ADR-002:** Combat Foundation (GCD infrastructure mentioned, not yet implemented)
- **ADR-003:** Reaction Queue System (threat insertion, timer expiry)
- **ADR-004:** Ability System and Directional Targeting (directional targeting, execute_ability validation, facing cone)
- **ADR-005:** Damage Pipeline (damage calculation, threat resolution)

---

## Decision Makers

- ARCHITECT role evaluation
- User specification: Option 1 (behavior tree), GCD component, FaceTarget node
- Game design requirements: `docs/spec/combat-system.md` (Wild Dog attack pattern)

## Date

2025-10-30

---

## Summary for Developers

**What this ADR adds:**

1. **`Gcd` Component** - Cooldown tracking for all actors (players and NPCs)
2. **`TargetLock` Component** - Sticky target acquisition (prevents target switching, MANDATORY for MVP)
3. **`FindOrKeepTarget` Behavior Node** - Target acquisition with sticky lock (no time limit, persists until invalid)
4. **`FaceTarget` Behavior Node** - Updates NPC heading to face Target entity (runs twice per sequence)
5. **`UseAbilityIfAdjacent` Behavior Node** - Emits `Try::UseAbility` when adjacent + facing + GCD ready
6. **Wild Dog Behavior Tree** - Complete attack pattern with sustained pressure (1s attack cooldown)

**Integration Points:**

- Uses `select_target` from ADR-004 (directional targeting with 60° facing cone)
- Emits `Try::UseAbility` → processed by ADR-004 `execute_ability`
- Ability hits → ADR-003 inserts threat → ADR-005 applies damage
- GCD enforced at behavior node (optimization) and server (security)

**Testing Priority:**

1. TargetLock prevents target switching (CRITICAL - enables ADR-003 validation)
2. FindOrKeepTarget maintains lock until invalid, releases only on death/despawn/leash
3. FaceTarget updates heading correctly (twice: before and after pathfinding)
4. UseAbilityIfAdjacent emits abilities when conditions met
5. Full combat loop functional with sustained pressure (Dog → attack → queue fills → Dodge required)

---

## Quick Start: MVP Implementation Checklist

For developers ready to implement, here's the **minimum viable path** to functional dog combat:

### Phase 1: Foundation (1-2 days)
- [ ] Create `Gcd` component in `src/common/components/gcd.rs`
- [ ] Create `TargetLock` component in `src/server/components/target_lock.rs`
- [ ] Add `Gcd::new()` to NPC and player spawn flows
- [ ] Unit tests for both components

### Phase 2: Behavior Nodes (2-3 days)
- [ ] Implement `FindOrKeepTarget` in `src/server/systems/behaviour/find_target.rs`
- [ ] Implement `FaceTarget` in `src/server/systems/behaviour/face_target.rs`
- [ ] Implement `UseAbilityIfAdjacent` in `src/server/systems/behaviour/use_ability.rs`
- [ ] Register all nodes with behavior plugin

### Phase 3: Wild Dog Behavior Tree (1 day)
- [ ] Update Wild Dog tree in `spawner.rs` to use new nodes
- [ ] Sequence: FindOrKeepTarget → FaceTarget → Nearby → PathTo → FaceTarget → UseAbilityIfAdjacent → Wait(1.0)
- [ ] Set leash distance to 30 hexes, acquisition range to 20 hexes

### Phase 4: Validation (1 day)
- [ ] **Critical Test:** Dog locks to Player A, ignores closer Player B
- [ ] **Critical Test:** 2 Dogs fill reaction queue within 3 seconds
- [ ] **Critical Test:** Behavior tree success rate >80%
- [ ] Visual test: Dogs chase and attack consistently every ~1 second

### Success Criteria
✅ Dogs commit to targets (no mid-chase abandonment)
✅ Attack speed ~1 second (creates pressure)
✅ Reaction queue accumulates threats (validates ADR-003)
✅ Combat feels intense and consistent (player feedback matches expected outcome)
