# ADR-016: Projectile System Architecture

## Status

**Accepted** - 2025-11-03

## Context

**Related RFC:** [RFC-010: Combat Variety Phase 1](../01-rfc/010-combat-variety-phase-1.md)

Forest Sprite enemy needs ranged projectile attacks. Combat system requires dodgeable, visible projectiles with travel time to maintain "conscious but decisive" design pillar. Must decide: implement projectiles as entities or events?

### Requirements

- Projectiles must be dodgeable (player can move off hex during travel)
- Travel time visible (player sees projectile approaching)
- Multiple projectiles can exist simultaneously
- Server-authoritative (prevent client manipulation)
- Integrates with damage pipeline (enters reaction queue)
- Reusable for future projectile abilities

### Options Considered

**Option 1: Projectiles as Entities** ✅ **SELECTED**
- Spawn entity with Projectile component
- Update position in FixedUpdate
- Collision detection via hex position
- Despawn on hit or timeout

**Option 2: Projectiles as Events**
- Emit ProjectileFired event
- Instant damage calculation
- No visual representation
- ❌ No travel time (not dodgeable)

**Option 3: Projectiles as Temporary Sprites (No Entity)**
- Client-side visual only
- Server calculates instant hit
- ❌ No dodging (server decides hit before client sees)

## Decision

**Use entity-based projectile system with travel time and server-authoritative collision detection.**

### Core Mechanism

**Projectile Component:**

```rust
pub struct Projectile {
    pub source: Entity,          // Who fired it
    pub damage: u32,             // Base damage
    pub target_pos: Vec3,        // Snapshot of target location
    pub speed: f32,              // Hexes per second
    pub damage_type: DamageType, // Physical/Magic
    pub max_lifetime: f32,       // Timeout (prevents infinite projectiles)
}
```

**Spawning Pattern:**

```rust
// Server spawns projectile entity
pub fn spawn_projectile(
    commands: &mut Commands,
    caster: Entity,
    caster_loc: Loc,
    target_pos: Vec3,
    damage: u32,
) -> Entity {
    commands.spawn((
        Projectile {
            source: caster,
            damage,
            target_pos,
            speed: 4.0,  // 4 hexes/second
            damage_type: DamageType::Physical,
            max_lifetime: 5.0,  // 5 seconds max
        },
        Loc::from_qrz(caster_loc.qrz()),  // Start at caster hex
        Offset::default(),                 // Sub-hex position
        Heading::from_direction(direction),
        // Network sync components...
    )).id()
}
```

**Update System (FixedUpdate):**

```rust
pub fn update_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    mut projectiles: Query<(Entity, &mut Offset, &mut Loc, &Projectile)>,
    potential_targets: Query<(Entity, &Loc, &Health)>,
) {
    for (proj_entity, mut offset, mut loc, projectile) in projectiles.iter_mut() {
        // Move projectile toward target_pos
        let current_pos = loc.to_world_pos() + offset.state;
        let direction = (projectile.target_pos - current_pos).normalize();
        let move_distance = projectile.speed * time.delta_seconds();

        offset.state += direction * move_distance;

        // Update Loc if crossed hex boundary
        update_loc_from_offset(&mut loc, &mut offset);

        // Check if reached target hex
        if current_pos.distance(projectile.target_pos) < 0.5 {
            // Hit entities at target hex
            apply_projectile_damage(loc, projectile, &potential_targets);
            commands.entity(proj_entity).despawn();
        }

        // Timeout check
        if time.elapsed_seconds() - projectile.spawn_time > projectile.max_lifetime {
            commands.entity(proj_entity).despawn();
        }
    }
}
```

**Collision Detection (Hex-Based):**

```rust
fn apply_projectile_damage(
    projectile_loc: &Loc,
    projectile: &Projectile,
    targets: &Query<(Entity, &Loc, &Health)>,
) {
    // Find all entities at projectile's hex
    for (entity, loc, health) in targets.iter() {
        if loc.qrz() == projectile_loc.qrz() {
            // Apply damage via existing pipeline (ADR-005)
            // Damage enters reaction queue (ADR-003)
            queue_damage(entity, projectile.damage, projectile.source);
        }
    }
}
```

**Dodging Mechanic:**

Player at hex A, projectile fired targeting A.
- Travel time: ~1-2 seconds (distance / speed)
- Player sees projectile approaching (visual feedback)
- Player moves to hex B during travel
- Projectile arrives at hex A (original target)
- Hex A is empty → no damage
- Player successfully dodged

---

## Rationale

### 1. Entities Enable Dodging

**Problem:** Events (instant damage) don't allow reaction time.

**Solution:** Entity-based projectiles have travel time.

**Impact:**
- Player sees projectile approaching (visual feedback)
- Player can move off hex during travel (dodge window)
- Creates skill moment ("I dodged that projectile")
- Aligns with "conscious but decisive" design pillar

**Trade-off:** More complex than instant events (entity lifecycle management).

### 2. Server Authority Prevents Cheating

**Problem:** Client-predicted projectiles could be manipulated.

**Solution:** Server spawns, updates, and detects collisions.

**Impact:**
- Server calculates projectile position (authoritative)
- Client predicts position for smooth rendering (visual only)
- Collision detection server-side (prevents fake dodges)
- Matches existing prediction pattern (Offset component from ADR-002)

