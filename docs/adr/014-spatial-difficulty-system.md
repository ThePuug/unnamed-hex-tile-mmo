# ADR-014: Spatial Difficulty System (Level-Based Enemy Archetypes)

## Status
Proposed

## Context

### Problem Statement

The current spawner system creates uniform difficulty across the entire world - every NPC is level 10, every location presents identical threat. This creates several player experience issues:

**Current State:**
- Static NPC spawners scattered across terrain (all spawn level 10 enemies)
- Uniform difficulty regardless of location (no spatial variation)
- No incentive to explore (every tile is mechanically identical)
- Players unable to self-select challenge level (combat too hard/easy for everyone)
- Knockback ability not working well (to be removed)

**Missing Systems:**
- No experience/leveling for players (all players are static level 10)
- No loot system (no extrinsic exploration rewards)
- No resource gathering (no economic exploration incentive)
- No progression (players don't get stronger over time)

**Attempted Solutions:**
- Haven system spec ([haven-system.md](../spec/haven-system.md)) provides full solution: 3 havens, influence radius, encroachment calculation, siege integration
- **Problem:** Haven system depends on hub/siege/influence systems (ADRs not yet written)
- **Problem:** Full implementation ~40+ hours (3 havens, influence math, siege integration, biome placement)

### Design Goals

**Immediate (Combat Prototyping):**
1. **Spatial variation** - Different locations spawn different level enemies with distinct builds
2. **Self-directed difficulty** - Players choose challenge level by location
3. **Exploration foundation** - Distance from spawn matters mechanically
4. **Accessibility** - New/struggling players can find comfortable zones
5. **Tactical variety** - Different enemy archetypes require different combat approaches

**Future (Haven System Integration):**
6. **Foundation for influence** - Distance-based scaling prepares for encroachment system
7. **Hub integration ready** - Can add player-created hubs without rearchitecture
8. **Reward system ready** - Loot/XP can plug in when implemented

### Spec References

- **Haven System:** [haven-system.md](../spec/haven-system.md) Lines 1-160
- **Siege System:** [siege-system.md](../spec/siege-system.md) Lines 1-190
- **Attribute System:** [attribute-system.md](../spec/attribute-system.md) Lines 1-100
- **Haven Feature Matrix:** [haven-system-feature-matrix.md](../spec/haven-system-feature-matrix.md)

### Deviation from Spec

This ADR intentionally implements a **simplified subset** of the haven system to enable playable spatial variation during combat prototyping:

**Deviations:**
- ❌ Single haven instead of three (Mountain/Prairie/Forest)
- ❌ No influence radius system (using direct distance calculation)
- ❌ No encroachment formula (using distance-based level scaling)
- ❌ No siege mechanics (no anger, no waves, no destruction)
- ❌ No biome-specific placement (spawn at world origin)

**Retained Concepts:**
- ✅ Haven as safe spawn zone (spec Line 27-29)
- ✅ Distance = difficulty curve (spec Lines 96-104 territory zones)
- ✅ Foundation for future expansion (spec Lines 106-149 progression timeline)

**Rationale:**
- Combat systems (ADR-002 through ADR-012) need playable variety NOW
- Attribute system already exists - use it properly for enemy differentiation
- Intrinsic reward (spatial difficulty selection) provides immediate value
- Foundation prepares for future haven/hub/siege integration

---

## Decision

**Implement level-based spatial difficulty system: single haven spawn + distance-based enemy leveling + directional enemy archetypes with attribute distributions + archetype-specific AI behaviors.**

Enemy level scales from 0 (near haven) to 10 (far from haven). Each archetype distributes attribute points differently, creating distinct combat profiles. New abilities (Counter, Volley) enable kiting/defensive archetypes. Knockback ability removed.

---

## Technical Design

### Core Mechanic Summary

**Single Haven Spawn**
- World origin (0, 0) becomes permanent spawn point
- Players spawn at haven on join/death
- Haven provides safe zone reference point (no mechanics, just location)

**Distance-Based Enemy Leveling**
- Enemy level scales linearly with distance from haven
- Every 100 tiles = 1 level (0-99 tiles = level 0, 100-199 = level 1, etc.)
- Each level = 1 attribute point to spend on archetype's chosen axis

**Directional Enemy Archetypes**
- Different compass directions spawn enemies with different attribute distributions
- Four archetypes: Berserker, Juggernaut, Kiter, Defender
- Each archetype has ONE signature ability (no supporting abilities)

**Archetype-Specific AI**
- Berserker/Juggernaut/Defender: Chase AI (existing - close to melee, use ability)
- Kiter: Kite AI (existing - Forest Sprite behavior, maintain distance, use Volley)

---

## Enemy Leveling System

### Level Calculation from Distance

```rust
pub const HAVEN_LOCATION: Qrz = Qrz::ORIGIN;

/// Calculate enemy level based on distance from haven
/// <100 tiles = level 0
/// 1000 tiles = level 10
/// Linear scaling in between
pub fn calculate_enemy_level(spawn_location: Qrz, haven_location: Qrz) -> u8 {
    let distance = haven_location.distance_to(spawn_location) as f32;

    // Linear scaling: level = distance / 100
    // Clamped to 0-10 range
    (distance / 100.0).min(10.0) as u8
}
```

**Level Progression:**

| Distance | Level | Attribute Points | Description |
|----------|-------|------------------|-------------|
| 0-99     | 0     | 0 points         | Tutorial zone |
| 100-199  | 1     | 1 point          | Very Easy |
| 200-299  | 2     | 2 points         | Easy |
| 300-399  | 3     | 3 points         | Easy-Moderate |
| 400-499  | 4     | 4 points         | Moderate |
| 500-599  | 5     | 5 points         | Moderate-Hard |
| 600-699  | 6     | 6 points         | Hard |
| 700-799  | 7     | 7 points         | Hard-Very Hard |
| 800-899  | 8     | 8 points         | Very Hard |
| 900-999  | 9     | 9 points         | Extreme |
| 1000+    | 10    | 10 points        | Maximum |

**Design Rationale:**
- Smooth gradient (no sudden spikes)
- 100 tiles per level = easy mental math for players ("I'm 500 tiles out, so level 5 enemies")
- Aligns with haven spec territory zones (spec Lines 96-104)

---

## Directional Enemy Archetypes

### Zone Definition

Divide world into 4 cardinal direction zones based on angle from haven:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DirectionalZone {
    North,  // 315° - 45° (top) - Berserkers
    East,   // 45° - 135° (right) - Juggernauts
    South,  // 135° - 225° (bottom) - Kiters
    West,   // 225° - 315° (left) - Defenders
}

pub fn get_directional_zone(spawn_location: Qrz, haven_location: Qrz) -> DirectionalZone {
    // Calculate angle from haven to spawn point
    let delta = spawn_location - haven_location;
    let angle = f32::atan2(delta.y() as f32, delta.x() as f32).to_degrees();

    // Normalize to 0-360
    let angle = if angle < 0.0 { angle + 360.0 } else { angle };

    match angle {
        a if a >= 315.0 || a < 45.0 => DirectionalZone::North,
        a if a >= 45.0 && a < 135.0 => DirectionalZone::East,
        a if a >= 135.0 && a < 225.0 => DirectionalZone::South,
        _ => DirectionalZone::West,
    }
}
```

### Archetype Definitions

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnemyArchetype {
    Berserker,   // North - Aggressive melee burst
    Juggernaut,  // East - Tanky melee pressure
    Kiter,       // South - Ranged harassment
    Defender,    // West - Reactive counter-attacks
}

impl EnemyArchetype {
    /// Get archetype from directional zone
    pub fn from_zone(zone: DirectionalZone) -> Self {
        match zone {
            DirectionalZone::North => EnemyArchetype::Berserker,
            DirectionalZone::East => EnemyArchetype::Juggernaut,
            DirectionalZone::South => EnemyArchetype::Kiter,
            DirectionalZone::West => EnemyArchetype::Defender,
        }
    }

    /// Get signature ability for this archetype (one ability only)
    pub fn ability(&self) -> AbilityKey {
        match self {
            EnemyArchetype::Berserker => AbilityKey::Lunge,
            EnemyArchetype::Juggernaut => AbilityKey::Overpower,
            EnemyArchetype::Kiter => AbilityKey::Volley,      // NEW ABILITY
            EnemyArchetype::Defender => AbilityKey::Counter,  // NEW ABILITY (replaces Knockback)
        }
    }

    /// Get AI behavior type (existing behaviors)
    pub fn ai_behavior(&self) -> AIBehaviorType {
        match self {
            EnemyArchetype::Berserker => AIBehaviorType::Chase,      // Existing
            EnemyArchetype::Juggernaut => AIBehaviorType::Chase,     // Existing
            EnemyArchetype::Kiter => AIBehaviorType::Kite,           // Existing (Forest Sprite)
            EnemyArchetype::Defender => AIBehaviorType::Chase,       // Existing
        }
    }
}
```

---

## Attribute Distribution System

### Distribution Strategy per Archetype

Each archetype allocates 1 attribute point per level, alternating between two attributes:

**Attribute Allocation Rules:**
- **Berserker**: Might/Instinct equally (Might first on odd levels)
- **Juggernaut**: Vitality/Presence equally (Vitality first on odd levels)
- **Kiter**: Grace/Focus equally (Grace first on odd levels)
- **Defender**: Focus/Instinct equally (Focus first on odd levels)

```rust
impl EnemyArchetype {
    /// Get which attributes this archetype invests in (as axis indices)
    pub fn primary_axes(&self) -> (AxisPair, AxisPair) {
        match self {
            // Berserker: Might (odd levels) / Instinct (even levels)
            EnemyArchetype::Berserker => (AxisPair::MightGrace, AxisPair::InstinctPresence),
            // Juggernaut: Vitality (odd levels) / Presence (even levels)
            EnemyArchetype::Juggernaut => (AxisPair::VitalityFocus, AxisPair::InstinctPresence),
            // Kiter: Grace (odd levels) / Focus (even levels)
            EnemyArchetype::Kiter => (AxisPair::MightGrace, AxisPair::VitalityFocus),
            // Defender: Focus (odd levels) / Instinct (even levels)
            EnemyArchetype::Defender => (AxisPair::VitalityFocus, AxisPair::InstinctPresence),
        }
    }

    /// Get attribute direction (left/right on axis)
    pub fn primary_directions(&self) -> (i8, i8) {
        match self {
            // Berserker: -Might (left), -Instinct (left)
            EnemyArchetype::Berserker => (-1, -1),
            // Juggernaut: -Vitality (left), +Presence (right)
            EnemyArchetype::Juggernaut => (-1, 1),
            // Kiter: +Grace (right), +Focus (right)
            EnemyArchetype::Kiter => (1, 1),
            // Defender: +Focus (right), -Instinct (left)
            EnemyArchetype::Defender => (1, -1),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AxisPair {
    MightGrace,
    VitalityFocus,
    InstinctPresence,
}
```

### Attribute Allocation Logic

```rust
/// Calculate ActorAttributes for an enemy based on level and archetype
/// Each level = 1 attribute point, alternating between two attributes
pub fn calculate_enemy_attributes(
    level: u8,
    archetype: EnemyArchetype,
) -> ActorAttributes {
    let (axis1, axis2) = archetype.primary_axes();
    let (dir1, dir2) = archetype.primary_directions();

    // Calculate how many points go to each axis
    // Odd levels go to axis1, even levels go to axis2
    let points_axis1 = ((level + 1) / 2) as i8; // Levels 1,3,5,7,9 -> 1,2,3,4,5
    let points_axis2 = (level / 2) as i8;       // Levels 2,4,6,8,10 -> 1,2,3,4,5

    // Apply direction multiplier (-1 for left, +1 for right)
    let value_axis1 = points_axis1 * dir1;
    let value_axis2 = points_axis2 * dir2;

    // Assign to correct axis pairs
    let mut might_grace = 0;
    let mut vitality_focus = 0;
    let mut instinct_presence = 0;

    match axis1 {
        AxisPair::MightGrace => might_grace = value_axis1,
        AxisPair::VitalityFocus => vitality_focus = value_axis1,
        AxisPair::InstinctPresence => instinct_presence = value_axis1,
    }

    match axis2 {
        AxisPair::MightGrace => might_grace = value_axis2,
        AxisPair::VitalityFocus => vitality_focus = value_axis2,
        AxisPair::InstinctPresence => instinct_presence = value_axis2,
    }

    ActorAttributes::new(
        might_grace,
        0, // spectrum = 0 for NPCs
        0, // shift = 0 for NPCs
        vitality_focus,
        0,
        0,
        instinct_presence,
        0,
        0,
    )
}
```

**Example Attribute Progressions:**

**Berserker:**
- Level 1: -1 Might, 0 Instinct
- Level 2: -1 Might, -1 Instinct
- Level 3: -2 Might, -1 Instinct
- Level 4: -2 Might, -2 Instinct
- Level 5: -3 Might, -2 Instinct
- Result: Balanced physical damage + reaction speed

**Juggernaut:**
- Level 1: -1 Vitality, 0 Presence
- Level 2: -1 Vitality, +1 Presence
- Level 3: -2 Vitality, +1 Presence
- Level 4: -2 Vitality, +2 Presence
- Level 5: -3 Vitality, +2 Presence
- Result: High HP + threat generation/AoE

**Kiter:**
- Level 1: +1 Grace, 0 Focus
- Level 2: +1 Grace, +1 Focus
- Level 3: +2 Grace, +1 Focus
- Level 4: +2 Grace, +2 Focus
- Level 5: +3 Grace, +2 Focus
- Result: Movement speed + mana pool

**Defender:**
- Level 1: +1 Focus, 0 Instinct
- Level 2: +1 Focus, -1 Instinct
- Level 3: +2 Focus, -1 Instinct
- Level 4: +2 Focus, -2 Instinct
- Level 5: +3 Focus, -2 Instinct
- Result: Mana + critical chance/parry

---

## Abilities

### Volley (Kiter Primary - EXISTING)

**Type:** Ranged attack
**Range:** 3-6 hexes
**Cost:** 20 stamina
**Damage:** 30 (base)
**Recovery:** 0.8s

**Description:** Fire a quick projectile at distant target. Cannot be used on adjacent enemies (min range 3).

**Tactical Purpose:**
- Kiter archetype maintains distance while dealing damage
- Forces players to close gap or use ranged abilities
- Creates spatial positioning gameplay

**Implementation Status:**
- ✅ Already exists: `src/server/systems/combat/abilities/volley.rs`
- ✅ Already integrated with Kite AI (Forest Sprite uses it)
- ✅ Validation: target in range 3-6, line of sight
- ✅ Effect: Projectile with travel time, damage on hit

### Counter (Defender Primary - Replaces Knockback)

**Type:** Push / Counter-attack (same type as Knockback)
**Range:** Melee (1 hex)
**Cost:** 30 stamina
**Damage:** 50% of mitigated threat damage (reflected)
**Recovery:** 1.2s

**Description:** React to incoming threat at front of ReactionQueue. Fully mitigate the threat's damage and reflect 50% back to attacker.

**Mechanic:**
1. Remove threat from **front** of ReactionQueue (Defender's queue)
2. Fully negate the threat's damage (Defender takes 0 damage)
3. Queue 50% of threat's damage as new threat in **origin entity's ReactionQueue**

**Tactical Purpose:**
- Defender archetype punishes aggressive players
- Rewards defensive timing (must react to front of queue)
- Strong counter to heavy-hitting abilities (50% of high damage queued back)
- Creates counter-attack chains (player must react to reflected threat)
- Fully integrated with ReactionQueue system (ADR-003)

**Usage Pattern:**
- AI checks if ReactionQueue has entries at front
- Uses Counter when threat detected
- Negates damage + queues reflected damage back to attacker

**Example:**
- Player uses Lunge (40 damage) → queues threat in Defender's ReactionQueue
- Defender uses Counter before threat resolves
- Defender takes 0 damage (threat removed from Defender's queue)
- 20 damage threat queued in Player's ReactionQueue (50% reflected)
- Player must now react (Deflect/Counter) or take 20 damage when threat resolves

**Synergy Integration:**
- Counter has same ability type as Knockback (Push)
- Overpower → Counter synergy works (ADR-012 Overpower → Knockback becomes Overpower → Counter)
- Drop-in replacement for Knockback on ability bars

**Implementation Requirements:**
- New ability file: `src/server/systems/combat/abilities/counter.rs`
- Same ability type enum as Knockback (preserves synergy)
- Validation: Has entries in ReactionQueue (front entry)
- Effect: Pop front threat, negate its damage, queue 50% damage to origin entity's ReactionQueue

---

## Knockback Removal

**Reason:** "Not working well anyway" (user feedback)

**Affected Systems:**
- Delete file: `src/server/systems/combat/abilities/knockback.rs`
- Rename enum: `AbilityKey::Knockback` → `AbilityKey::Counter`
- Update ability bar assignments: Replace Knockback with Counter
- Preserve ADR-012 synergy: Overpower → Counter (same ability type)
- Update AI ability loadouts

**Migration:**
- Counter is drop-in replacement for Knockback (same ability type for synergies)
- Overpower → Knockback synergy becomes Overpower → Counter automatically
- No other archetypes relied on Knockback

---

## AI Behavior System

### Behavior Types

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AIBehaviorType {
    Chase,  // Close distance to target, use melee abilities
    Kite,   // Maintain distance from target, use ranged abilities
}
```

### Chase AI (Berserker, Juggernaut, Defender)

**Existing Behavior Tree** (no changes needed):
```rust
Behave::Forever => {
    Behave::Sequence => {
        FindSomethingInterestingWithin { dist: 20 },
        FaceTarget,
        Nearby { min: 1, max: 1, origin: Target },  // Move to adjacent
        PathTo::default(),
        UseAbilityIfAdjacent { ability },  // Use archetype's single ability
        Behave::Wait(1.0),  // Wait for recovery
    }
}
```

**Ability Usage:**
- Berserker: Use Lunge when in range (1-4 hexes)
- Juggernaut: Use Overpower when adjacent (1 hex)
- Defender: Use Counter when ReactionQueue has entries AND adjacent

### Kite AI (Kiter Only)

**Existing Behavior Tree** (Forest Sprite - no changes needed):
```rust
Behave::Forever => {
    Behave::Sequence => {
        FindSomethingInterestingWithin { dist: 20 },
        FaceTarget,
        Nearby { min: 3, max: 6, origin: Target }, // Maintain distance 3-6
        PathTo::default(),
        UseAbilityIfInRange { ability: Volley, min: 3, max: 6 },
        Behave::Wait(1.0),
    }
}
```

**Ability Usage:**
- Kiter: Use Volley when in range (3-6 hexes)

---

## Dynamic Engagement System

### System Overview

**Problem:** Static spawners create farmable locations that discourage exploration and conflict with spatial difficulty goals.

**Solution:** Dynamic engagement spawning triggered by player chunk discovery - engagements appear as players explore, creating exploration-driven content discovery.

**Architecture Goals:**
- **Exploration-driven** - Movement finds content (not camping)
- **Self-paced** - Players control encounter rate through exploration speed
- **Fair spawning** - Never spawn on top of players
- **Bounded density** - Budget system prevents overwhelming encounters
- **Cleanup lifecycle** - Engagements despawn when complete or abandoned

---

### Trigger Mechanism

**Primary Trigger:** Player receives chunk from server (client-side chunk load event)

**Design Rationale:**
- Chunk receipt happens when player moves into unloaded terrain
- Server-authoritative (chunk sent from server, not client prediction)
- Already exists in networking layer (no new events needed)
- Naturally tied to exploration (move = load chunks = potential spawns)

**Integration Point:**
```rust
// In server/systems/chunk_management.rs (or equivalent)
// Hook existing chunk send logic:

fn send_chunk_to_player(player_id: ActorId, chunk_pos: Qrz) {
    // ... existing chunk send logic ...

    // NEW: Check if this should trigger engagement spawn
    try_spawn_engagement(chunk_pos, player_id);
}
```

---

### Spawn Validation Pipeline

**Multi-stage validation ensures fair, bounded spawning:**

```rust
/// Attempt to spawn engagement when player receives chunk
fn try_spawn_engagement(chunk_pos: Qrz, requesting_player: ActorId) {
    // Stage 1: Probability gate (50% chance)
    if !rand::gen_bool(0.5) {
        return; // Silent skip
    }

    // Stage 2: Budget check (max 5 active per zone)
    let zone = get_zone_for_position(chunk_pos, ZONE_RADIUS);
    if count_active_engagements_in_zone(zone) >= MAX_ENGAGEMENTS_PER_ZONE {
        return; // Zone full
    }

    // Stage 3: Player proximity check (min 30 tiles from ANY player)
    for player_pos in all_player_positions() {
        if chunk_pos.distance_to(player_pos) < MIN_DISTANCE_FROM_PLAYER {
            return; // Too close to a player
        }
    }

    // Stage 4: Engagement proximity check (min 50 tiles from other engagements)
    for engagement_pos in active_engagement_positions() {
        if chunk_pos.distance_to(engagement_pos) < MIN_DISTANCE_FROM_ENGAGEMENT {
            return; // Too close to another engagement
        }
    }

    // All checks passed: Spawn engagement
    spawn_engagement_at(chunk_pos);
}
```

**Validation Constants:**
```rust
const SPAWN_PROBABILITY: f64 = 0.5;              // 50% chance per chunk
const ZONE_RADIUS: u32 = 500;                     // Zone = 500 tile radius
const MAX_ENGAGEMENTS_PER_ZONE: usize = 5;        // Budget cap
const MIN_DISTANCE_FROM_PLAYER: u32 = 30;         // Safety radius
const MIN_DISTANCE_FROM_ENGAGEMENT: u32 = 50;     // Spacing between encounters
```

**Design Tradeoffs:**
- **50% probability**: Prevents predictable "every chunk = fight", varies encounter pacing
- **Zone budget (5)**: Balances content density (not empty, not overwhelming)
- **Player distance (30 tiles)**: Prevents unfair surprise spawns
- **Engagement distance (50 tiles)**: Ensures independent encounters (no unintended multi-fights)

---

### Engagement Entity Structure

**New Entity Type:** `Engagement` (parent entity for engagement group)

```rust
#[derive(Component)]
pub struct Engagement {
    pub spawn_location: Qrz,
    pub level: u8,
    pub archetype: EnemyArchetype,
    pub npc_count: u8,           // 1-3 NPCs
    pub spawned_npcs: Vec<Entity>, // Child NPC entities
    pub zone_id: ZoneId,          // For budget tracking
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZoneId(pub i32, pub i32); // Zone grid coordinates

impl ZoneId {
    /// Calculate zone ID from position (500-tile zones)
    pub fn from_position(pos: Qrz) -> Self {
        ZoneId(
            pos.x() / ZONE_RADIUS as i32,
            pos.y() / ZONE_RADIUS as i32,
        )
    }
}
```

**Engagement Lifecycle:**
```
1. Trigger (chunk received)
   → Validation (probability, budget, distances)
   → Spawn engagement entity

2. Spawn NPCs (1-3 count)
   → Calculate level, archetype, attributes
   → Create NPC entities with leash to engagement location
   → Track NPC entities in engagement.spawned_npcs

3. Monitor engagement
   → Track NPC deaths (remove from spawned_npcs)
   → Track player proximity

4. Cleanup conditions
   → All NPCs killed → despawn engagement entity, free budget
   → No players within 100 tiles for 60s → despawn all, free budget
```

---

### Engagement Spawning Logic

```rust
fn spawn_engagement_at(location: Qrz) {
    // Calculate level from distance to haven
    let level = calculate_enemy_level(location, HAVEN_LOCATION);

    // Determine archetype from directional zone
    let zone = get_directional_zone(location, HAVEN_LOCATION);
    let archetype = EnemyArchetype::from_zone(zone);

    // Random group size (1-3 NPCs)
    let npc_count = rand::gen_range(1..=3);

    // Calculate zone for budget tracking
    let zone_id = ZoneId::from_position(location);

    // Spawn engagement parent entity
    let engagement_entity = commands.spawn((
        Engagement {
            spawn_location: location,
            level,
            archetype,
            npc_count,
            spawned_npcs: Vec::new(),
            zone_id,
        },
        Loc(location),
    )).id();

    // Spawn NPCs as children
    for i in 0..npc_count {
        // Calculate attributes based on level and archetype
        let attributes = calculate_enemy_attributes(level, archetype);

        // Get ability and AI behavior for archetype
        let ability = archetype.ability();
        let behavior = archetype.ai_behavior();

        // Spawn NPC slightly offset from engagement center
        let offset = random_hex_offset(i);
        let npc_location = location + offset;

        let npc_entity = commands.spawn((
            Actor,
            attributes,
            Loc(npc_location),
            AbilityBar::from_ability(ability),
            behavior,
            Leash {
                origin: location,
                max_distance: 15, // Leashed to engagement spawn
            },
            EngagementMember(engagement_entity), // Back-reference
        )).id();

        // Track NPC in engagement
        engagement.spawned_npcs.push(npc_entity);
    }

    // Play forming sound (3s warning)
    play_sound_at_location(location, "engagement_forming.ogg", 3.0);
}
```

---

### Budget Management System

**Zone-Based Tracking:**
```rust
#[derive(Resource, Default)]
pub struct EngagementBudget {
    /// Map of zone_id → count of active engagements
    active_per_zone: HashMap<ZoneId, usize>,
}

impl EngagementBudget {
    pub fn can_spawn_in_zone(&self, zone_id: ZoneId) -> bool {
        self.active_per_zone.get(&zone_id).unwrap_or(&0) < &MAX_ENGAGEMENTS_PER_ZONE
    }

    pub fn register_engagement(&mut self, zone_id: ZoneId) {
        *self.active_per_zone.entry(zone_id).or_insert(0) += 1;
    }

    pub fn unregister_engagement(&mut self, zone_id: ZoneId) {
        if let Some(count) = self.active_per_zone.get_mut(&zone_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.active_per_zone.remove(&zone_id);
            }
        }
    }
}
```

**Budget Update Points:**
- **Spawn**: `budget.register_engagement(zone_id)` when engagement created
- **Cleanup**: `budget.unregister_engagement(zone_id)` when engagement despawned

---

### Cleanup System

**Two Cleanup Conditions:**

**Condition 1: All NPCs Killed**
```rust
fn cleanup_completed_engagements(
    mut commands: Commands,
    mut budget: ResMut<EngagementBudget>,
    engagements: Query<(Entity, &Engagement)>,
) {
    for (entity, engagement) in engagements.iter() {
        // Check if all NPCs are dead (entities despawned)
        let all_dead = engagement.spawned_npcs.iter()
            .all(|&npc| commands.get_entity(npc).is_none());

        if all_dead {
            // Despawn engagement entity
            commands.entity(entity).despawn();

            // Free budget slot
            budget.unregister_engagement(engagement.zone_id);
        }
    }
}
```

**Condition 2: No Players Nearby**
```rust
fn cleanup_abandoned_engagements(
    mut commands: Commands,
    mut budget: ResMut<EngagementBudget>,
    engagements: Query<(Entity, &Engagement, &Loc)>,
    players: Query<&Loc, With<Player>>,
    time: Res<Time>,
) {
    const ABANDONMENT_DISTANCE: u32 = 100;
    const ABANDONMENT_TIME: f32 = 60.0; // seconds

    for (entity, engagement, loc) in engagements.iter() {
        // Check if any player within 100 tiles
        let has_nearby_player = players.iter()
            .any(|player_loc| loc.0.distance_to(player_loc.0) <= ABANDONMENT_DISTANCE);

        if !has_nearby_player {
            // Track time since last player nearby
            // (Needs additional component for tracking)
            // If > 60s, despawn engagement and all NPCs

            for &npc in &engagement.spawned_npcs {
                commands.entity(npc).despawn();
            }
            commands.entity(entity).despawn();
            budget.unregister_engagement(engagement.zone_id);
        }
    }
}
```

---

### Integration with Existing Systems

**Chunk Management:**
- Hook `send_chunk_to_player()` to call `try_spawn_engagement()`
- No changes to chunk loading logic itself

**Leashing (Existing):**
- Engagements reuse existing `Leash` component
- NPCs leashed to engagement spawn location (15 tile radius)
- No new leash logic needed

**AI Behavior (Existing):**
- Engagements use existing `AIBehaviorType` (Chase/Kite)
- Behavior trees unchanged
- AI naturally respects leash boundaries

**Audio System:**
- Play "forming" sound at engagement location
- 3-second delay before NPCs become active (minimal feedback for Phase 1)

**Level/Archetype Calculation (This ADR):**
- Reuses `calculate_enemy_level()` and `calculate_enemy_attributes()`
- Reuses `get_directional_zone()` and `EnemyArchetype::from_zone()`
- No changes to attribute/level systems

---

### Static Spawner Migration

**Deprecation Plan:**
- Phase 1: Implement dynamic engagements alongside static spawners
- Test dynamic system thoroughly
- Phase 2: Remove static spawner system entirely
- Delete `StaticSpawner` component and related systems

**Why Not Remove Immediately:**
- Allows A/B comparison during development
- Fallback if dynamic system needs iteration
- Clean migration path

---

## Implementation Plan

### Phase 1: Core Infrastructure + Dynamic Engagements (4-5 hours)

**Goal:** Level calculation, attribute distribution, archetype definitions, dynamic engagement spawning system

**Part A: Spatial Difficulty Core (1.5-2 hours)**

1. **Create spatial difficulty module** (`src/common/spatial_difficulty.rs`)
   - `HAVEN_LOCATION` constant
   - `calculate_enemy_level()` function
   - `DirectionalZone` enum + `get_directional_zone()`
   - `EnemyArchetype` enum + methods
   - `AttributeDistribution` system
   - `calculate_enemy_attributes()` function

**Part B: Dynamic Engagement System (2.5-3 hours)**

2. **Create engagement components** (`src/common/components/engagement.rs`)
   - `Engagement` component (parent entity)
   - `EngagementMember` component (back-reference for NPCs)
   - `ZoneId` type for budget tracking
   - Lifecycle tracking (spawned NPCs list)

3. **Create engagement budget resource** (`src/server/resources/engagement_budget.rs`)
   - `EngagementBudget` resource (HashMap<ZoneId, usize>)
   - `can_spawn_in_zone()`, `register_engagement()`, `unregister_engagement()`
   - Zone calculation logic

4. **Create engagement spawning system** (`src/server/systems/engagement_spawner.rs`)
   - Hook chunk send event (`send_chunk_to_player`)
   - `try_spawn_engagement()` validation pipeline
   - `spawn_engagement_at()` - create engagement + NPCs
   - Spawn probability (50%), distance checks (30/50 tiles)
   - Random group size (1-3), level/archetype calculation
   - Play forming sound

5. **Create engagement cleanup system** (`src/server/systems/engagement_cleanup.rs`)
   - `cleanup_completed_engagements()` - all NPCs killed
   - `cleanup_abandoned_engagements()` - no players nearby for 60s
   - Budget deregistration

6. **Hook chunk management** (modify existing chunk send code)
   - Call `try_spawn_engagement()` when sending chunk to player
   - Integration point in chunk networking

**Test Criteria:**
- ✅ Level calculation: Distance 500 tiles = level 5 enemies
- ✅ Archetype assignment: North = Berserkers, East = Juggernauts, South = Kiters, West = Defenders
- ✅ Attribute distribution: Berserker level 5 has -3 Might, -2 Instinct (alternating allocation)
- ✅ Engagement spawning: Chunk receipt triggers 50% spawn chance
- ✅ Budget enforcement: Max 5 engagements per 500-tile zone
- ✅ Distance checks: No spawns within 30 tiles of players, 50 tiles of other engagements
- ✅ Group size: Random 1-3 NPCs per engagement
- ✅ Cleanup: Engagement despawns when all NPCs killed
- ✅ Abandonment: Engagement despawns after 60s with no players within 100 tiles

---

### Phase 2: Counter Ability (1-2 hours)

**Goal:** Implement Counter ability and replace Knockback

**Existing Abilities (no work required):**
- ✅ Volley already exists and works with Kite AI
- ✅ Lunge already exists
- ✅ Overpower already exists

**New Work:**

1. **Implement Counter** (`src/server/systems/combat/abilities/counter.rs`)
   - **Same ability type as Knockback** (preserves synergy with Overpower)
   - ReactionQueue validation (has entries in front)
   - Pop front threat from caster's queue
   - Negate threat's damage completely (caster takes 0 damage)
   - Queue 50% of threat's damage as new threat in origin entity's ReactionQueue
   - Recovery integration (ADR-012)

2. **Replace Knockback with Counter**
   - Rename `AbilityKey::Knockback` → `AbilityKey::Counter`
   - Delete `knockback.rs` file
   - Update ability bar assignments (Counter replaces Knockback)
   - ADR-012 synergy preserved: Overpower → Counter (same ability type)

**Test Criteria:**
- Counter only usable when ReactionQueue has entries, fails otherwise
- Counter pops front threat from caster's queue (not just any threat)
- Counter fully negates threat damage (caster takes 0)
- Counter queues 50% of threat damage to origin entity's ReactionQueue (not direct damage)
- Reflected threat appears in origin's queue and can be reacted to
- Counter preserves Overpower synergy (same ability type as Knockback)
- Knockback enum renamed to Counter, file deleted
- Volley works with Kiter archetype (already working)

---

### Phase 3: AI Integration (MINIMAL - ~30 minutes)

**Goal:** Use existing AI behaviors with new archetypes

**Changes Required:**
1. **Assign AI behavior to spawned NPCs** (already done in spawner.rs)
   - Berserker/Juggernaut/Defender → existing Chase AI
   - Kiter → existing Kite AI (Forest Sprite behavior)

2. **Update Counter ability validation** (in counter.rs)
   - Check ReactionQueue has entries before use
   - If queue empty, fail (AI will retry later)

**NO NEW AI WORK REQUIRED:**
- Chase AI already exists ✓
- Kite AI already exists (Forest Sprite) ✓
- Ability usage nodes already exist ✓

**Test Criteria:**
- Berserkers chase and use Lunge
- Juggernauts chase and use Overpower
- Kiters maintain distance and use Volley (like Forest Sprite)
- Defenders chase and use Counter when ReactionQueue has threats

---

### Phase 4: UI and Feedback (1-2 hours)

**Goal:** Players understand spatial difficulty

1. **Distance indicator UI** (`src/client/ui/distance_indicator.rs`)
   - Show distance from haven
   - Show current zone (North/East/South/West)
   - Show expected enemy level

2. **Enemy nameplate enhancements**
   - Show enemy level
   - Show archetype name
   - Color-code by difficulty (relative to player)

3. **Danger warnings** (optional)
   - Screen border color based on zone level
   - Audio cues for high-level zones

**Test Criteria:**
- UI shows accurate distance and zone
- Enemy nameplates show level and archetype
- New players can identify safe/dangerous zones

---

## File Structure

```
src/
├── common/
│   ├── components/
│   │   ├── ai_behavior.rs [NEW - AIBehavior component]
│   │   └── engagement.rs [NEW - Engagement, EngagementMember, ZoneId]
│   │
│   └── spatial_difficulty.rs [NEW]
│       ├── HAVEN_LOCATION
│       ├── calculate_enemy_level()
│       ├── DirectionalZone enum
│       ├── EnemyArchetype definitions
│       ├── AttributeDistribution system
│       └── calculate_enemy_attributes()
│
├── server/
│   ├── resources/
│   │   └── engagement_budget.rs [NEW - Zone budget tracking]
│   │
│   └── systems/
│       ├── engagement_spawner.rs [NEW - Dynamic engagement spawning]
│       ├── engagement_cleanup.rs [NEW - Cleanup completed/abandoned]
│       ├── spawner.rs [DEPRECATED - To be removed in Phase 2]
│       │
│       └── combat/abilities/
│           ├── volley.rs [EXISTING - Kiter uses this]
│           ├── counter.rs [NEW - Defender primary, replaces Knockback]
│           └── knockback.rs [DELETE]
│
└── client/ui/
    └── distance_indicator.rs [NEW - Show zone/level]
```

---

## Consequences

### Positive

✅ **Proper attribute usage** - Enemies use existing attribute system correctly

✅ **Meaningful differentiation** - Each archetype feels mechanically distinct

✅ **Smooth difficulty curve** - Linear level scaling (100 tiles per level)

✅ **Tactical variety** - Four distinct combat puzzles (chase melee, tank, kiter, counter)

✅ **Self-directed difficulty** - Players choose challenge by exploration

✅ **Accessibility** - New players stay near haven (level 0-2 enemies)

✅ **Future-proof** - Clean migration to full haven/hub/siege systems

✅ **Exploration foundation** - Distance matters, ready for loot/XP integration

✅ **Removes broken content** - Knockback eliminated

✅ **AI diversity** - Existing Chase/Kite behaviors create different encounters

✅ **Minimal AI work** - Reuses existing Forest Sprite kite behavior

✅ **Minimal ability work** - Volley already exists, only Counter needed

✅ **Exploration-driven content** - Dynamic engagements reward exploration (anti-farming)

✅ **Self-paced encounters** - Players control engagement rate through exploration speed

✅ **Fair spawning** - Multi-stage validation prevents unfair surprise spawns

✅ **Bounded density** - Budget system prevents overwhelming world

✅ **Automatic cleanup** - Engagements despawn when complete/abandoned (no manual cleanup)

✅ **Reuses existing systems** - Leash, AI behaviors, chunk management (minimal new code)

### Negative

⚠️ **Increased implementation scope** - 6.5-8 hours total (engagement system + Counter ability)

⚠️ **New systems required** - Engagement entities, budget tracking, cleanup systems

⚠️ **One new ability required** - Counter must work correctly

⚠️ **Attribute calculation subtlety** - Alternating allocation requires careful implementation

⚠️ **Spec deviation** - Not full haven system (deferred to post-MVP)

⚠️ **Knockback removal** - Content cut (may affect other systems)

⚠️ **Balancing challenge** - Four archetypes need tuning for fun combat

⚠️ **Attribute calculation complexity** - Berserker alternating allocation is subtle

⚠️ **Chunk integration coupling** - Ties engagement spawning to chunk networking (new dependency)

⚠️ **Budget tuning needed** - 5 per zone, 50% spawn rate need playtesting for balance

⚠️ **Abandonment tracking complexity** - Time-based despawn requires additional component/tracking

⚠️ **Static spawner deprecation** - Existing system becomes dead code (cleanup needed later)

### Neutral

🔹 **Commits to level-based design** - Enemies have levels (prepares for player leveling)

🔹 **Directional zones arbitrary** - No lore justification yet (can add later)

🔹 **Counter requires ReactionQueue** - Ties defender archetype to reaction system

🔹 **Kite AI uses existing behavior** - Forest Sprite AI reused for Kiter archetype

🔹 **Exploration-dependent content** - Players standing still won't find engagements (by design)

🔹 **Zone-based budget** - Creates spatial variance in engagement density (not uniform)

🔹 **Chunk boundary spawns** - Engagements can appear in "previously explored" areas (dynamic world)

---

## Open Questions

**Difficulty Tuning:**
- Is 100 tiles per level correct pacing? (Too slow/fast?)
- Should level cap at 10, or scale infinitely?
- Do level 0 enemies need minimum stats? (Too weak to be interesting?)

**Archetype Balance:**
- Is Volley damage (30) balanced vs Lunge (40)?
- Is Counter's 50% reflection balanced? (Too strong/weak?)
- Are single-ability archetypes interesting enough? (No ability variety per enemy)

**AI Behavior:**
- Should Kite AI flee when low health? (More complex behavior tree)
- Should Defenders only use Counter when ReactionQueue has threats? (Currently yes)
- Does Counter work correctly with existing Chase AI behavior nodes?

**Attribute Distribution:**
- Is Berserker alternating allocation clear? (Might on odd, Instinct on even)
- Should spectrum be non-zero for NPCs? (Tactical flexibility?)
- Do derived stats from attributes provide enough differentiation?

**Player Feedback:**
- Is directional archetype assignment intuitive? ("Why are North enemies different?")
- Should zones rotate/randomize on server restart?
- Do players discover archetypes naturally, or need tutorial?

**Dynamic Engagement System:**
- Is 50% spawn probability correct? (Too many/few engagements?)
- Is zone budget (5 per 500 tiles) balanced? (Too dense/sparse?)
- Are distance checks (30/50 tiles) appropriate? (Too safe/dangerous?)
- Should abandoned timer be 60s? (Too short/long?)
- Do players understand chunk-triggered spawning? (Need tutorial/feedback?)
- Should engagements persist across server restarts? (Currently no)
- Can players "game" chunk boundaries to farm spawns? (Exploit potential?)
- Does engagement cleanup feel fair? (Or frustrating if NPCs disappear?)

**Future Integration:**
- When adding player leveling, how to handle level disparity? (Level 5 player vs level 10 enemy?)
- Should player hubs affect nearby enemy levels? (Haven spec says yes, defer?)
- Do dynamic spawners use same formula? (Event spawns, boss spawns?)
- Should mixed-archetype engagements be added? (2 Berserkers + 1 Juggernaut?)
- Can static points of interest use engagement system? (Dungeons, ruins, etc.?)

---

## Alternatives Considered

### Alt 1: Stat Multiplier System (Original ADR-014 Draft)

**Approach:** Scale base stats (health, damage) by distance multiplier

**Rejected:**
- Doesn't use attribute system properly
- All enemies feel same, just stronger/weaker
- No tactical variety

### Alt 2: Random Archetype Assignment

**Approach:** Each spawner randomly picks archetype

**Rejected:**
- No spatial coherence (immersion-breaking)
- Can't self-select preferred combat style
- Doesn't prepare for haven directional design

### Alt 3: Keep Knockback, Don't Add New Abilities

**Approach:** Use existing 4 abilities, simple stat scaling

**Rejected:**
- User explicitly requested Knockback removal ("not working well")
- Can't create ranged kiter archetype without Volley
- Defender archetype needs reactive ability (Counter)

### Alt 4: Complex AI (Multiple States)

**Approach:** AI with multiple states (aggressive, defensive, fleeing)

**Rejected:**
- Overkill for combat prototyping phase
- Simple chase/kite sufficient for archetype differentiation
- Can add complexity later when needed

### Alt 5: Static Spawners with Level Scaling

**Approach:** Keep existing static spawner system, just add level/archetype calculation based on spawner location

**Rejected:**
- Creates farmable spawn points (anti-exploration)
- Conflicts with spatial difficulty goals (players camp closest spawn)
- Doesn't reward exploration (static world)
- Harder to balance density (manually place spawners)
- Doesn't prepare for haven system (expects dynamic world)

**Why Dynamic Engagements Instead:**
- Exploration-driven content discovery (primary design goal)
- Self-paced encounter rate (player controls difficulty through movement)
- No farming locations (anti-grind)
- Automatic cleanup (no stale spawns)
- Bounded density through budget system (not manual placement)
- Aligns with haven/hub/siege system vision (dynamic world)

---

## References

**Spec Documents:**
- [haven-system.md](../spec/haven-system.md) - Full haven system design (this ADR is MVP subset)
- [siege-system.md](../spec/siege-system.md) - Encroachment/anger calculations
- [attribute-system.md](../spec/attribute-system.md) - Attribute pairs and derived stats
- [haven-system-feature-matrix.md](../spec/haven-system-feature-matrix.md) - Implementation tracking

**Related ADRs:**
- ADR-002: Combat Foundation - Base combat systems
- ADR-003: Reaction Queue System - Counter ability integration
- ADR-006: AI Behavior and Ability Integration - AI behavior trees
- ADR-009: MVP Ability Set - Existing abilities
- ADR-012: Ability Recovery and Synergies - Recovery system (Knockback synergy removed)

**Current Implementations:**
- [spawner.rs](../../src/server/systems/spawner.rs) - Spawner system to modify
- [ActorAttributes](../../src/common/components/mod.rs) - Attribute system
- [abilities/](../../src/server/systems/combat/abilities/) - Ability implementations

---

**Document Version:** 2.1
**Created:** 2025-11-07
**Last Updated:** 2025-11-07
**Author:** ARCHITECT
**Spec Deviation:** Yes - Simplified MVP of haven-system.md for combat prototyping
**Major Changes from v2.0:** Added Dynamic Engagement System (chunk-triggered spawning, budget system, cleanup lifecycle) - replaces static spawners
**Major Changes from v1.0:** Level-based system with attribute distributions, new abilities (Volley/Counter), Knockback removal, AI behavior types
