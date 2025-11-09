# RFC-014: Spatial Difficulty System (Level-Based Enemy Variety)

## Status

**Implemented** - 2025-11-08 (originally proposed 2025-11-07)

## Feature Request

### Player Need

From player perspective: **Meaningful exploration with self-directed difficulty** - Different locations should present different challenges, and players should choose their comfort zone.

**Current Problem:**
Without spatial difficulty variation:
- All enemies are identical (level 10, same stats, same abilities)
- No reason to explore (every location mechanically identical)
- Can't self-select challenge level (combat too hard/easy for everyone)
- Static spawners create farmable locations (anti-exploration)
- Knockback ability not working well (needs replacement)
- Attribute system underutilized (all enemies same build)

**We need a system that:**
- Creates spatial difficulty gradient (safe near spawn, dangerous far away)
- Enables self-directed challenge selection (go where comfortable)
- Rewards exploration with variety (different enemies in different directions)
- Uses existing attribute system properly (enemies have distinct builds)
- Prevents farming (dynamic spawning, not static locations)
- Provides tactical variety (ranged, melee, defensive, aggressive archetypes)

### Desired Experience

Players should experience:
- **Safe Haven:** Spawn area has weak/no enemies (tutorial zone)
- **Difficulty Gradient:** Enemies get stronger with distance (100 tiles per level)
- **Directional Variety:** North enemies fight differently than East/South/West
- **Tactical Adaptation:** Each archetype requires different combat approach
- **Exploration Rewards:** Moving finds new engagements (anti-camping)
- **Self-Paced Combat:** Control encounter rate through exploration speed
- **Fair Spawning:** Never surprised by spawns on top of you

### Specification Requirements

**MVP Spatial Difficulty System:**

**1. Level-Based Scaling:**
- Single haven at world origin (0, 0) as safe spawn point
- Enemy level scales with distance: every 100 tiles = 1 level (0-10 range)
- Each level = 1 attribute point distributed to archetype's chosen axes
- Linear difficulty curve (smooth gradient, no spikes)

**2. Directional Archetypes:**
- Four compass directions spawn different enemy types:
  - **North (Berserker):** Aggressive melee burst (Might/Instinct, uses Lunge)
  - **East (Juggernaut):** Tanky melee pressure (Vitality/Presence, uses Overpower)
  - **South (Kiter):** Ranged harassment (Grace/Focus, uses Volley)
  - **West (Defender):** Reactive counter-attacks (Focus/Instinct, uses Counter)
- Each archetype has ONE signature ability
- Distinct attribute distributions create different combat profiles

**3. Attribute Distribution:**
- Each archetype allocates points to two attributes (alternating by level):
  - Berserker: -Might (odd levels), -Instinct (even levels)
  - Juggernaut: -Vitality (odd levels), +Presence (even levels)
  - Kiter: +Grace (odd levels), +Focus (even levels)
  - Defender: +Focus (odd levels), -Instinct (even levels)
- Derived stats (HP, damage, movement speed, armor) scale automatically

**4. Dynamic Engagement Spawning:**
- Chunk receipt triggers spawn chance (50% probability)
- Multi-stage validation:
  - Zone budget (max 5 engagements per 500-tile zone)
  - Player proximity (min 30 tiles from any player)
  - Engagement spacing (min 50 tiles from other engagements)
- Engagement = parent entity with 1-3 NPCs (random group size)
- NPCs leashed to engagement location (15 tile radius)
- Cleanup conditions: All NPCs killed OR no players within 100 tiles for 60s

**5. New Abilities:**
- **Volley (Kiter):** Already exists, ranged projectile (3-6 hex range, 30 damage)
- **Counter (Defender):** NEW - Replaces Knockback
  - Pop front threat from own ReactionQueue
  - Negate threat damage (take 0)
  - Queue 50% reflected damage to attacker's ReactionQueue
  - Costs 30 stamina, 1.2s recovery

**6. AI Behaviors:**
- Berserker/Juggernaut/Defender: Existing Chase AI (close to melee, use ability)
- Kiter: Existing Kite AI (Forest Sprite behavior, maintain distance 3-6 hexes)

### MVP Scope

