# ADR-010: Combat Variety Phase 1 - Ranged Enemies, Tier Lock, and Movement Speed

## Status

Proposed (2025-11-03)

## Context

### Current System State

Based on accepted ADRs:

1. **ADR-002: Combat Foundation** - Resources, combat state, attribute scaling
2. **ADR-003: Reaction Queue System** - Threat queueing, timer resolution
3. **ADR-004: Ability System and Targeting** - Directional targeting framework
4. **ADR-005: Damage Pipeline** - Damage calculation, server authority
5. **ADR-006: AI Behavior** - Enemy behaviors, Wild Dog melee enemy
6. **ADR-009: MVP Ability Set** - Auto-attack, Lunge, Overpower, Knockback, Deflect

### Game Design Requirements

**From combat-system.md:**
- Tier lock targeting (1/2/3 keys) - Lines 85-106
- Ranged enemy (Forest Sprite) with kiting behavior - Lines 482-490
- Projectile attacks with travel time - Lines 151-161

**From attribute-system.md:**
- Movement speed scales with Grace attribute - Lines 338-347
- Formula: `movement_speed = max(75, 100 + (grace / 2))`
- Creates mobility trade-offs between Might specialists and Grace specialists

### Problem: Limited Combat Variety

**Current MVP State:**
- Only one enemy type (Wild Dog - melee aggressor)
- No ranged combat patterns
- Tier lock targeting partially implemented (framework exists, but no keybindings)
- All players move at identical speed (no Grace scaling)
- Combat encounters feel repetitive

**Player Experience Issues:**
1. **Targeting frustration:** Can't easily select distant targets when multiple enemies nearby
2. **Combat sameness:** All enemies are melee, no ranged threat variety
3. **No mobility differentiation:** Grace attribute feels useless (no movement speed benefit)
4. **Tactical depth missing:** No reason to use positioning beyond "stay in melee or kite"

### Design Constraints

**Technical Constraints:**
- Targeting framework from ADR-004 exists but tier lock not bound to keys
- Enemy AI framework from ADR-006 supports behavior composition
- No projectile system implemented yet (instant abilities only)
- Movement timing currently fixed (125ms per hex)

**Player Experience Constraints:**
- Must validate ranged combat before adding more enemies
- Must make Grace attribute feel valuable
- Must not overcomplicate targeting (tier lock should feel natural)
- Must maintain "conscious but decisive" combat philosophy

## Decision

We will implement **three interconnected features** that enhance combat variety and tactical depth:

### Feature 1: Tier Lock Targeting (1/2/3 Keys)

Complete the targeting system from ADR-004 with number key tier locks.

**Mechanics:**
- **1 Key:** Lock to Close tier (1-2 hexes)
- **2 Key:** Lock to Mid tier (3-6 hexes)
- **3 Key:** Lock to Far tier (7+ hexes)
- **Lock Duration:** Drops after 1 ability use (returns to automatic targeting)
- **Empty Tier Behavior:** Lock remains active, shows visual range indicator, switches to target immediately when enemy enters tier
- **Visual Feedback:**
  - Tier badge on target indicator (small "1", "2", or "3" icon)
  - Empty tier: Highlight facing cone range at locked tier distance
  - Clear visual distinction between auto-target and manual tier lock

**State Machine:**
```
Default (Auto) → Press 1/2/3 → Tier Locked
Tier Locked → Use Any Ability → Default (Auto)
Tier Locked → Press Different Number → New Tier Locked
Tier Locked → Target Dies/Invalid → Stay Locked (search for new target in tier)
```

**Component Structure:**
```rust
pub struct TargetingState {
    pub mode: TargetingMode,
    pub last_target: Option<Entity>,
}

pub enum TargetingMode {
    Automatic,                    // Default: nearest in facing direction
    TierLocked(RangeTier),       // Locked to specific tier until ability use
    ManualLocked(Entity),        // TAB-selected (future feature)
}

pub enum RangeTier {
    Close,   // 1-2 hexes
    Mid,     // 3-6 hexes
    Far,     // 7+ hexes
}
```