**Trade-off:** Network overhead (projectile position sync), but reuses existing sync patterns.

### 3. Hex-Based Collision Simplifies Detection

**Problem:** Sub-hex precision collision is complex.

**Solution:** Projectile hits entities at same hex.

**Impact:**
- Simple collision check (loc.qrz() == projectile_loc.qrz())
- Reuses existing hex grid system
- Consistent with hex-based positioning
- Fast (no expensive spatial queries)

**Trade-off:** Less precise than sub-hex (but hex precision is core to game design).

### 4. Reusable for Future Abilities

**Current:** Forest Sprite projectile attack.

**Future:**
- Player abilities: Volley (ranged physical), Fireball (ranged magic)
- Enemy abilities: Archer, Mage variants
- Environmental hazards: Traps, turrets

**Pattern established:**
- Spawn projectile entity
- Set target_pos, damage, speed, damage_type
- System handles movement, collision, despawn
- Consistent behavior across all projectile sources

---

## Consequences

### Positive

**1. Dodging Creates Skill Expression**
- Players see projectiles approaching (visual feedback)
- Can react during travel time (dodge window ~1-2s)
- Skill moment: "I dodged that!" (positive reinforcement)
- Differentiates good players (consistent dodging) from bad (get hit)

**2. Visual Feedback Improves Combat Feel**
- Projectiles rendered as moving sprites
- Travel time visible (approach trajectory)
- Impact visual (hit effect when collision detected)
- Clearer than instant damage (no "invisible hit" confusion)

**3. Reusable Architecture**
- Future projectile abilities use same system
- Consistent behavior (all projectiles dodge-able)
- Consistent implementation (spawn entity, system handles rest)
- Minimal per-ability code (just set parameters)

**4. Integrates Cleanly with Existing Systems**
- Uses damage pipeline (ADR-005)
- Enters reaction queue (ADR-003)
- Uses Loc + Offset (ADR-002)
- Server-authoritative (matches architecture)

**5. Server Authority Prevents Cheating**
- Client can't fake dodges (server calculates collision)
- Client can't manipulate projectile speed
- Client can't force hits (server validates)

### Negative

**1. Entity Lifecycle Complexity**
- Spawning projectiles (commands.spawn)
- Updating positions (FixedUpdate system)
- Despawning on hit/timeout (entity management)
- More complex than instant events

**Mitigation:** Reuse existing entity patterns, isolate in projectile system.

**2. Network Overhead**
- Projectile position synced server → clients
- Multiple projectiles = multiple sync messages
- More network traffic than instant events

**Mitigation:** Projectile count typically low (5-10 active), reuse existing Offset sync.

**3. Hex-Based Collision Less Precise**
- Projectile at hex center might "miss" entity at hex edge
- Less realistic than sub-hex precision
- Edge cases: Entity moves to adjacent hex at exact collision moment

**Mitigation:** Hex precision is core to game design (intentional trade-off), edge cases rare.

**4. Requires Visual Assets**
- Projectile sprites (green orb, arrow, fireball, etc.)
- Trail effects (optional)
- Impact effects (hit/miss)

**Mitigation:** Start with simple sprite, add polish later.

### Neutral

**1. Client Prediction for Smooth Rendering**
- Server authoritative, client predicts for visuals
- Similar to existing Offset prediction (ADR-002)
- Adds complexity but necessary for responsiveness

**2. Timeout Required**
- Prevents infinite projectiles (if target despawns mid-flight)
- Adds max_lifetime parameter (5s default)
- Housekeeping cost (check timeout every frame)

---

## Implementation Notes

**File Structure:**
```
src/server/systems/projectile.rs  - Update system, collision detection
src/common/components/projectile.rs - Projectile component
```

**Integration Points:**
- Spawning: Enemy AI fires projectile (spawn entity)
- Update: FixedUpdate system (movement, collision, despawn)
- Damage: Uses damage pipeline (ADR-005), enters reaction queue (ADR-003)
- Rendering: Client renders sprite + trail based on Offset

**Network:**
- Projectile entity synced (Loc, Offset, Heading)
- Server-authoritative collision detection
- Client predicts position for smooth rendering

---

## Validation Criteria

**Functional:**
- Projectile spawns at caster position
- Projectile travels toward target_pos (4 hexes/second)
- Projectile hits entities at target hex on arrival
- Projectile is dodgeable (player moves off hex during travel)
- Projectile despawns after hit or timeout

**Dodging:**
- Player at hex A, projectile fired at A
- Player moves to B during travel (~1s)
- Projectile hits A (empty), player takes no damage
- Visual: Player sees projectile miss (passed by)

**Performance:**
- 10 active projectiles: < 1ms per FixedUpdate tick
- Network: Projectile sync overhead negligible (< 100 bytes/sec per projectile)

**Reusability:**
- Future abilities spawn projectiles with different parameters
- Same update system, collision detection, rendering
- Consistent behavior across all projectile sources

---

## References

- **RFC-010:** Combat Variety Phase 1
- **Spec:** `docs/00-spec/combat-system.md` (Projectiles Lines 151-161)
- **ADR-002:** Combat Foundation (Offset component, prediction pattern)
- **ADR-003:** Reaction Queue (projectile damage enters queue)
- **ADR-005:** Damage Pipeline (projectile damage calculation)

## Date

2025-11-03
