# Codebase Architecture Guidance

**CRITICAL: Re-read this document every 5-10 exchanges or when switching codebase areas.**

This prevents repeating documented mistakes and architectural violations.

## Contents
[Core Architecture](#core-architecture) • [Specs & Docs](#game-design-specifications) • [Chunks](#chunk-based-terrain-system) • [Terrain Generation](#terrain-generation) • [Position/Movement](#position--movement-system) • [Prediction](#client-side-prediction) • [Components](#key-components--resources) • [System Order](#system-execution-order) • [Pitfalls](#common-pitfalls)

---

## Core Architecture

Client-server MMO built with Bevy ECS:
- **Authoritative Server**: Server has final say on all game state
- **Client-Side Prediction**: Clients predict their own movement locally
- **Hexagonal Grid**: World uses Qrz (hexagonal) coordinates
- **Shared Systems**: Physics and behaviour run on both client and server

**Workspace crates (all under `crates/`):**
- `crates/common/`: Shared library crate (components, physics, messages)
- `crates/client/`: Client binary crate (rendering, input, networking)
- `crates/server/`: Server binary crate (AI, terrain serving, connections)
- `crates/terrain/`: Terrain generation library (pure functions, no Bevy dependency)
- `crates/terrain-viewer/`: Headless CLI tool for rendering terrain heightmaps to PNG
- `crates/qrz/`: Hexagonal grid library
- `crates/console/`: Server monitoring console

---

## Game Design Specifications

**Location:** `docs/design/` - Authoritative game design (what systems should do)
**Location:** `GUIDANCE/` - Plugin implementation details
**Location:** `crates/qrz/GUIDANCE.md` - Hex coordinate system

---

## Chunk-Based Terrain System

**Constants:** `CHUNK_SIZE=16` (256 tiles), `FOV_CHUNK_RADIUS=5` (base visible radius / LoD boundary), `MAX_TERRAIN_CHUNK_RADIUS=12`

**Adaptive loading:** `visibility_radius(player_z, ground_z, half_viewport)` computes per-chunk visibility using orthographic ray-ground intersection. Chunks below the player extend further; chunks at or above player elevation stay at base radius. Discovery shape is asymmetric — extends toward valleys, tight toward ridges.

**Two-ring LoD (ADR-032):** Chunks split into inner ring (≤ `FOV_CHUNK_RADIUS`, full 64-tile detail) and outer ring (beyond, summary hexes only). `calculate_visible_chunks_adaptive` returns `(inner, outer)`. Inner ring tiles go in the Map; outer ring summaries go in `ChunkSummaries` resource. Physics/movement/pathfinding ONLY read the Map — never summaries.

**ChunkSummary:** ~12 bytes per chunk (chunk_id, average elevation, dominant biome). Server sends `Event::ChunkSummary` for outer ring, `Event::ChunkData` for inner ring. Ring transitions: inner↔outer sends the appropriate message type; chunk is never absent from both.

**Summary mesh:** 7-vertex hex — center at chunk average elevation, 6 corners averaged across 3 neighboring chunks. Deferred if neighbor summaries missing. Produces continuous terrain silhouette at distance.

**Server:** `VisibleChunkCache` component caches inner/outer ring sets plus eviction mirrors, recomputed on chunk boundary crossings using `chunk_max_z` (conservative). `do_incremental` handles four transition types: enter inner, enter outer, inner↔outer upgrade/downgrade, leave outer. Mirrors client per-chunk eviction logic via `compute_eviction_set`.

**Client:** `evict_distant_chunks` (5s timer) runs two passes: (1) full-detail chunks beyond `FOV_CHUNK_RADIUS + 1` evicted from Map, (2) summary chunks beyond per-chunk `visibility_radius + 1` evicted from `ChunkSummaries`.

**Async mesh pipeline:** All mesh generation runs off the main thread via `AsyncComputeTaskPool`. Full-detail: `PendingChunkMeshes` (dispatch in `spawn_missing_chunk_meshes`, poll in `poll_chunk_mesh_tasks`). Summary: `PendingSummaryMeshes` (dispatch in `spawn_summary_meshes`, poll in `poll_summary_mesh_tasks`). Flyover tile generation: `PendingFlyoverTiles` (dispatch in `flyover_generate_chunks`, poll in `poll_flyover_tile_tasks`). When regenerating an existing mesh, the old entity stays visible and its mesh asset is updated in place when the task completes — no visual gap.

**Critical Invariant:** Server and client eviction logic MUST match — both use per-chunk `visibility_radius + 1`. Server uses `chunk_max_z` (superset guarantee). Ring separation: summaries are rendering-only, never gameplay data.

---

## Terrain Generation

**Library:** `crates/terrain/` — Pure functions, no Bevy dependency. `(position, seed, tick) → value`.

**Three-layer pipeline:**
1. **Material** (`material.rs`): Simplex noise → density in [0, 1]. Three discordant wavelengths (12.5k, 20.3k, 32.8k tiles). Dense regions (≥ 0.55) support hotspots; light regions are quiescent.
2. **Hotspots** (`hotspots.rs`): Fixed grid (750-tile spacing) of convection cells under dense lid. Asymmetric lifecycle: 60% rise, 10% peak, 30% collapse over 1000 ticks. Diagnostic layer only.
3. **Thermal** (`thermal.rs`): Active hotspot cells become point sources. Intensity = `exp(-lid * 8) * lifecycle * 0.12`. Additive Gaussian diffusion (σ=1200 tiles). Sum clamped to [0, 1]. This is the primary terrain signal.

**Hex Voronoi chunk caching (ADR-033):** Both hotspot and thermal layers cache data in hexagonal chunks assigned via cube-coordinate rounding (`hex_round`). Each query gathers center + 6 hex neighbors (7 chunks). No diagonal gaps. Shared utility: `tile_to_hex_chunk(q, r, spacing)` in `lib.rs`.

**Boundary invariant:** `missed_sources_beyond_neighborhood_are_negligible` test proves ring-2 sources contribute < 1%. Chunk size ≥ 3σ ensures this.

**Key constants:** `HOTSPOT_THRESHOLD=0.55`, `HOTSPOT_GRID_SPACING=750`, `THERMAL_SIGMA=1200`, `MAX_SOURCE_INTENSITY=0.12`, `LID_SUPPRESSION=8.0`, `THERMAL_CHUNK_SIZE=4500`.

**Server wrapper:** `crates/server/src/resources/terrain.rs` — thin Bevy Resource wrapping `terrain::Terrain`.

**Viewer:** `crates/terrain-viewer/` — CLI renders terrain to PNG (modes: Material, Hotspots, Thermal).

**Height is placeholder** — `get_height` returns 0. Elevation system will rebuild on top of material + thermal.

---

## Position & Movement System (ADR-019)

**Components:**
- `Position { tile: Qrz, offset: Vec3 }` — Authoritative position (physics/server truth)
- `VisualPosition { from, to, progress, duration }` — Visual interpolation (rendering only)

**Formula:** `WORLD_POS = map.convert(Position.tile) + Position.offset`

**Canonical physics:** `movement::calculate_movement()` in `common/systems/movement.rs`
- `physics::apply()` is a thin delegation wrapper
- Pure function: `(MovementInput, dt, map, nntree) -> MovementOutput`
- Terrain following uses `blended_terrain_y` for smooth slopes, skips cliff neighbors (elevation_diff > 1)

**Local:** Input → queue → `controlled::apply` calls `physics::apply` → server confirms → dequeue
**Remote:** Server sends `Loc`/`Heading` → Position updated → VisualPosition interpolates toward new world pos

---

## Client-Side Prediction

**InputQueue:** Local players only (distinguishes from remote). Invariant: ≥1 input always (front accumulates time). Check local: `buffers.get(&entity).is_some()`

**Flow:** Keys change/1-sec periodic → push front → `controlled::tick` accumulates dt on front → server pops back → client dequeues by `seq`

**Prediction:** `predict_local_player` (client/systems/prediction.rs) replays InputQueue from Position.offset → VisualPosition

**Network Events:** `Try` (client→server), `Do` (server→client broadcast)

---

## Key Components & Resources

**Components:** `Loc` (hex tile), `Position` (tile + sub-tile offset), `VisualPosition` (interpolation), `Behaviour::Controlled` (player), `KeyBits`, `Heading`, `AirTime`, `Physics`, `ActorAttributes`

**Resources:** `InputQueues`, `EntityMap` (client), `Map` (hex↔world), `NNTree` (spatial queries)

---

## Combat & Threat System

**ReactionQueue:** Incoming threats that must be resolved. Window size from Focus commitment tier (1-4 slots visible).

**Critical Invariant (INV-003): Threat Timer Consistency**
- **Rule:** ALL threats from source X to target Y MUST have identical timer durations
- **Why:** Ensures predictable reaction windows regardless of ability used
- **How:** ALWAYS use `queue_utils::create_threat()` helper when creating threats
- **Never:** Manually construct `QueuedThreat` or calculate custom timers per ability

**Timer Formula (3 components):**
1. Base timer: `target.instinct + target.level_multiplier`
2. Gap multiplier: `(1.0 + level_gap × 0.15).min(3.0)` — up to 3× when defender outlevels attacker
3. Cunning extension: `target.cunning × 2ms × contest(cunning, source.finesse)` — up to 600ms

**Example:** Level 1 NPC → Level 50 player = ~23 second window (regardless of ability)

---

## System Execution Order

**FixedUpdate (125ms):** `controlled::apply` → `tick` → `interpolate_remote`
**Update (frame):** `renet` → `world::do_incremental` → `input::do_input` → `predict_local_player` → `advance_interpolation` → `actor::update` → `camera`

---

## Common Pitfalls

### Critical Anti-Patterns (NEVER Do These)

1. **❌ Manual system ordering (`.after()`)** - **0 bugs ever fixed this way**. Bevy flushes commands automatically. Fix: Use `commands.get_entity()`, `Option<&Component>`, or review Try/Do flow.

2. **❌ Forget renet updates** - When adding Events/Components, update BOTH `server/systems/renet.rs` AND `client/systems/renet.rs` (`on_event()` + `send_do()` handlers).

3. **❌ Bypass Try/Do pattern** - ALWAYS: Client→Try → Server validates → Server→Do broadcast. Never write `Do` events directly.

4. **❌ Manual threat construction** - NEVER manually construct `QueuedThreat` in abilities. ALWAYS use `queue_utils::create_threat()` to ensure consistent timers (INV-003). Bypassing this breaks reaction window predictability.

4. **❌ Test trivial code** - Test invariants ("PROXIMITY_RANGE > eviction distance"), not getters/setters. Tests should document architecture and survive refactors.

### Position/Movement Pitfalls

5. **Confuse Position vs VisualPosition**: Position=server authority, VisualPosition=visual interpolation only
6. **Forget world-space preservation during Loc updates**: Causes teleporting/falling
7. **Use blended terrain near cliffs**: `blended_terrain_y` must skip neighbors with elevation_diff > 1 to avoid bypassing cliff blocking
8. **Apply heading positioning in rendering**: It's physics concern (`physics::apply`)

### Other Common Mistakes

9. **Mix schedules**: Rendering in Update, physics in FixedUpdate
10. **Remove periodic KeyBits updates**: Prevents dt overflow
11. **Pop/push queue front**: Use `front_mut()` to avoid temporary empty queue
12. **Run `controlled::tick` in FixedUpdate**: Must be Update
13. **Check offset magnitude for stationary**: Use `KeyBits`, not offset
14. **Filter `With<Behaviour>`**: It's an enum - filter `Behaviour::Controlled` specifically

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

### Renet Event Checklist

When adding Events/Components that need network sync:
1. Define `Event` in `common/message.rs`
2. Update `server/systems/renet.rs`: `on_event()` + `send_do()`
3. Update `client/systems/renet.rs`: `on_event()`
4. For component sync: Add to `Component` enum + both `Event::Incremental` handlers

### Test Invariants, Not Trivia

**Good tests:**
- Architectural invariants: "PROXIMITY_RANGE > eviction distance" (prevents ghost NPCs)
- Edge cases: Boundary conditions, empty collections, max values
- Critical paths: Network sync, physics, core loops
- Document WHY invariant matters in failure message

**Bad tests:**
- Getters/setters, trivial constructors
- Implementation details that break on refactor
- Testing that value assigned equals value retrieved

---

## Physics Constants

See `common/systems/movement.rs` for values (GRAVITY, JUMP_*, MOVEMENT_SPEED, SLOPE_FOLLOW_SPEED, etc.)

---

## Testing

```bash
cargo test               # All tests
cargo test physics       # Physics tests
cargo test behaviour     # Behaviour tests
```