**Integration Points:**
- **Input System:** Bind 1/2/3 keys to `Event::TrySetTierLock(RangeTier)`
- **Targeting System:** Query `TargetingState`, filter targets by tier, apply geometric tiebreaker
- **Ability System:** On ability execution, reset `TargetingState.mode` to `Automatic`
- **UI System:** Render tier badge, highlight empty tier ranges

### Feature 2: Forest Sprite (Ranged Enemy)

Add a second enemy type with ranged projectile attacks and kiting AI.

**Entity Stats:**
- **HP:** 80 (lower than Wild Dog's 100 - glass cannon)
- **Damage:** 20 physical (higher than Wild Dog's 15)
- **Attack Speed:** 3 seconds (slower than Wild Dog's 1 second)
- **Aggro Range:** 15 hexes (longer than Wild Dog's 10 hexes)
- **Optimal Distance:** 5-8 hexes (kiting zone)
- **Disengage Distance:** 3 hexes (too close threshold)

**AI Behavior Pattern:**
```
1. Detect player within 15 hexes → Face player
2. Measure distance to player:
   - If distance < 3 hexes: FLEE (move away while maintaining facing)
   - If distance 3-5 hexes: REPOSITION (move to 6-7 hex range)
   - If distance 5-8 hexes: ATTACK (projectile every 3s)
   - If distance > 8 hexes: ADVANCE (move closer while maintaining facing)
3. If distance > 30 hexes: LEASH (return to spawn, reset aggro)
```

**Projectile Mechanics:**
- **Travel Speed:** 4 hexes/second (slower than instant, fast enough to be threatening)
- **Targeting:** Snapshot player position at cast time
- **Hit Detection:** Damages entities at projectile position when it arrives
- **Dodgeable:** Player can move off targeted hex during travel time
- **Visual:** Glowing green projectile sprite
- **Audio:** Whoosh sound on fire, impact sound on hit

**Kiting Behavior Details:**
- **Back-pedal Movement:** Forest Sprite moves away from player while maintaining facing
  - Uses inverse pathfinding (move away from player, not toward)
  - Updates heading to always face player (harder to flank than melee enemies)
  - Maintains LoS (doesn't pathfind through obstacles while kiting)
- **Opportunistic Attacks:** Continues firing projectiles while repositioning
  - Attack timer independent of movement
  - Creates pressure even during retreat

**Component Structure:**
```rust
pub struct Projectile {
    pub source: Entity,          // Who fired it
    pub damage: u32,             // Base damage
    pub target_pos: Vec3,        // Snapshot of target location
    pub speed: f32,              // Hexes per second
    pub damage_type: DamageType, // Physical/Magic
}

pub enum EnemyBehavior {
    // Existing
    MeleeAggressor { /* Wild Dog */ },

    // New
    RangedKiter {
        optimal_distance: RangeInclusive<u8>, // 5-8 hexes
        disengage_distance: u8,               // 3 hexes
        attack_interval: f32,                 // 3.0 seconds
        projectile_speed: f32,                // 4.0 hexes/sec
    },
}
```

**Integration Points:**
- **AI System:** Add `RangedKiter` behavior variant, implement kiting logic
- **Projectile System:** New system to update projectile positions, detect collisions
- **Spawn System:** Add Forest Sprite to spawn tables (mixed with Wild Dogs)
- **Visual System:** Render projectile sprites, kiting animations

### Feature 3: Movement Speed (Grace Formula)

Implement Grace attribute scaling for movement speed.

**Formula (from attribute-system.md Lines 338-347):**
```rust
let movement_speed = max(75, 100 + (grace / 2));
```

**Scaling Examples:**
- Grace = -100 (Might specialist): speed = 75 (clamped at -25% penalty)
- Grace = 0 (parity): speed = 100 (baseline)
- Grace = 50: speed = 125 (+25% bonus)
- Grace = 100 (Grace specialist): speed = 150 (+50% bonus)

**Implementation Approach:**
- **Movement Timing:** Scale the fixed update movement step
  - Current: All entities move at 125ms per hex
  - New: Movement duration = `125ms / (movement_speed / 100.0)`
  - Grace 100 entity: `125ms / 1.5 = 83ms` per hex (50% faster)
  - Grace -100 entity: `125ms / 0.75 = 167ms` per hex (25% slower)

**Design Decision: Why Penalty Cap at -25%?**
- Prevents extreme immobility (Might 100, Grace -100 should still be playable)
- Ensures melee specialists aren't completely kited to death
- Balances trade-off (Might specialists get damage + stamina, but sacrifice speed)

**Component Structure:**
```rust
pub struct MovementSpeed {
    pub speed_multiplier: f32, // Derived from Grace: max(0.75, 1.0 + grace/200.0)
}
```

**Integration Points:**
- **ActorAttributes:** Add `movement_speed()` method that calculates from Grace
- **Movement System:** Query `ActorAttributes`, apply speed multiplier to movement timing
- **AI System:** Ranged kiters can outrun Grace -100 players, creating tactical scenarios
- **Combat Balance:** Grace now provides tangible benefit (mobility + dodge recovery)

## Architectural Decisions

### Decision 1: Projectile System Architecture

**Question:** Should projectiles be entities or events?

**Chosen: Entities**

**Rationale:**
- Projectiles have position, velocity, duration (entity properties)
- Need to be rendered (visual representation)
- Need collision detection (spatial queries via NNTree)
- Multiple projectiles can exist simultaneously
- Server authority on projectile position (prevent client manipulation)

**Alternative Considered: Events**
- Simpler implementation (no entity lifecycle management)
- No visual representation (projectiles "teleport" on hit)
- Harder to dodge (no travel time feedback)
- **Rejected:** Violates "conscious but decisive" design pillar (no time to react)

**Implementation:**
```rust
// Server spawns projectile entity
let projectile_entity = commands.spawn((
    Projectile {
        source: caster_entity,
        damage: 20,
        target_pos: target_snapshot,
        speed: 4.0,
    },
    Loc::from_qrz(caster_loc.qrz()),
    Offset::default(),
    Heading::from_direction(direction),
)).id();

// Projectile update system (FixedUpdate)
fn update_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    mut projectiles: Query<(Entity, &mut Offset, &Projectile, &Loc)>,
    potential_targets: Query<(Entity, &Loc, &Health)>,
) {
    for (proj_entity, mut offset, projectile, loc) in projectiles.iter_mut() {
        // Move projectile toward target_pos
        let direction = (projectile.target_pos - offset.state).normalize();
        let move_distance = projectile.speed * time.delta_seconds();
        offset.state += direction * move_distance;

        // Check if reached target position or hit entity
        if offset.state.distance(projectile.target_pos) < 0.5 {
            // Apply damage to entities at target hex
            apply_projectile_damage(loc, projectile, &potential_targets);
            commands.entity(proj_entity).despawn();
        }
    }
}
```

### Decision 2: Tier Lock State vs. Stateless

**Question:** Should tier lock be entity component state or computed per-frame?

**Chosen: Component State (`TargetingState`)**

**Rationale:**
- Tier lock persists across frames (not frame-local)
- Needs to reset on ability use (state transition)
- Visual feedback depends on current lock state
- Input handling needs to query "am I locked?" (state-dependent behavior)

**Alternative Considered: Stateless (compute on demand)**
- No state management complexity
- Must pass "last pressed key" through input system
- Harder to implement "drop lock after ability" (no state to clear)
- **Rejected:** State transitions are core to tier lock UX

### Decision 3: Movement Speed Scaling - Fixed Update vs. Delta Time

**Question:** How should movement speed affect movement timing?

**Chosen: Scale movement delta in Fixed Update**

**Rationale:**
- Movement occurs in FixedUpdate (125ms ticks) for physics determinism
- Grace scaling should affect how much distance is covered per tick
- Formula: `distance_per_tick = base_distance * movement_speed_multiplier`
- Maintains fixed update timing (consistent physics simulation)
- Simple implementation (multiply existing movement vector)

**Alternative Considered: Variable Fixed Update Rate**
- Faster entities run FixedUpdate more frequently
- Requires per-entity schedules (massive complexity)
- Breaks physics determinism (different tick rates = different collisions)
- **Rejected:** Over-engineered, breaks core physics assumptions

**Implementation:**
```rust
// In movement system (FixedUpdate)
fn apply_movement(
    mut actors: Query<(&ActorAttributes, &KeyBits, &mut Offset, &mut Heading)>,
) {
    for (attrs, keys, mut offset, mut heading) in actors.iter_mut() {
        let movement_speed = attrs.movement_speed(); // Derived from Grace
        let base_velocity = calculate_velocity_from_keybits(keys);
        let scaled_velocity = base_velocity * movement_speed;

        offset.step += scaled_velocity * FIXED_DELTA_TIME;
    }
}
```

### Decision 4: Forest Sprite Spawn Distribution

**Question:** Where/how should Forest Sprites spawn?

**Chosen: Mixed spawns with Wild Dogs (40% Sprites, 60% Dogs)**

**Rationale:**
- Creates varied encounters (sometimes melee, sometimes ranged, sometimes both)
- Tests targeting system (need tier lock when both types present)
- Forces tactical adaptation (can't use same strategy every fight)
- 40% ensures Sprites are common enough to encounter regularly

**Alternative Considered: Separate biomes**
- Forest Sprites only in forest chunks, Wild Dogs in plains
- More thematically coherent
- **Rejected:** Delays testing tier lock (players might avoid forest areas)

**Spawn Table:**
```rust
pub fn spawn_enemy(chunk: ChunkCoord, rng: &mut impl Rng) -> EntityType {
    let roll = rng.gen_range(0..100);
    match roll {
        0..40 => EntityType::ForestSprite,  // 40% ranged
        40..100 => EntityType::WildDog,     // 60% melee
    }
}
```

## Consequences

### Positive Consequences

**1. Tactical Depth Increases Significantly**

*Tier Lock:*
- Players can prioritize targets (kill ranged enemy first, ignore melee)
- Skill expression: "Do I tier lock the Sprite (dangerous, distant) or auto-target the Dog (immediate threat)?"
- Validates targeting system before adding more complex scenarios

*Ranged Enemies:*
- Forces movement and positioning (can't face-tank ranged damage)
- Dodging projectiles creates skill moments (move during travel time)
- Mixed encounters require adaptive tactics (can't use same rotation every fight)

*Movement Speed:*
- Grace specialists can kite effectively (outrun melee enemies)
- Might specialists are slower but tankier (trade mobility for survivability)
- Creates attribute value (Grace is now useful, not just a dump stat)

**Result:** Combat feels varied, tactical, and skill-based.

**2. Attributes Become Tangible**

Before: "Grace does... dodge recovery? What's that?"
After: "Grace makes me move faster. I feel the difference."

- Players immediately perceive Grace benefit (mobility is obvious)
- Might vs. Grace trade-off is concrete (damage + stamina vs. speed + evasion)
- Creates early build identity (fast vs. slow playstyles)

**Result:** Attribute system feels impactful even without progression.

**3. Validates Core Systems Before Expansion**

- **Tier Lock:** Tests targeting framework before adding TAB cycling
- **Projectiles:** Tests travel time, dodging, LoS before adding player ranged abilities
- **Movement Speed:** Tests Grace scaling before adding dodge recovery, hit chance, etc.
- **Enemy Variety:** Tests AI framework before adding bosses, elite variants, etc.

**Result:** Confidence in architecture before adding complexity.

**4. Emergent Tactical Scenarios**

*Example Encounter: 1 Forest Sprite + 2 Wild Dogs*
```
Player (Grace 0, speed 100):
- Dogs chase at speed 100 (same speed)
- Sprite kites at speed 110 (faster, stays 7 hexes away)
- Decision: "Lunge to Sprite (tier lock 3) to close gap? Or fight Dogs first?"
```

*Example Encounter: Grace 100 Player vs. Forest Sprite*
```
Player speed 150, Sprite speed 110:
- Player can close distance faster than Sprite kites
- Sprite's optimal zone (5-8 hexes) becomes unreachable
- Tactical: Grace specialists counter ranged enemies
```

*Example Encounter: Grace -100 Player vs. Forest Sprite*
```
Player speed 75, Sprite speed 110:
- Player cannot catch Sprite (speed disadvantage)
- Sprite can kite indefinitely
- Tactical: Might specialists need positioning (use terrain to cut off Sprite)
```

**Result:** Different attribute distributions create distinct combat experiences.

### Negative Consequences

**1. Projectile System Complexity**

*Implementation Challenges:*
- Projectile entity lifecycle (spawn, update, despawn)
- Collision detection (hex-based or sub-hex precision?)
- Network synchronization (projectile position must be authoritative)
- Visual rendering (sprite rotation, trail effects)

*Mitigation:*
- Start with simple hex-based collision (projectile at same hex = hit)
- Use existing entity spawning patterns (projectiles are just entities)
- Defer visual polish (basic sprite + line renderer for MVP)

**2. Kiting AI Complexity**

*Implementation Challenges:*
- Inverse pathfinding (move away from player, not toward)
- Maintaining LoS while kiting (don't path through walls)
- Opportunistic attacks during movement (attack timer independent of pathing)
- Edge cases: Backed into corner, multiple players, terrain obstacles

*Mitigation:*
- Use simple distance threshold (if too close, move 1-2 hexes away)
- Reuse existing pathfinding (reverse direction vector)
- Leash early (30 hex distance) to prevent infinite kiting

**3. Movement Speed Balancing**

*Potential Issues:*
- Grace 100 players might be "unkitable" (too fast for ranged enemies)
- Grace -100 players might be "uncatchable by melee" if they Lunge away
- Speed differences might feel too subtle (players don't notice 25% difference)
- Speed differences might feel too extreme (Grace -100 feels unplayable)

*Mitigation:*
- Playtest extensively with different Grace values
- Adjust formula if needed (current: `max(75, 100 + grace/2)`)
- Consider capping Grace at ±50 for MVP (prevents extremes)
- Add visual feedback (speed lines, dust particles) to make speed obvious

**4. Tier Lock Learning Curve**

*UX Challenges:*
- New players might not understand tier lock (no tutorial)
- Number keys might conflict with other keybindings
- "Lock drops after ability" might feel arbitrary (why not persistent?)
- Empty tier visualization might clutter screen

*Mitigation:*
- Add tutorial pop-up on first enemy encounter ("Press 2 to target distant enemies")
- Make tier lock keybindings configurable
- Document "drop after ability" clearly (prevents "stuck lock" confusion)
- Make empty tier visualization subtle (outline, not solid fill)

**5. Forest Sprite Might Be Too Punishing**

*Risk Scenarios:*
- Grace -100 player cannot catch Forest Sprite (frustrating)
- Forest Sprite + Wild Dogs = overwhelming pressure (too much threat variety)
- Projectiles deal 20 damage (same as auto-attack) but at range (feels unfair)

*Mitigation:*
- Reduce Forest Sprite HP to 80 (glass cannon - burst it down fast)
- Increase projectile cooldown to 3 seconds (less pressure)
- Ensure Lunge can close distance (4 hex range, Sprite at 5-8 hexes)
- Playtest with different spawn ratios (maybe 30% Sprites, 70% Dogs if too hard)

### Neutral Impacts

**1. Projectile Dodging Requires Client-Side Prediction**

Projectiles are authoritative server-side, but clients predict positions for smooth rendering. Similar to existing player movement prediction (ADR-002), this is additional complexity but necessary for responsiveness.

**2. Movement Speed Affects All Systems**

NPCs, players, and future mounts all use movement speed. This is a foundational change that ripples through codebase, but it's isolated to movement system (not invasive).

**3. Tier Lock Is MVP Feature, TAB Cycling Deferred**

Tier lock (number keys) solves 80% of targeting issues. TAB cycling (manual target selection) is still needed for equidistant targets but deferred to Phase 2.

## Implementation Phases

### Phase 1: Tier Lock Targeting (Estimated: 1-2 days)

**Tasks:**
1. Add `TargetingState` component to player entities
2. Add input bindings for 1/2/3 keys → `Event::TrySetTierLock`
3. Update targeting system to filter by tier when locked
4. Add ability execution hook to reset tier lock
5. Implement tier badge UI rendering
6. Implement empty tier range visualization
7. Write unit tests for tier lock state transitions

**Validation:**
- Press 1 → indicator shows "Close" badge, targets only 1-2 hex enemies
- Press 3 → indicator shows "Far" badge, targets only 7+ hex enemies
- Use ability → tier lock drops, returns to auto-target
- Empty tier → range cone highlights, switches to target when enemy enters

### Phase 2: Movement Speed (Estimated: 1 day)

**Tasks:**
1. Add `movement_speed()` method to `ActorAttributes` (derives from Grace)
2. Update movement system to apply speed multiplier
3. Update NPC spawning to set Grace values (vary speeds)
4. Add movement speed to character panel UI
5. Write unit tests for speed formula

**Validation:**
- Grace 0 player: moves at 100% speed (baseline)
- Grace 100 player: moves at 150% speed (noticeably faster)
- Grace -100 player: moves at 75% speed (clamped, slower but playable)
- Speed difference visible in side-by-side test (two players racing)

### Phase 3: Projectile System (Estimated: 2-3 days)

**Tasks:**
1. Create `Projectile` component and entity archetype
2. Implement projectile update system (movement, collision detection)
3. Add projectile spawning to ability system
4. Implement projectile rendering (sprite + rotation)
5. Add network synchronization for projectiles
6. Write unit tests for projectile physics

**Validation:**
- Projectile spawns at caster position, travels toward target
- Projectile hits entities at target hex when it arrives
- Projectile dodgeable (player moves off hex during travel)
- Projectile despawns after hit or timeout

### Phase 4: Forest Sprite AI (Estimated: 2-3 days)

**Tasks:**
1. Create `ForestSprite` entity type with stats
2. Implement `RangedKiter` behavior variant
3. Add kiting logic (inverse pathfinding, optimal distance)
4. Add projectile attack to Forest Sprite ability set
5. Update spawn tables to include Forest Sprites
6. Add Forest Sprite visual assets (sprite, animations)
7. Write unit tests for kiting behavior

**Validation:**
- Forest Sprite spawns in world (mixed with Wild Dogs)
- Sprite aggros player at 15 hexes, kites to 5-8 hex range
- Sprite fires projectile every 3 seconds
- Sprite flees if player closes within 3 hexes
- Sprite leashes at 30 hexes

### Phase 5: Integration and Balance (Estimated: 1-2 days)

**Tasks:**
1. Playtest all features together (tier lock + ranged enemy + movement speed)
2. Balance projectile damage, attack speed, movement speed
3. Adjust spawn ratios (Sprite vs. Dog)
4. Polish visual feedback (tier lock UI, projectile effects, speed indicators)
5. Write integration tests for mixed encounters

**Validation:**
- Mixed encounters feel tactical (must adapt to enemy composition)
- Grace attribute feels valuable (mobility difference is obvious)
- Tier lock feels natural (easy to target distant enemies)
- Combat variety exists (different fights feel different)

**Total Estimated Time:** 7-11 days (1.5-2 weeks)

## Validation Criteria

**Implementation Complete When:**

1. ✅ **Tier Lock Targeting:**
   - 1/2/3 keys lock to Close/Mid/Far tiers
   - Tier lock drops after 1 ability use
   - Empty tier shows range visualization
   - Tier badge appears on target indicator

2. ✅ **Movement Speed:**
   - Grace 0: speed = 100 (baseline)
   - Grace 100: speed = 150 (+50%)
   - Grace -100: speed = 75 (clamped at -25%)
   - Speed difference visible in gameplay

3. ✅ **Forest Sprite:**
   - Spawns in world (40% of enemy spawns)
   - Kites to 5-8 hex range
   - Fires projectile every 3 seconds (20 damage)
   - Projectile travel time ~1-2 seconds (dodgeable)
   - Flees if player closes within 3 hexes

4. ✅ **Projectile System:**
   - Projectiles spawn at caster position
   - Projectiles travel toward target hex
   - Projectiles hit entities at target hex on arrival
   - Projectiles are dodgeable (move off hex during travel)

**System Validation (Integration Tests):**

1. **Tier Lock Workflow:**
   - Spawn 1 Forest Sprite (7 hexes) + 1 Wild Dog (2 hexes)
   - Default targeting → targets Wild Dog (closer)
   - Press 3 → targets Forest Sprite (far tier)
   - Use Lunge → tier lock drops, targets Wild Dog again

2. **Movement Speed Scaling:**
   - Spawn Grace 0 player, Grace 100 player, Grace -100 player
   - Race across 10 hexes
   - Verify arrival times: Grace 100 arrives first, Grace -100 arrives last
   - Measure: Grace 100 should be ~2x faster than Grace -100 (150% vs 75%)

3. **Projectile Dodging:**
   - Forest Sprite fires projectile at player
   - Player sees projectile traveling (visual feedback)
   - Player moves to adjacent hex during travel
   - Projectile hits original hex (player takes no damage)

4. **Kiting Behavior:**
   - Player approaches Forest Sprite from 10 hexes
   - Sprite remains stationary (optimal distance maintained)
   - Player closes to 6 hexes
   - Sprite attacks (projectile)
   - Player closes to 3 hexes
   - Sprite flees (moves away while firing)

5. **Grace vs. Ranged Enemy:**
   - Grace -100 player (speed 75) chases Forest Sprite (speed 110)
   - Sprite can kite indefinitely (speed advantage)
   - Player must use Lunge to close distance (tier lock 3, press Q)

**Player Experience Validation (Playtest):**

1. "Does tier lock feel natural to use?" (UX)
2. "Does Grace attribute feel valuable?" (mobility difference obvious)
3. "Are Forest Sprites fair but challenging?" (difficulty balance)
4. "Can you dodge projectiles consistently?" (skill expression)
5. "Do mixed encounters feel tactical?" (variety creates decisions)

## Related ADRs

- **ADR-002: Combat Foundation** - Movement speed affects stamina usage (future)
- **ADR-003: Reaction Queue System** - Projectile damage enters queue (standard flow)
- **ADR-004: Ability System and Targeting** - Tier lock completes targeting framework
- **ADR-005: Damage Pipeline** - Projectile damage uses existing pipeline
- **ADR-006: AI Behavior** - Forest Sprite adds second behavior variant
- **ADR-009: MVP Ability Set** - Lunge benefits from tier lock (mid-tier targeting)

## References

- `docs/spec/combat-system.md` - Tier lock (Lines 85-106), Forest Sprite (Lines 482-490), Projectiles (Lines 151-161)
- `docs/spec/attribute-system.md` - Movement speed formula (Lines 338-347)
- `docs/spec/combat-system-feature-matrix.md` - Implementation tracking
- `docs/spec/attribute-system-feature-matrix.md` - Grace attribute tracking
