# Codebase Guidance

**Read before making code changes.** Patterns, invariants, and pitfalls that prevent repeating known mistakes.

---

## Architecture Overview

Client-server MMO built with Bevy ECS. Authoritative server, client-side prediction, flat-top hexagonal grid.

**Workspace crates (all under `crates/`):**
- `common/` — Non-Bevy shared library (plate tags, hex spatial grid, pure data)
- `common-bevy/` — Bevy-specific shared code (components, chunk system, physics, messages, map)
- `client/` — Rendering, input, networking
- `server/` — Authority, AI, terrain serving, connections
- `terrain/` — Pure terrain generation library (no Bevy). See `docs/design/terrain-generation.md`.
- `terrain-viewer/` — CLI for rendering terrain layers to PNG
- `qrz/` — Hexagonal grid library. See `crates/qrz/GUIDANCE.md`.
- `console/` — Server monitoring console

---

## Invariants

**INV-001 — Ring separation (ADR-032):** Physics, movement, and pathfinding ONLY read the Map (inner ring chunks). Never read ChunkSummaries. Summaries are rendering-only.

**INV-002 — InputQueue non-empty:** All input queues must always have ≥1 entry. Violations panic.

**INV-003 — Threat timer consistency:** ALL threats from source X to target Y MUST have identical timer durations. ALWAYS use `queue_utils::create_threat()`. Never manually construct `QueuedThreat`.

**INV-004 — Chunk spatial authority:** The chunk system is the spatial authority. Never filter or classify cells by raw `wx/wy` coordinates as a substitute for chunk marking. Two spatial authority systems = bugs.

**INV-005 — Server/client eviction match:** Both use per-chunk `visibility_radius + 1`. Server uses `chunk_max_z` (superset guarantee).

---

## Patterns

**Position & Movement (ADR-019):**
- `Position { tile: Qrz, offset: Vec3 }` = server authority. `VisualPosition` = rendering interpolation only.
- `WORLD_POS = map.convert(Position.tile) + Position.offset`
- Canonical physics: `movement::calculate_movement()` in `common-bevy/systems/movement.rs`. `physics::apply()` is a thin wrapper.

**Client-Side Prediction:**
- InputQueue distinguishes local from remote players. `predict_local_player` replays queue from Position.offset → VisualPosition.
- Flow: Keys → push front → `controlled::tick` accumulates dt → server pops back → client dequeues by `seq`.

**Network Events:**
- `Try` (client→server) → server validates → `Do` (server→client broadcast). Never write `Do` events directly.

**Async Mesh Pipeline:**
- All mesh generation off main thread via `AsyncComputeTaskPool`. In-place mesh update pattern — old entity stays visible until task completes.

---

## System Execution Order

**FixedUpdate (125ms):** `controlled::apply` → `tick` → `interpolate_remote`

**Update (frame):** `renet` → `world::do_incremental` → `input::do_input` → `predict_local_player` → `advance_interpolation` → `actor::update` → `camera`

---

## Anti-Patterns

1. **Manual system ordering (`.after()`)** — 0 bugs ever fixed this way. Use `commands.get_entity()`, `Option<&Component>`, or review Try/Do flow.

2. **Forget renet updates** — When adding Events/Components, update BOTH `server/systems/renet.rs` AND `client/systems/renet.rs`.

3. **Spatial search for hex neighbors** — Neighbors are coordinate offsets, not spatial searches. 6 neighbors at fixed offsets: `(±1, 0), (0, ±1), (+1, -1), (-1, +1)`. Look up by key. Never scan rings or compute distances. Banned at every scale (macro plates, micro cells, game chunks).

4. **Test trivial code** — Test invariants and edge cases, not getters/setters. Tests should document architecture and survive refactors. For tunable systems, test shape not magnitude (ordering, monotonicity, determinism — not exact values).

5. **Confuse Position vs VisualPosition** — Position = server authority. VisualPosition = visual interpolation only.

6. **Forget world-space preservation during Loc updates** — Causes teleporting/falling.

7. **Use blended terrain near cliffs** — `blended_terrain_y` must skip neighbors with elevation_diff > 1.

8. **Mix schedules** — Rendering in Update, physics in FixedUpdate. `controlled::tick` must be Update.

9. **Pop/push queue front** — Use `front_mut()` to avoid temporary empty queue (INV-002).

---

## Renet Event Checklist

When adding Events/Components that need network sync:
1. Define `Event` in `common-bevy/message.rs`
2. Update `server/systems/renet.rs`: `on_event()` + `send_do()`
3. Update `client/systems/renet.rs`: `on_event()`
4. For component sync: Add to `Component` enum + both `Event::Incremental` handlers
