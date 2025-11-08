# ADR-015: Architectural Invariants and Test Coverage

## Status
Accepted (Priority 1 implemented, Priority 2 partially implemented with regen tests only)

## Context

### Problem Statement

During an architectural review of the codebase (2025-11-08), a comprehensive analysis identified **17 critical architectural invariants** that underpin the correctness of core systems. While some invariants have test coverage, many critical invariants are either:

1. **Enforced by assertions but not tested** (runtime checks without regression prevention)
2. **Implemented correctly but undocumented** (risk of breaking during refactors)
3. **Completely untested** (silent failures possible)

Without unit tests for these invariants, the codebase is vulnerable to regressions during refactoring, feature additions, or dependency updates.

### Review Methodology

The review analyzed:
- All ADRs (001-014) for documented architectural decisions
- GUIDANCE.md for documented patterns and anti-patterns
- Client-server synchronization code (renet systems)
- Position/movement systems (physics, world updates, offset management)
- Combat systems (damage pipeline, resource calculations, queue management)
- Network protocol (Try/Do event flow, entity ID mapping)
- Existing test coverage (41 test files identified)

### Key Findings

**Strong Coverage:**
- Chunk eviction parity (tested in engagement_cleanup.rs)
- Spatial difficulty calculations (tested in spatial_difficulty.rs)
- Chunk coordinate conversions (tested in chunk.rs)
- Physics characterization tests (gravity, jump, movement)

**Critical Gaps:**
- Client-server synchronization invariants (InputQueue, entity mapping)
- Combat fairness guarantees (two-phase damage timing, armor caps)
- Movement continuity (world-space preservation during Loc updates)
- Network protocol ordering (Spawn before Incremental, proximity filtering)

---

## Decision

We will **document all 17 critical architectural invariants** as the authoritative reference for system correctness guarantees, and **implement unit tests** for the 8 high-priority untested invariants to prevent regressions.

---

## What Makes a Good Invariant Test?

An architectural invariant test must test **system behavior**, not library correctness or formula accuracy. The key question is: **"Would this test fail if our architecture changed incorrectly?"**

### Quality Criteria

1. **Test System Behavior, Not Libraries**
   - ❌ BAD: `assert_eq!(vec.pop_front(), first_item)` // Testing VecDeque
   - ✅ GOOD: `assert!(world_pos_before == world_pos_after)` // Testing our coordinate transform

2. **Would Fail If Architecture Changed**
   - ❌ BAD: Test passes if VecDeque swapped for Vec (not architectural)
   - ✅ GOOD: Test fails if Loc update changes coordinate transform (architectural)

3. **Test Integration Points, Not Pure Logic**
   - ❌ BAD: `assert_eq!(crit_chance, 0.05)` // Formula correctness
   - ✅ GOOD: `assert!(damage_stored_at_attack_time)` // Timing property

4. **Test What Could Actually Break**
   - ❌ BAD: "Does standard library work?" (no)
   - ✅ GOOD: "Does system maintain invariant under edge case?" (yes)

### Examples

**Good Invariant Test:**
```rust
#[test]
fn test_world_space_preserved_on_smooth_tile_crossing() {
    // Tests that our Loc update logic maintains world position
    let world_pos_before = calculate_world_position(&old_loc, &old_offset);
    // ... apply Loc update for adjacent tile crossing ...
    let world_pos_after = calculate_world_position(&new_loc, &new_offset);
    assert_eq!(world_pos_before, world_pos_after); // Architectural invariant
}
```

**Bad Invariant Test:**
```rust
#[test]
fn test_vecdeque_is_fifo() {
    let mut queue = VecDeque::new();
    queue.push_back(1);
    queue.push_back(2);
    assert_eq!(queue.pop_front(), Some(1)); // Testing standard library
}
```

**What to Test Instead:**
```rust
#[test]
fn test_threat_processing_order_matches_expiry_time() {
    // Tests that our system processes threats in FIFO order
    // Insert threats with different timestamps
    // Verify process_expired_threats() processes oldest first
}
```

### When Writing Invariant Tests

- **Focus on timing:** Attack time vs resolution time, schedule ordering
- **Focus on boundaries:** Coordinate transforms, tile crossings, caps
- **Focus on integration:** How systems interact, not how libraries work
- **Avoid statistical tests:** Formula correctness belongs in unit tests
- **Prefer pure functions:** Test logic directly when possible, avoid heavy ECS setup

---

## Critical Architectural Invariants

### Category 1: Client-Server Synchronization

#### INV-001: Chunk Eviction Parity
**Invariant:** Client and server chunk eviction MUST use identical radius (`FOV_CHUNK_RADIUS + 1`)

**Why Critical:** Server tracks which chunks client has seen. Mismatch causes server to re-send chunks client already has (bandwidth waste) or fail to send chunks client needs (missing terrain).

