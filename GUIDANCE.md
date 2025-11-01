# Codebase Architecture Guidance

Critical architectural information for the unnamed-hex-tile-mmo codebase.

## Development Workflow Rules

### Rule 1: Test-Driven Development (TDD)
**ALWAYS write a failing unit test before making code changes.**

1. Write test capturing expected behavior
2. Verify test fails
3. Implement fix/feature
4. Verify test passes
5. Run full test suite

### Rule 2: Re-Read GUIDANCE.md Periodically
**Re-read every 5-10 exchanges or when switching codebase areas** to maintain awareness of critical patterns and avoid documented pitfalls.

### Rule 3: Update GUIDANCE.md After Confirmed Solutions
**ONLY update AFTER user explicitly confirms the solution works.**
- Add minimum necessary to prevent future misunderstandings
- Keep concise and essential
- Do NOT commit - only update the file

## Table of Contents
- [Core Architecture](#core-architecture)
- [Game Design Specifications](#game-design-specifications)
- [Plugin Documentation](#plugin-documentation)
- [Internal Libraries](#internal-libraries)
- [Chunk-Based Terrain System](#chunk-based-terrain-system)
- [Position & Movement System](#position--movement-system)
- [Client-Side Prediction](#client-side-prediction)
- [Network Events](#network-events)
- [Key Components & Resources](#key-components--resources)
- [System Execution Order](#system-execution-order)
- [Common Pitfalls](#common-pitfalls)

---

## Core Architecture

Client-server MMO built with Bevy ECS:
- **Authoritative Server**: Server has final say on all game state
- **Client-Side Prediction**: Clients predict their own movement locally
- **Hexagonal Grid**: World uses Qrz (hexagonal) coordinates
- **Shared Systems**: Physics and behaviour run on both client and server

**Directories:**
- `src/common/`: Shared code
- `src/client/`: Client-only (rendering, input, networking)
- `src/server/`: Server-only (AI, terrain, connections)

---

## Game Design Specifications

High-level game design documents in `docs/spec/`. These define game systems (both existing and planned). Consult when designing, implementing, or understanding major features.

- **`docs/spec/triumvirate.md`**: Actor classification system (Origin, Approach, Resilience) with signature skills *(partial implementation)*
- **`docs/spec/attribute-system.md`**: Attribute pairs (Might/Grace, Vitality/Focus, Instinct/Presence), Axis/Spectrum progression *(planned)*
- **`docs/spec/hub-system.md`**: Settlement growth, influence radius, encroachment, urban cores, hub merging *(planned)*
- **`docs/spec/siege-system.md`**: Siege mechanics based on encroachment (enemy strength) vs anger (wave size) *(planned)*
- **`docs/spec/haven-system.md`**: Permanent starter havens for bootstrapping new settlements *(planned)*

**Purpose:**
- Authoritative reference for game mechanics (what the system should do)
- Living documentation that evolves with implementation
- Context for understanding design intent and relationships between systems
- Guide for future feature development

**Evolution:**
- Specs are **living documents** - updated as features are implemented and refined
- New specs added as systems are designed
- Implementation may reveal better approaches - update specs accordingly
- Specs capture design intent, implementation captures technical reality

---

## Plugin Documentation

Detailed plugin docs in `GUIDANCE/`. Consult when working on plugin-specific functionality.

- **`GUIDANCE/ControlledPlugin.md`**: Player movement, prediction, interpolation
- **`GUIDANCE/BehaviourPlugin.md`**: AI behaviors, pathfinding, target selection
- **`GUIDANCE/NNTreePlugin.md`**: Spatial queries, proximity detection
- **`GUIDANCE/DiagnosticsPlugin.md`**: Debug tools, performance monitoring
- **`GUIDANCE/UiPlugin.md`**: Game UI elements (character panel, HUD, target cursor)

---

## Internal Libraries

- **`lib/qrz/GUIDANCE.md`**: Hex coordinates, world position conversion, grid math

---

## Chunk-Based Terrain System

### Overview

Terrain is loaded dynamically in **8×8 tile chunks** to support infinite world exploration without memory issues.

**Constants** (`common/chunk.rs`):
- `CHUNK_SIZE: i16 = 8` - 8×8 tiles per chunk (64 tiles)
- `FOV_CHUNK_RADIUS: u8 = 2` - Chunks visible to player (5×5 = 25 chunks)

### Server-Side Discovery

**Flow:**
1. Player spawns → server discovers initial 25 chunks around spawn location
2. Player crosses chunk boundary → server discovers new chunks in FOV
3. Server maintains `PlayerDiscoveryState.seen_chunks` to track sent chunks

**Server-Side Eviction Tracking** (`server/systems/actor.rs::do_incremental`):
- Server mirrors client eviction logic (FOV_CHUNK_RADIUS + 1 buffer)
- When player moves, calculate which chunks client would evict
- Remove evicted chunks from `seen_chunks` so they can be re-sent
- **No additional network messages required** - inferred from player position

### Client-Side Management

**Eviction** (`client/systems/world.rs::evict_distant_chunks`):
- Runs every 5 seconds
- Keeps chunks at `FOV_CHUNK_RADIUS + 1` (3 chunk radius = 49 chunks)
- Evicts chunks outside buffer (prevents memory growth)
- Triggers mesh regeneration after eviction

**Reception** (`client/systems/renet.rs`):
- `Event::ChunkData` contains 64 tiles (Qrz + EntityType)
- Unpacked into individual `Event::Spawn` events
- Tracked in `LoadedChunks` resource

**Memory Bounds:**
- Visible: 25 chunks × 64 tiles = 1,600 tiles
- Buffered: 49 chunks × 64 tiles = 3,136 tiles max

### Key Invariant

**Server and client eviction logic MUST match exactly:**
- Both use `FOV_CHUNK_RADIUS + 1` for buffer
- Server infers eviction from player position
- Mismatch causes missing chunks or over-sending

### Camera Zoom

Max zoom out increased to 2.0x (`client/systems/camera.rs`) to observe chunk loading with smaller chunk sizes.

---

## Position & Movement System

### Offset Component - CRITICAL

```rust
pub struct Offset {
    pub state: Vec3,      // Server-authoritative position
    pub step: Vec3,       // Client prediction (local) OR interpolated position (remote)
    pub prev_step: Vec3,  // Previous frame for rendering interpolation
}
```

**`state`**: Server authority
- Local: Server's confirmed position
- Remote: Heading-based position (`HERE` distance from tile center toward heading)
- Updated by: Server confirmations, `world.rs::do_incremental()`

**`step`**: Visual position (what gets rendered)
- Local: Client-predicted position
- Remote: Current interpolated position (moving toward `state`)

**`prev_step`**: Previous frame's `step` for smooth interpolation

**Position Formula**: `WORLD_POS = map.convert(Loc) + Offset.step`

### Movement Flows

**Local Player:**
1. Input → prediction queue → physics updates `step`
2. Server confirms → remove from queue
3. Tile crossing → preserve world position when updating `Loc`

**Remote Player:**
1. Server sends `Loc`/`Heading` → `state` recalculated
2. `interpolate_remote()` moves `step` toward `state`

---

## Client-Side Prediction

### Input Queue

```rust
pub struct InputQueue {
    pub queue: VecDeque<Event>,  // FIFO: back=oldest, front=newest
}
```

**Key points:**
- Local players have queues, remote players don't (this distinguishes them)
- Check local: `buffers.get(&entity).is_some()`
- Invariant: **Queues always contain ≥1 input** (front accumulates time)
- Sequence numbers wrap at 256 (u8)

**Flow:**
1. Keys change OR 1-sec periodic → create `Event::Input`
2. Push to front with `dt=0`
3. `controlled::tick` accumulates frame `dt` on front (use `front_mut()`, never pop/push)
4. Server pops back → sends confirmation
5. Client receives → searches queue by `seq` → removes

**Why periodic**: Prevent `dt` overflow (u16 max = 65s)

---

## Network Events

**`Try`**: Client → Server requests
**`Do`**: Server → Client confirmations/broadcasts

**Common patterns:**
- `Event::Input`: Input with sequence number for prediction/confirmation
- `Event::Incremental`: Component updates (`Loc`, `KeyBits`, `Heading`)
- `Event::Spawn`: Entity spawning

---

## Key Components & Resources

**Components:**
- `Loc`: Hex tile position (wraps `Qrz`)
- `Offset`: Sub-tile position (see above)
- `Behaviour::Controlled`: Player-controlled entity
- `KeyBits`: Bitfield of pressed keys
- `Heading`: Facing direction (also affects positioning for stationary players)
- `AirTime`: Jump/fall state (`None`=grounded, `Some(+)`=jump, `Some(-)`=fall)
- `Physics`: Marker for physics simulation
- `ActorAttributes`: Configurable attributes (e.g., `movement_speed`)

**Resources:**
- `InputQueues`: Maps `Entity` → `InputQueue`
- `EntityMap` (client): Maps remote entity IDs → local entity IDs
- `Map`: Hex world map with `convert()` for hex ↔ world space
- `NNTree`: R*-tree for spatial queries

---

## System Execution Order

**FixedUpdate (125ms):**
1. `controlled::apply` - Apply physics to controlled entities
2. `controlled::tick` - Accumulate time on inputs
3. `controlled::interpolate_remote` - Interpolate remote toward state
4. `physics::update` - Run physics simulation

**Update (every frame):**
1. `renet::do_manage_connections` - Network events
2. `world::do_incremental` - Process server updates
3. `input::do_input` - Process confirmations
4. `actor::update` - Interpolate for rendering
5. `camera::update` - Update camera

**Loc Update Handling:**
- Local: Preserve ALL offset fields in world space, re-express in new tile coords
- Remote: Preserve visual (`step`, `prev_step`); recalculate `state` from heading

---

## Common Pitfalls

### Critical DO NOTs

1. **Confuse `state` vs `step`**: `state`=server authority, `step`=prediction/interpolation
2. **Forget world-space preservation during Loc updates**: Causes teleporting/falling
3. **Mix schedules**: Rendering in Update, physics in FixedUpdate
4. **Remove periodic KeyBits updates**: Prevents dt overflow
5. **Expect perfect queue sync**: 1-3 input latency is normal
6. **Apply heading positioning in rendering**: It's physics concern (`physics::apply`)
7. **Check offset magnitude for stationary**: Use `KeyBits`, not offset
8. **Set remote `state` to Vec3::ZERO**: Must be heading-based position
9. **Pop/push queue front**: Use `front_mut()` to avoid temporary empty queue
10. **Run `controlled::tick` in FixedUpdate**: Must be Update, `.after(update_keybits)`
11. **Use manual system ordering (`.after()`) to fix bugs**: Almost never the answer - investigate root cause instead. Bevy commands flush automatically between system sets. If components are missing, they'll exist by next frame. Manual ordering is a code smell indicating architectural issues.

### Performance Patterns

**Prefer `retain()` over collect-filter-remove**:
```rust
// Bad: 3 allocations (HashSet, Vec, loop removes)
let kept: HashSet<_> = calculate_kept().collect();
let to_remove: Vec<_> = set.iter().filter(|x| !kept.contains(x)).collect();
for x in to_remove { set.remove(x); }

// Good: 1 allocation (HashSet only)
let kept: HashSet<_> = calculate_kept().collect();
set.retain(|x| kept.contains(x));
```

### Player Detection
`Behaviour` is enum, not marker. Filter for `Behaviour::Controlled` specifically, not `With<Behaviour>`.

### NPC Teleporting
Clear all `Offset` fields (`state`, `step`, `prev_step`) when teleporting NPCs.

### Distance Checks
Use `>` not `>=` for "beyond distance" semantics.

---

## Physics Constants

See `common/systems/physics.rs` for values (GRAVITY, JUMP_*, MOVEMENT_SPEED, etc.)

---

## Testing

```bash
cargo test               # All tests
cargo test physics       # Physics tests
cargo test behaviour     # Behaviour tests
```
