# Codebase Architecture Guidance

**CRITICAL: Re-read this document every 5-10 exchanges or when switching codebase areas.**

This prevents repeating documented mistakes and architectural violations.

## Contents
[Core Architecture](#core-architecture) • [Specs & Docs](#game-design-specifications) • [Chunks](#chunk-based-terrain-system) • [Position/Movement](#position--movement-system) • [Prediction](#client-side-prediction) • [Components](#key-components--resources) • [System Order](#system-execution-order) • [Pitfalls](#common-pitfalls)

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

**Location:** `docs/00-spec/` - Authoritative game design (what systems should do)
**Location:** `GUIDANCE/` - Plugin implementation details
**Location:** `lib/qrz/GUIDANCE.md` - Hex coordinate system

---

## Chunk-Based Terrain System

**Constants:** `CHUNK_SIZE=8` (64 tiles), `FOV_CHUNK_RADIUS=2` (25 visible chunks)

**Server:** Discovers chunks in FOV when player moves. Mirrors client eviction (`FOV_CHUNK_RADIUS + 1`) to know when to re-send chunks. No network messages for eviction - inferred from position.

**Client:** Evicts chunks outside `FOV_CHUNK_RADIUS + 1` every 5 seconds (max 49 chunks = 3,136 tiles). Receives `Event::ChunkData` (64 tiles) unpacked to individual spawns.

**Critical Invariant:** Server and client eviction logic MUST match exactly (both use `FOV_CHUNK_RADIUS + 1`).

---

## Position & Movement System

**Offset Component:**
- `state`: Server authority (confirmed for local, heading-based for remote)
- `step`: Visual position (predicted for local, interpolated for remote)
- `prev_step`: Previous `step` for smooth rendering

**Formula:** `WORLD_POS = map.convert(Loc) + Offset.step`

**Local:** Input → queue → physics updates `step` → server confirms → dequeue
**Remote:** Server sends `Loc`/`Heading` → `state` recalculated → `step` interpolates toward `state`

---

## Client-Side Prediction

**InputQueue:** Local players only (distinguishes from remote). Invariant: ≥1 input always (front accumulates time). Check local: `buffers.get(&entity).is_some()`

**Flow:** Keys change/1-sec periodic → push front → `controlled::tick` accumulates dt on front → server pops back → client dequeues by `seq`

**Network Events:** `Try` (client→server), `Do` (server→client broadcast)

---

## Key Components & Resources

**Components:** `Loc` (hex tile), `Offset` (sub-tile), `Behaviour::Controlled` (player), `KeyBits`, `Heading`, `AirTime`, `Physics`, `ActorAttributes`

**Resources:** `InputQueues`, `EntityMap` (client), `Map` (hex↔world), `NNTree` (spatial queries)

## System Execution Order

**FixedUpdate (125ms):** `controlled::apply` → `tick` → `interpolate_remote` → `physics::update`
**Update (frame):** `renet` → `world::do_incremental` → `input::do_input` → `actor::update` → `camera`

---

## Common Pitfalls

### Critical Anti-Patterns (NEVER Do These)

1. **❌ Manual system ordering (`.after()`)** - **0 bugs ever fixed this way**. Bevy flushes commands automatically. Fix: Use `commands.get_entity()`, `Option<&Component>`, or review Try/Do flow.

2. **❌ Forget renet updates** - When adding Events/Components, update BOTH `server/systems/renet.rs` AND `client/systems/renet.rs` (`on_event()` + `send_do()` handlers).

3. **❌ Bypass Try/Do pattern** - ALWAYS: Client→Try → Server validates → Server→Do broadcast. Never write `Do` events directly.

4. **❌ Test trivial code** - Test invariants ("PROXIMITY_RANGE > eviction distance"), not getters/setters. Tests should document architecture and survive refactors.

### Position/Movement Pitfalls

5. **Confuse `state` vs `step`**: `state`=server authority, `step`=prediction/interpolation
6. **Forget world-space preservation during Loc updates**: Causes teleporting/falling
7. **Set remote `state` to Vec3::ZERO**: Must be heading-based position
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

See `common/systems/physics.rs` for values (GRAVITY, JUMP_*, MOVEMENT_SPEED, etc.)

---

## Testing

```bash
cargo test               # All tests
cargo test physics       # Physics tests
cargo test behaviour     # Behaviour tests
```