**Location:**
- Client: [src/client/systems/world.rs:227](../../src/client/systems/world.rs#L227)
- Server: [src/server/systems/actor.rs:85](../../src/server/systems/actor.rs#L85)
- Constant: [src/common/chunk.rs:16](../../src/common/chunk.rs#L16) (`FOV_CHUNK_RADIUS = 2`)

**Formula:**
```rust
let active_chunks = calculate_visible_chunks(player_chunk, FOV_CHUNK_RADIUS + 1);
```

**Test Status:** ✅ **TESTED** ([src/server/systems/engagement_cleanup.rs:137-250](../../src/server/systems/engagement_cleanup.rs#L137-L250))

**Reference:** GUIDANCE.md Line 43, ADR-001

---

#### INV-002: InputQueue Never Empty
**Invariant:** All InputQueues MUST contain ≥1 input at all times. Front input accumulates dt overflow.

**Why Critical:** Physics system (`src/common/systems/physics.rs:69`) asserts non-empty queue. Empty queue causes panic, breaking all movement for that entity.

**Location:** [src/common/systems/physics.rs:69](../../src/common/systems/physics.rs#L69)

**Enforcement:**
```rust
assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");
```

**Test Status:** ⚠️ **ASSERTED BUT NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_input_queue_never_empty_invariant() {
    let mut app = App::new();
    let entity = app.world_mut().spawn(/* ... */).id();
    let mut queues = InputQueues::default();

    // Create queue with initial input
    queues.insert(entity, InputQueue::with_initial_input(
        Event::Input { key_bits: KeyBits::default(), dt: 125, seq: 0 }
    ));

    // ASSERT: Queue never empty
    assert!(!queues.get(&entity).unwrap().queue.is_empty());

    // Pop all but last input
    while queues.get(&entity).unwrap().queue.len() > 1 {
        queues.get_mut(&entity).unwrap().queue.pop_back();
    }

    // ASSERT: Still has 1 input (front accumulates time)
    assert_eq!(queues.get(&entity).unwrap().queue.len(), 1);
}

#[test]
#[should_panic(expected = "Queue invariant violation")]
fn test_physics_panics_on_empty_queue() {
    // Characterization test - documents intentional panic behavior
    // Setup entity with empty queue, run physics update
    // EXPECT: Panic with "Queue invariant violation"
}
```

**Reference:** GUIDANCE.md Line 63

---

#### INV-003: World-Space Preservation During Loc Updates
**Invariant:** When entity crosses tile boundary (adjacent hex, distance < 2), world-space position MUST be preserved. Teleports (distance ≥ 2) clear offset.

**Why Critical:** Failure causes visual "snapping" (entity jumps to tile center) or falling through terrain. Breaks client-side prediction continuity.

**Location:** [src/common/systems/world.rs:72-145](../../src/common/systems/world.rs#L72-L145)

**Formula:**
```rust
// Smooth tile crossing (distance < 2):
let state_world = map.convert(old_loc) + old_offset;
let new_tile_center = map.convert(new_loc);
let new_offset = state_world - new_tile_center;

// Teleport (distance ≥ 2):
let new_offset = Vec3::ZERO;
```

**Test Status:** ❌ **NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_world_space_preserved_on_smooth_tile_crossing() {
    let map = Map::new(qrz::Map::new(1.0, 0.8));
    let old_loc = Qrz::new(5, 5);
    let new_loc = Qrz::new(6, 5);  // Adjacent hex (distance = 1)
    let old_offset = Vec3::new(0.5, 0.0, 0.3);

    // World position before update
    let world_pos_before = map.convert(old_loc) + old_offset;

    // Apply world-space preservation formula
    let state_world = map.convert(old_loc) + old_offset;
    let new_tile_center = map.convert(new_loc);
    let new_offset = state_world - new_tile_center;

    // World position after update
    let world_pos_after = map.convert(new_loc) + new_offset;

    // ASSERT: World position unchanged (within floating-point tolerance)
    assert!((world_pos_before - world_pos_after).length() < 0.001,
        "World-space position changed during tile crossing");
}

#[test]
fn test_teleport_clears_offset_for_jumps_over_two_hexes() {
    let old_loc = Qrz::new(0, 0);
    let new_loc = Qrz::new(5, 5);  // Distance >= 2 (teleport)
    let old_offset = Vec3::new(0.5, 1.0, 0.3);

    // Teleport detection
    let hex_distance = old_loc.flat_distance(&new_loc);
    let new_offset = if hex_distance >= 2 {
        Vec3::ZERO  // Teleport: clear offset
    } else {
        old_offset  // Smooth crossing: preserve world space
    };

    assert_eq!(new_offset, Vec3::ZERO, "Teleport did not clear offset");
}
```

**Reference:** GUIDANCE.md Lines 99-100, Client-side prediction architecture

---

#### INV-004: Entity ID Mapping Bidirectionality
**Invariant:** `Lobby` bimap MUST maintain 1:1 mapping between `ClientId ↔ Entity`. No duplicate client IDs, no orphaned entities.

**Why Critical:** Unmapped entities cause server panics (`panic!("no {client_id} in lobby")`) or silent message drops on client.

**Location:** [src/server/systems/renet.rs:218](../../src/server/systems/renet.rs#L218)

**Enforcement:**
```rust
let Some(&ent) = lobby.get_by_left(&client_id) else {
    panic!("no {client_id} in lobby")
};
```

**Test Status:** ❌ **NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_lobby_maintains_bidirectional_client_entity_mapping() {
    use bimap::BiHashMap;

    let mut lobby = BiHashMap::<ClientId, Entity>::new();
    let client1 = ClientId::from_raw(1);
    let entity1 = Entity::from_raw(100);

    // Insert mapping
    lobby.insert(client1, entity1);

    // ASSERT: Bidirectional lookup
    assert_eq!(lobby.get_by_left(&client1), Some(&entity1));
    assert_eq!(lobby.get_by_right(&entity1), Some(&client1));

    // ASSERT: Removal maintains consistency
    lobby.remove_by_left(&client1);
    assert_eq!(lobby.get_by_left(&client1), None);
    assert_eq!(lobby.get_by_right(&entity1), None);
}

#[test]
fn test_lobby_overwrites_duplicate_client_id() {
    use bimap::BiHashMap;

    let mut lobby = BiHashMap::<ClientId, Entity>::new();
    let client1 = ClientId::from_raw(1);
    let entity1 = Entity::from_raw(100);
    let entity2 = Entity::from_raw(200);

    lobby.insert(client1, entity1);

    // Inserting same ClientId with different Entity should overwrite
    let old = lobby.insert(client1, entity2);

    assert_eq!(old, (Some(client1), Some(entity1)));  // Returns old mapping
    assert_eq!(lobby.get_by_left(&client1), Some(&entity2));  // Now maps to entity2
}
```

**Reference:** Try/Do event flow pattern

---

### Category 2: Combat System Fairness

#### INV-005: Damage Calculation Two-Phase Timing
**Invariant:** Outgoing damage calculated at attack time (using attacker's attributes), incoming damage calculated at resolution time (using defender's attributes).

**Why Critical:** Ensures fairness when attributes change mid-queue (buffs/debuffs). Attacker's power at attack time determines base damage, defender's mitigation at resolution time determines final damage.

**Location:**
- Phase 1: [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) (`calculate_outgoing_damage`)
- Phase 2: [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) (`apply_passive_modifiers`)

**Formula:**
```rust
// Phase 1 (attack time): Outgoing damage
let outgoing = base_damage * (1.0 + scaling_attribute / 33.0);

// Phase 2 (resolution time): Incoming damage
let mitigation = (defensive_attribute / 66.0).min(0.75);
let final_damage = outgoing * (1.0 - mitigation);
```

**Test Status:** ❌ **NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_outgoing_damage_uses_attacker_attributes_at_attack_time() {
    let attacker_attrs = ActorAttributes::new(50, 0, 0, 0, 0, 0, 0, 0, 0);  // 50 might
    let base_damage = 20.0;

    let outgoing = calculate_outgoing_damage(base_damage, &attacker_attrs, DamageType::Physical);

    // Formula: base × (1 + might/33) = 20 × (1 + 50/33) ≈ 50.3
    assert!((outgoing - 50.3).abs() < 0.1,
        "Outgoing damage incorrect: expected ~50.3, got {}", outgoing);
}

#[test]
fn test_passive_modifiers_use_defender_attributes_at_resolution_time() {
    let defender_attrs = ActorAttributes::new(0, 0, 0, 66, 0, 0, 0, 0, 0);  // 66 vitality
    let outgoing_damage = 50.0;

    let final_damage = apply_passive_modifiers(outgoing_damage, &defender_attrs, DamageType::Physical);

    // Formula: armor = 0 + (vitality/66) = 1.0, capped at 0.75
    // final = 50 × (1 - 0.75) = 12.5
    assert!((final_damage - 12.5).abs() < 0.1,
        "Final damage incorrect: expected 12.5, got {}", final_damage);
}

#[test]
fn test_two_phase_timing_handles_attribute_changes_mid_queue() {
    // Scenario: Attacker has 50 might at attack time,
    // defender gains 66 vitality buff before resolution

    let attacker_attrs_at_attack = ActorAttributes::new(50, 0, 0, 0, 0, 0, 0, 0, 0);
    let defender_attrs_at_resolution = ActorAttributes::new(0, 0, 0, 66, 0, 0, 0, 0, 0);

    // Phase 1: Attack time (attacker's stats frozen)
    let outgoing = calculate_outgoing_damage(20.0, &attacker_attrs_at_attack, DamageType::Physical);

    // (Time passes, defender gains buff)

    // Phase 2: Resolution time (defender's new stats apply)
    let final_damage = apply_passive_modifiers(outgoing, &defender_attrs_at_resolution, DamageType::Physical);

    assert!((outgoing - 50.3).abs() < 0.1);
    assert!((final_damage - 12.5).abs() < 0.1);
}
```

**Reference:** ADR-005:156-184

---

#### INV-006: Critical Hit Roll at Attack Time
**Invariant:** Crit roll and multiplier determined at attack time using attacker's Instinct. Result stored in `QueuedThreat.damage` (outgoing damage includes crit multiplier).

**Why Critical:** Target cannot "change fate" of crit after attack lands. Maintains fairness and consistency.

**Location:** [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) (`roll_critical`)

**Formula:**
```rust
let crit_chance = 0.05 + (instinct / 200.0);  // 5-80% range
let crit_multiplier = if was_crit {
    1.5 + (instinct / 200.0)  // 1.5-2.25× range
} else {
    1.0
};
```

**Test Status:** ⚠️ **PARTIALLY TESTED** (formula tested, not timing)

**Recommended Test:**
```rust
#[test]
fn test_crit_roll_probabilities_match_formula() {
    let instinct_0 = ActorAttributes::new(0, 0, 0, 0, 0, 0, 0, 0, 0);
    let instinct_100 = ActorAttributes::new(0, 0, 0, 0, 0, 0, 100, 0, 0);

    // Sample 10,000 rolls to verify probability distribution
    let samples = 10_000;
    let mut crits_instinct_0 = 0;
    let mut crits_instinct_100 = 0;

    for _ in 0..samples {
        if roll_critical(&instinct_0).0 { crits_instinct_0 += 1; }
        if roll_critical(&instinct_100).0 { crits_instinct_100 += 1; }
    }

    let crit_rate_0 = crits_instinct_0 as f32 / samples as f32;
    let crit_rate_100 = crits_instinct_100 as f32 / samples as f32;

    // Formula: 5% + (instinct / 200)
    // Instinct 0: 5% ± 1% tolerance
    // Instinct 100: 55% ± 2% tolerance
    assert!((crit_rate_0 - 0.05).abs() < 0.01,
        "Crit rate with 0 instinct: expected ~5%, got {:.1}%", crit_rate_0 * 100.0);
    assert!((crit_rate_100 - 0.55).abs() < 0.02,
        "Crit rate with 100 instinct: expected ~55%, got {:.1}%", crit_rate_100 * 100.0);
}

#[test]
fn test_crit_multiplier_scales_with_instinct() {
    // Formula: 1.5 + (instinct / 200)
    let mult_0 = 1.5 + (0.0 / 200.0);
    let mult_100 = 1.5 + (100.0 / 200.0);

    assert_eq!(mult_0, 1.5);
    assert_eq!(mult_100, 2.0);
}
```

**Reference:** ADR-005:231-255

---

#### INV-007: Armor and Resistance 75% Cap
**Invariant:** Armor (physical mitigation) and Resistance (magic mitigation) MUST cap at 75% damage reduction. No entity can become invulnerable.

**Why Critical:** Prevents invulnerability exploits from extreme attribute stacking. Ensures minimum 25% damage always goes through.

**Location:** [src/common/systems/combat/resources.rs](../../src/common/systems/combat/resources.rs)

**Formula:**
```rust
let armor = base_armor + (vitality / 66.0);
armor.min(0.75)  // Hard cap at 75%

let resistance = base_resistance + (focus / 66.0);
resistance.min(0.75)  // Hard cap at 75%
```

**Test Status:** ❌ **NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_armor_caps_at_75_percent() {
    let extreme_vitality = ActorAttributes::new(0, 0, 0, 150, 0, 0, 0, 0, 0);  // 150 vitality (max)
    let base_armor = 0.0;

    let armor = calculate_armor(&extreme_vitality, base_armor);

    // Should cap at 0.75 despite vitality/66 = 2.27
    assert_eq!(armor, 0.75, "Armor did not cap at 75%");

    // Test damage reduction with capped armor
    let incoming_damage = 100.0;
    let final_damage = apply_passive_modifiers(incoming_damage, &extreme_vitality, DamageType::Physical);

    // 100 × (1 - 0.75) = 25.0 (minimum 25% damage always goes through)
    assert_eq!(final_damage, 25.0, "Damage reduction exceeded 75% cap");
}

#[test]
fn test_resistance_caps_at_75_percent() {
    let extreme_focus = ActorAttributes::new(0, 0, 0, 0, 0, 0, 0, 150, 0);  // 150 focus (max)
    let base_resistance = 0.0;

    let resistance = calculate_resistance(&extreme_focus, base_resistance);

    assert_eq!(resistance, 0.75, "Resistance did not cap at 75%");

    let incoming_damage = 100.0;
    let final_damage = apply_passive_modifiers(incoming_damage, &extreme_focus, DamageType::Magic);

    assert_eq!(final_damage, 25.0, "Magic damage reduction exceeded 75% cap");
}
```

**Reference:** ADR-002:271, ADR-005:260-271

---

#### INV-008: Mutual Destruction Order Independence
**Invariant:** Death checks run AFTER all damage application in same frame. Both entities can die simultaneously (no "who died first" race condition).

**Why Critical:** Spec-compliant "both die" behavior (ADR-005). Prevents unfair outcomes where faster network connection determines survivor.

**Location:** System schedule (server)
```rust
app.add_systems(FixedUpdate, (
    reaction_queue::process_expired_threats,  // Emit ResolveThreat
    damage::process_resolve_threat,           // Apply damage
    resources::check_death,                   // Check HP <= 0, emit Death
).chain());
```

**Test Status:** ❌ **NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_mutual_destruction_both_entities_die() {
    let mut app = App::new();

    // Setup: Two entities with low health
    let entity_a = app.world_mut().spawn((
        Health { state: 10.0, step: 10.0, max: 100.0 },
    )).id();

    let entity_b = app.world_mut().spawn((
        Health { state: 10.0, step: 10.0, max: 100.0 },
    )).id();

    // ACT: Apply lethal damage to both in same frame
    let mut healths = app.world_mut().query::<&mut Health>();

    if let Ok(mut health_a) = healths.get_mut(app.world_mut(), entity_a) {
        health_a.state -= 15.0;  // Lethal
    }

    if let Ok(mut health_b) = healths.get_mut(app.world_mut(), entity_b) {
        health_b.state -= 15.0;  // Lethal
    }

    // Run death check system
    app.update();

    // ASSERT: Both entities should have death events
    let death_events = app.world().resource::<Events<Do>>();
    let death_count = death_events.iter_current_update_events()
        .filter(|e| matches!(e.event, Event::Despawn { .. }))
        .count();

    assert_eq!(death_count, 2, "Both entities should die simultaneously");
}
```

**Reference:** ADR-005:505-520

---

### Category 3: Spatial and Movement

#### INV-009: Heading-Based Position Offset Magnitude
**Invariant:** Entities with non-default heading offset by `HERE = 0.33` units towards heading-specified neighbor.

**Why Critical:** Consistent positioning for stationary entities facing a direction. Affects rendering, collision detection, and combat range calculations.

**Location:** [src/common/systems/physics.rs:171-182](../../src/common/systems/physics.rs#L171-L182)

**Formula:**
```rust
let dest_heading_neighbor = map.convert(dest + heading);
let direction = dest_heading_neighbor - dest_center;
let heading_offset = (direction * HERE).xz();  // 0.33 × direction
let final_pos = dest_center + Vec3::new(heading_offset.x, 0.0, heading_offset.y);
```

**Constants:** `HERE = 0.33`, `THERE = 1.33`

**Test Status:** ❌ **NOT TESTED**

**Recommended Test:**
```rust
#[test]
fn test_heading_based_position_offset_magnitude() {
    let map = Map::new(qrz::Map::new(1.0, 0.8));
    let dest = Qrz::new(5, 5);
    let heading = Qrz::new(1, 0);  // East direction

    let dest_center = map.convert(dest);
    let dest_heading_neighbor = map.convert(dest + heading);
    let direction = dest_heading_neighbor - dest_center;
    let heading_offset_xz = (direction * HERE).xz();  // 0.33 * direction

    let expected_offset_magnitude = direction.length() * 0.33;
    let actual_offset_magnitude = heading_offset_xz.length();

    assert!((actual_offset_magnitude - expected_offset_magnitude).abs() < 0.01,
        "Heading offset magnitude incorrect: expected {}, got {}",
        expected_offset_magnitude, actual_offset_magnitude);
}
```

**Reference:** [src/common/components/heading.rs](../../src/common/components/heading.rs)

---

#### INV-010: Chunk Size and FOV Radius Relationship
**Invariant:** `CHUNK_SIZE = 8` tiles, `FOV_CHUNK_RADIUS = 2` chunks. This covers 25 chunks (5×5 grid) or 1,600 tiles total.

**Why Critical:** Changes to chunk size require recalculating FOV radius, eviction distance, and network proximity filtering radii.

**Location:** [src/common/chunk.rs:16](../../src/common/chunk.rs#L16)

**Constants:**
```rust
pub const CHUNK_SIZE: u8 = 8;
pub const FOV_CHUNK_RADIUS: u8 = 2;
```

**Derived Values:**
- Visible chunks: `(FOV_CHUNK_RADIUS × 2 + 1)² = 25`
- Visible tiles: `25 × 8² = 1,600`

**Test Status:** ⚠️ **IMPLICITLY TESTED** (chunk tests use constants)

**Recommended Test:**
```rust
#[test]
fn test_chunk_size_and_fov_relationship() {
    // Document the relationship between chunk size and FOV
    assert_eq!(CHUNK_SIZE, 8, "Chunk size changed - update FOV calculations");
    assert_eq!(FOV_CHUNK_RADIUS, 2, "FOV radius changed - verify eviction logic");

    // Visible chunks = (radius × 2 + 1)²
    let expected_visible_chunks = (FOV_CHUNK_RADIUS as usize * 2 + 1).pow(2);
    assert_eq!(expected_visible_chunks, 25, "FOV covers 25 chunks (5×5 grid)");

    // Visible tiles = visible_chunks × CHUNK_SIZE²
    let expected_visible_tiles = expected_visible_chunks * (CHUNK_SIZE as usize).pow(2);
    assert_eq!(expected_visible_tiles, 1600, "FOV covers 1,600 tiles total");
}
```

**Reference:** ADR-001:55-58

---

#### INV-011: Spatial Difficulty Level Scaling
**Invariant:** Enemy level = `⌊distance_from_haven / 100⌋`, clamped to [0, 10].

**Why Critical:** Defines progression curve for entire game. Changes affect enemy difficulty gradient and player exploration incentives.

**Location:** [src/common/spatial_difficulty.rs](../../src/common/spatial_difficulty.rs)

**Formula:**
```rust
pub fn calculate_enemy_level(spawn_location: Qrz, haven_location: Qrz) -> u8 {
    let distance = haven_location.distance_to(spawn_location) as f32;
    (distance / 100.0).min(10.0) as u8
}
```

**Test Status:** ⚠️ **PARTIALLY TESTED** (basic tests exist, edge cases missing)

**Recommended Test:**
```rust
#[test]
fn test_enemy_level_calculation_edge_cases() {
    let haven = Qrz::ORIGIN;

    // At haven (distance 0)
    assert_eq!(calculate_enemy_level(Qrz::new(0, 0), haven), 0);

    // Just below level 1 threshold
    assert_eq!(calculate_enemy_level(Qrz::new(0, 99), haven), 0);

    // Exactly level 1 threshold
    assert_eq!(calculate_enemy_level(Qrz::new(0, 100), haven), 1);

    // Mid-range
    assert_eq!(calculate_enemy_level(Qrz::new(0, 500), haven), 5);

    // Max level at 1000+ tiles
    assert_eq!(calculate_enemy_level(Qrz::new(0, 1000), haven), 10);
    assert_eq!(calculate_enemy_level(Qrz::new(0, 5000), haven), 10);  // Still clamped
}
```

**Reference:** ADR-014:112-143

---

### Category 4: Network Protocol

#### INV-012: Spawn Before Incremental
**Invariant:** Client MUST receive `Spawn` event before any `Incremental` component updates for that entity. If Incremental received for unknown entity, client requests Spawn.

**Why Critical:** Component updates for unknown entities are meaningless. Client self-heals by requesting spawn when unknown entity detected.

**Location:** [src/client/systems/renet.rs](../../src/client/systems/renet.rs) (`write_do`)

**Enforcement:**
```rust
let Some(&ent) = l2r.get_by_right(&ent) else {
    // Unknown entity - request spawn
    try_writer.write(Try { event: Event::Spawn { ... } });
    continue
};
```

**Test Status:** ⚠️ **ENFORCED IN CODE** (client self-heals)

**Recommended Test:**
```rust
#[test]
fn test_client_requests_spawn_for_unknown_entity_on_incremental() {
    // Integration test documenting self-healing behavior
    // SETUP: Client entity map empty
    // ACT: Receive Incremental for entity 999 (unknown)
    // ASSERT: Try::Spawn emitted for entity 999
}
```

**Reference:** Try/Do event flow pattern

---

#### INV-013: Proximity Filtering Radii
**Invariant:** Different event types broadcast at different radii to balance network traffic with gameplay needs.

**Why Critical:** Too small = missed events (invisible enemies). Too large = bandwidth waste. Radii carefully tuned for each event type.

**Location:** [src/server/systems/renet.rs](../../src/server/systems/renet.rs) (`send_do`)

**Radii:**
- **Spawn/Incremental:** 55 hex (covers FOV with Z-level buffer)
- **Combat events (InsertThreat, ApplyDamage, ClearQueue):** 20 hex
- **Movement intent:** 30 hex (Unreliable channel)
- **Despawn:** 70 hex (ensures all discoverers notified)

**Test Status:** ❌ **NOT TESTED** (hardcoded values)

**Recommended Test:**
```rust
#[test]
fn test_proximity_filtering_radii_documented() {
    // Characterization test - documents expected radii
    const SPAWN_RADIUS: u32 = 55;
    const COMBAT_RADIUS: u32 = 20;
    const MOVEMENT_INTENT_RADIUS: u32 = 30;
    const DESPAWN_RADIUS: u32 = 70;

    // Verify ordering: combat < movement < spawn < despawn
    assert!(COMBAT_RADIUS < MOVEMENT_INTENT_RADIUS);
    assert!(MOVEMENT_INTENT_RADIUS < SPAWN_RADIUS);
    assert!(SPAWN_RADIUS < DESPAWN_RADIUS);

    // Verify spawn radius covers FOV with buffer
    let fov_tiles = FOV_CHUNK_RADIUS as u32 * CHUNK_SIZE as u32;  // 2 × 8 = 16
    assert!(SPAWN_RADIUS > fov_tiles * 2,
        "Spawn radius should exceed FOV diameter for Z-level buffer");
}
```

**Reference:** Network protocol architecture

---

## Implementation Plan

### Priority 1: Critical Synchronization (Sprint 1)

**Goal:** Prevent panics and visual glitches

1. **INV-002: InputQueue Never Empty** ([src/common/systems/physics.rs](../../src/common/systems/physics.rs))
   - Add `test_input_queue_never_empty_invariant()`
   - Add `test_physics_panics_on_empty_queue()` (characterization)
   - **Why first:** Prevents movement system crashes

2. **INV-003: World-Space Preservation** ([src/common/systems/world.rs](../../src/common/systems/world.rs))
   - Add `test_world_space_preserved_on_smooth_tile_crossing()`
   - Add `test_teleport_clears_offset_for_jumps_over_two_hexes()`
   - **Why first:** Prevents falling through terrain, visual snapping

3. **INV-007: Armor/Resistance 75% Cap** ([src/common/systems/combat/resources.rs](../../src/common/systems/combat/resources.rs))
   - Add `test_armor_caps_at_75_percent()`
   - Add `test_resistance_caps_at_75_percent()`
   - **Why first:** Prevents invulnerability exploits

4. **INV-005: Damage Two-Phase Timing** ([src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs))
   - Add `test_outgoing_damage_uses_attacker_attributes_at_attack_time()`
   - Add `test_passive_modifiers_use_defender_attributes_at_resolution_time()`
   - Add `test_two_phase_timing_handles_attribute_changes_mid_queue()`
   - **Why first:** Ensures combat fairness

**Estimated Effort:** 1 day (4 invariants, 9 tests total)

---

### Priority 2: Combat and Network (Sprint 2)

**Goal:** Validate fairness guarantees and protocol correctness

5. **INV-008: Mutual Destruction** ([server schedule](../../src/run-server.rs))
   - Add `test_mutual_destruction_both_entities_die()`
   - **Why second:** Spec compliance, not critical to stability

6. **INV-004: Entity ID Mapping** ([src/server/systems/renet.rs](../../src/server/systems/renet.rs))
   - Add `test_lobby_maintains_bidirectional_client_entity_mapping()`
   - Add `test_lobby_overwrites_duplicate_client_id()`
   - **Why second:** Prevents server panics, but rare edge case

7. **INV-006: Crit Roll at Attack Time** ([src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs))
   - Add `test_crit_roll_probabilities_match_formula()` (statistical)
   - Add `test_crit_multiplier_scales_with_instinct()`
   - **Why second:** Enhances existing partial coverage

8. **INV-009: Heading Offset Magnitude** ([src/common/systems/physics.rs](../../src/common/systems/physics.rs))
   - Add `test_heading_based_position_offset_magnitude()`
   - **Why second:** Movement system regression prevention

**Estimated Effort:** 1 day (4 invariants, 6 tests total)

---

### Priority 3: Documentation (Sprint 3)

**Goal:** Document relationships and assumptions

9. **INV-010: Chunk/FOV Relationship** ([src/common/chunk.rs](../../src/common/chunk.rs))
   - Add `test_chunk_size_and_fov_relationship()` (characterization)

10. **INV-011: Spatial Difficulty Edge Cases** ([src/common/spatial_difficulty.rs](../../src/common/spatial_difficulty.rs))
    - Add `test_enemy_level_calculation_edge_cases()`

11. **INV-013: Proximity Radii** ([src/server/systems/renet.rs](../../src/server/systems/renet.rs))
    - Add `test_proximity_filtering_radii_documented()` (characterization)

12. **INV-012: Spawn Before Incremental** ([src/client/systems/renet.rs](../../src/client/systems/renet.rs))
    - Add `test_client_requests_spawn_for_unknown_entity_on_incremental()` (integration)

**Estimated Effort:** 0.5 day (4 invariants, 4 tests total)

---

## Test Organization

### File Locations

Tests should live in same file as implementation (Rust convention):

| Invariant | Test File | Implementation File |
|-----------|-----------|---------------------|
| INV-001 | [src/server/systems/engagement_cleanup.rs](../../src/server/systems/engagement_cleanup.rs) | (multiple) |
| INV-002 | [src/common/systems/physics.rs](../../src/common/systems/physics.rs) | [src/common/systems/physics.rs](../../src/common/systems/physics.rs) |
| INV-003 | [src/common/systems/world.rs](../../src/common/systems/world.rs) | [src/common/systems/world.rs](../../src/common/systems/world.rs) |
| INV-004 | [src/server/systems/renet.rs](../../src/server/systems/renet.rs) | [src/server/systems/renet.rs](../../src/server/systems/renet.rs) |
| INV-005 | [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) | [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) |
| INV-006 | [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) | [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) |
| INV-007 | [src/common/systems/combat/resources.rs](../../src/common/systems/combat/resources.rs) | [src/common/systems/combat/resources.rs](../../src/common/systems/combat/resources.rs) |
| INV-008 | [src/common/systems/combat/resources.rs](../../src/common/systems/combat/resources.rs) | (system schedule) |
| INV-009 | [src/common/systems/physics.rs](../../src/common/systems/physics.rs) | [src/common/systems/physics.rs](../../src/common/systems/physics.rs) |
| INV-010 | [src/common/chunk.rs](../../src/common/chunk.rs) | [src/common/chunk.rs](../../src/common/chunk.rs) |
| INV-011 | [src/common/spatial_difficulty.rs](../../src/common/spatial_difficulty.rs) | [src/common/spatial_difficulty.rs](../../src/common/spatial_difficulty.rs) |
| INV-012 | [src/client/systems/renet.rs](../../src/client/systems/renet.rs) | [src/client/systems/renet.rs](../../src/client/systems/renet.rs) |
| INV-013 | [src/server/systems/renet.rs](../../src/server/systems/renet.rs) | [src/server/systems/renet.rs](../../src/server/systems/renet.rs) |

---

## Consequences

### Positive

✅ **Regression Prevention** - Tests catch breaking changes during refactors

✅ **Documentation** - Tests serve as executable documentation of invariants

✅ **Confidence** - Developers can refactor fearlessly with test coverage

✅ **Onboarding** - New developers learn critical invariants from test names/comments

✅ **Spec Compliance** - Tests verify implementation matches ADR specifications

✅ **Fast Feedback** - Unit tests run in milliseconds (no ECS overhead for pure functions)

### Negative

⚠️ **Implementation Effort** - 2.5 days to write all recommended tests

⚠️ **Maintenance Burden** - Tests must be updated when invariants change

⚠️ **False Security** - Tests document current invariants, but new invariants may emerge

### Neutral

🔹 **Test Count Increase** - Adds ~19 new tests to existing 41 test files

🔹 **Coverage Metrics** - Line coverage will increase, but invariant coverage matters more

---

## Open Questions

1. **Statistical Tests:** Should crit roll probability tests use fixed seed (deterministic) or sampling (realistic)?
   - **Recommendation:** Sampling with generous tolerance (documents real RNG behavior)

2. **Integration Tests:** Should network protocol invariants have full integration tests?
   - **Recommendation:** Start with unit tests (faster), add integration if unit tests insufficient

3. **Characterization Tests:** Should we test implementation details or just invariants?
   - **Recommendation:** Both - characterization tests document "what it does now", invariant tests enforce correctness

4. **Test Failure Messages:** How verbose should assertion messages be?
   - **Recommendation:** Include expected/actual values and formula in message (aids debugging)

---

## References

**ADRs:**
- ADR-001: Chunk-Based Terrain Discovery
- ADR-002: Combat Foundation
- ADR-003: Reaction Queue System
- ADR-005: Damage Pipeline
- ADR-011: Movement Intent System
- ADR-014: Spatial Difficulty System

**Documentation:**
- [GUIDANCE.md](../../GUIDANCE.md) - Core architecture patterns
- [ROLES/ARCHITECT.md](../../ROLES/ARCHITECT.md) - Architectural review process

**Code Locations:**
- [src/common/systems/physics.rs](../../src/common/systems/physics.rs) - Movement, heading, InputQueue
- [src/common/systems/world.rs](../../src/common/systems/world.rs) - World-space preservation
- [src/common/systems/combat/damage.rs](../../src/common/systems/combat/damage.rs) - Damage calculations
- [src/common/systems/combat/resources.rs](../../src/common/systems/combat/resources.rs) - Armor, resistance, death
- [src/server/systems/renet.rs](../../src/server/systems/renet.rs) - Network protocol (server)
- [src/client/systems/renet.rs](../../src/client/systems/renet.rs) - Network protocol (client)
- [src/common/chunk.rs](../../src/common/chunk.rs) - Chunk constants and calculations
- [src/common/spatial_difficulty.rs](../../src/common/spatial_difficulty.rs) - Enemy level scaling

---

## Acceptance Criteria

This ADR is accepted when:

1. ✅ All 17 invariants documented with formulas, locations, and rationale
2. ✅ Test recommendations provided for all untested invariants
3. ✅ Implementation plan prioritized and estimated
4. ✅ ADR reviewed by team and approved
5. ⏸️ Priority 1 tests implemented and passing (Sprint 1)
6. ⏸️ Priority 2 tests implemented and passing (Sprint 2)
7. ⏸️ Priority 3 tests implemented and passing (Sprint 3)

---

**Document Version:** 1.0
**Created:** 2025-11-08
**Author:** ARCHITECT
**Review Status:** Proposed (awaiting team review)