**Phase 1 includes:**
- Haven spawn at origin (no safe zone mechanics, just location)
- Distance-based level calculation (100 tiles per level, 0-10 range)
- Directional zone determination (North/East/South/West by angle)
- Four enemy archetypes with attribute distributions
- Dynamic engagement spawning (chunk-triggered, budget-managed)
- Engagement cleanup (completion + abandonment)
- Counter ability implementation
- Knockback ability removal
- AI behavior assignment (reuse existing Chase/Kite)

**Phase 1 excludes:**
- Multiple havens (spec has 3, MVP has 1)
- Influence radius system (using direct distance calculation)
- Encroachment formula (using simple distance scaling)
- Siege mechanics (no anger, waves, destruction)
- Biome-specific placement (spawn at world origin only)
- Player leveling (all players static level 10)
- Loot system (no extrinsic exploration rewards)
- Mixed-archetype engagements (each engagement one archetype only)

### Priority Justification

**HIGH PRIORITY** - Enables combat variety testing, validates attribute system, creates exploration foundation.

**Why high priority:**
- Combat prototyping needs variety (can't test with identical enemies)
- Attribute system validation (use it properly before adding more systems)
- Self-directed difficulty (accessibility for new/struggling players)
- Exploration foundation (distance matters, ready for loot/XP later)
- Removes broken content (Knockback not working well)

**Benefits:**
- Tactical variety (four distinct combat puzzles)
- Smooth difficulty curve (linear scaling, no spikes)
- Exploration-driven content (anti-farming)
- Future-proof (clean migration to full haven/hub/siege systems)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Haven + Distance Scaling + Directional Archetypes + Dynamic Spawning**

#### Core Mechanism

**Level Calculation:**
```rust
pub fn calculate_enemy_level(spawn_location: Qrz, haven_location: Qrz) -> u8 {
    let distance = haven_location.distance_to(spawn_location) as f32;
    (distance / 100.0).min(10.0) as u8  // 100 tiles per level, cap at 10
}
```

**Directional Zone:**
```rust
pub fn get_directional_zone(spawn_location: Qrz, haven_location: Qrz) -> DirectionalZone {
    let angle = atan2(delta.y, delta.x).to_degrees();
    // North (315°-45°), East (45°-135°), South (135°-225°), West (225°-315°)
}
```

**Attribute Distribution:**
- Alternating allocation (odd levels → axis1, even levels → axis2)
- Direction multiplier (-1 for left attribute, +1 for right)
- Example: Berserker level 5 = -3 Might (levels 1,3,5), -2 Instinct (levels 2,4)

**Dynamic Spawning:**
1. Hook chunk send event → try_spawn_engagement()
2. Validate: 50% probability, zone budget, player distance, engagement spacing
3. Spawn engagement entity + 1-3 NPCs (random)
4. Calculate level/archetype, apply attributes, assign AI
5. Cleanup: All NPCs killed OR abandoned for 60s

**Counter Ability:**
- Pop front threat from own ReactionQueue
- Negate damage (take 0)
- Queue 50% to attacker (reflection mechanic)

#### Performance Projections

**Level Calculation:**
- Runs once per NPC spawn (not every frame)
- Simple math (distance / 100)
- Overhead: negligible

**Attribute Distribution:**
- Runs once per NPC spawn
- Alternating allocation formula (integer division)
- Overhead: negligible

**Dynamic Spawning:**
- Runs on chunk receipt (not every frame)
- Multi-stage validation (probability, budget, distances)
- Zone budget tracking (HashMap lookup)
- Overhead: ~0.1ms per chunk receipt

**Engagement Cleanup:**
- Runs periodically (every few seconds)
- Checks NPC death, player proximity
- Overhead: ~0.1ms per cleanup tick

**Development Time:**
- Phase 1 (Core + Engagements): 4-5 hours
- Phase 2 (Counter Ability): 1-2 hours
- Phase 3 (AI Integration): 0.5 hours
- Phase 4 (UI Feedback): 1-2 hours
- **Total: 6.5-9.5 hours**

#### Technical Risks

**1. Engagement Spawning Balance**
- *Risk:* 50% spawn rate, 5 per zone budget might be too dense/sparse
- *Mitigation:* Playtest and tune constants (easy to adjust)
- *Impact:* Balancing issue, not technical blocker

**2. Attribute Distribution Complexity**
- *Risk:* Alternating allocation subtle to implement correctly
- *Mitigation:* Comprehensive unit tests, example progressions
- *Impact:* One-time implementation complexity

**3. Counter Ability Balance**
- *Risk:* 50% reflection might be too strong/weak
- *Mitigation:* Playtest, adjust percentage if needed
- *Impact:* Balancing issue, not technical blocker

**4. Abandonment Tracking**
- *Risk:* Time-based despawn requires additional component/tracking
- *Mitigation:* Add `LastPlayerNearby` component to engagements
- *Impact:* Minor additional complexity

### System Integration

**Affected Systems:**
- Spawning (dynamic engagements replace static spawners)
- Attributes (proper distribution per archetype)
- Abilities (Counter replaces Knockback)
- AI (behavior assignment per archetype)
- Chunk management (hook send event for spawn triggers)
- Reaction queue (Counter integrates with queue)

**Compatibility:**
- ✅ Uses existing attribute system (ActorAttributes)
- ✅ Uses existing AI behaviors (Chase, Kite)
- ✅ Uses existing abilities (Lunge, Overpower, Volley)
- ✅ Uses existing leash system (engagement leashing)
- ✅ Uses existing reaction queue (Counter integration)

### Alternatives Considered

#### Alternative 1: Stat Multiplier System

Scale base stats (HP, damage) by distance multiplier.

**Rejected because:**
- Doesn't use attribute system properly
- All enemies feel same, just stronger/weaker
- No tactical variety

#### Alternative 2: Random Archetype Assignment

Each spawn randomly picks archetype.

**Rejected because:**
- No spatial coherence (immersion-breaking)
- Can't self-select preferred combat style
- Doesn't prepare for haven directional design

#### Alternative 3: Static Spawners with Level Scaling

Keep existing spawners, add level/archetype calculation.

**Rejected because:**
- Creates farmable locations (anti-exploration)
- Conflicts with spatial difficulty goals
- Doesn't reward exploration (static world)

#### Alternative 4: Keep Knockback

Don't add Counter, use existing abilities.

**Rejected because:**
- User explicitly requested Knockback removal ("not working well")
- Can't create reactive defender archetype without Counter

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Spatial difficulty + dynamic spawning creates exploration-driven content without loot/XP systems. Intrinsic reward (self-directed challenge) provides immediate value.

**Haven system deviation justified:** Full haven/hub/siege system ~40+ hours. MVP spatial difficulty (6.5-9.5 hours) enables combat prototyping NOW, provides clean migration path later.

**Attribute system validation:** This properly uses the existing attribute system for enemy differentiation (current implementation wastes it with uniform enemies).

**Extensibility:**
- Future: Multiple havens (Mountain, Prairie, Forest)
- Future: Influence radius system (replace direct distance)
- Future: Player-created hubs affect nearby difficulty
- Future: Mixed-archetype engagements
- Future: Loot/XP rewards (plug into existing level system)

### PLAYER Validation

**From haven-system.md spec:**

**Retained Concepts:**
- ✅ Haven as safe spawn zone (spec Line 27-29)
- ✅ Distance = difficulty curve (spec Lines 96-104 territory zones)
- ✅ Foundation for future expansion (spec Lines 106-149 progression timeline)

**Deviations (Intentional MVP Simplification):**
- ❌ Single haven instead of three
- ❌ No influence radius system
- ❌ No encroachment formula
- ❌ No siege mechanics

**Success Criteria:**
- Spatial variation (different locations = different enemies)
- Self-directed difficulty (players choose challenge by location)
- Tactical variety (four distinct archetypes require adaptation)
- Exploration value (distance matters mechanically)

---

## Approval

**Status:** ✅ Implemented and merged to main

**Approvers:**
- ARCHITECT: ✅ Level scaling solid, dynamic spawning prevents farming, attribute usage proper, clean migration to full haven system
- PLAYER: ✅ Creates variety, self-directed difficulty, exploration foundation

**Scope Constraint:** Fits in one SOW (6.5-9.5 hours for 4 phases)

**Dependencies:**
- ActorAttributes system (existing)
- Chase/Kite AI (existing)
- Volley ability (existing)
- Reaction queue system (Counter integration)
- Chunk management (spawn trigger hook)

**Implementation:**
- **SOW:** [SOW-014: Spatial Difficulty System](../03-sow/014-spatial-difficulty-system.md)
- **Branch:** `adr-014-spatial-difficulty-system` (merged 2025-11-08)
- **Test Results:** 240/240 passing
- **Architecture Grade:** A+ (Excellent)

**Date Proposed:** 2025-11-07
**Date Implemented:** 2025-11-08
