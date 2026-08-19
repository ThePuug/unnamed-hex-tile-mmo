# AGENTS.md

Read before changing code. Constraints that prevent known bugs, and the house
style for comments.

Design specs live in the `unnamed-indie-studio-internal` repo, sibling checkout,
`projects/unnamed-hex-tile-mmo/`. This repo carries only what binds the code.

## Commands

```bash
cargo build
cargo run -p server                # separate processes
cargo run -p client
cargo test                         # all tests
cargo test -p common physics       # specific module
cargo test -p server actor

cargo build --release --no-default-features -p server -p client   # optimized
```

Bevy links as a dylib by default (`dynamic` feature) so an edit rebuilds one
crate instead of the engine. That dylib carries no bitcode, so it cannot be
linked under the release profile's LTO — every static it owns comes out
undefined. `--no-default-features` drops it; the client also loses `admin`,
which is what a shipping build wants. `-p world` has no Bevy dependency, so its
release builds keep LTO either way.

## Crates

Client-server MMO on Bevy ECS. Authoritative server, client-side prediction,
flat-top hex grid. All crates under `crates/`:

| Crate | Role |
|-------|------|
| `common` | Non-Bevy shared library — plate tags, hex spatial grid, pure data |
| `common-bevy` | Shared Bevy code — components, chunks, physics, messages, map |
| `client` | Rendering, input, networking |
| `server` | Authority, AI, terrain serving, connections |
| `world` | World event system + terrain generation, no Bevy |
| `world-viewer` | CLI rendering world composite layers to PNG |
| `qrz` | Hex grid library — see `crates/qrz/AGENTS.md` |
| `console` | Server monitoring console |

## Comments and docs

Self-documenting bodies, documented contracts. A better name beats a comment;
an interface still needs its contract stated, because a signature cannot carry
preconditions or valid-call conditions.

**State the constraint, not the decision.** A comment earns its place by saying
something that is true now and would cost time to rediscover — never by
narrating how the code got here.

```rust
// i64 because rstar multiplies dimension spans internally and overflows i32
// on large maps.                                                  // good

// Changed from i32 to i64 during the perf pass after we hit overflow.
//                                                                 // bad
```

Rules:

- No dates, ticket numbers, ADR/SOW references, phase numbers, or author names.
- No "previously", "used to", "now", "was", "currently". Present tense only.
- Never leave commented-out code as a record. Git holds history.
- Do not restate what the code already shows.
- Inline rationale caps at ~3 lines. Longer means it belongs in the crate or
  module doc, or the code needs restructuring.
- Rustdoc states the contract — what it does, what breaks it. Inline comments
  state the non-obvious constraint. Nothing else gets a comment.
- A comment that contradicts the code is worse than no comment. Update it in
  the same commit as the code, or delete it.

Docs describe what the code does. No deviations tables, gaps ledgers, status
checklists, phase plans, or roadmaps. If something is unbuilt, say so in one
line inline.

## Invariants

**INV-001 — Summary separation.** Summary caches are rendering-only:
`client::resources::SummaryCache`, `server::resources::summary_cache::SummaryCache`,
`server::systems::summary::VisibleSummaryCache`. Physics, movement, and
pathfinding read the `Map`, never a summary cache.

**INV-002 — InputQueue non-empty.** Every input queue holds ≥1 entry.
Violations panic.

**INV-003 — Threat timer consistency.** All threats from source X to target Y
carry identical timer durations. Use `queue_utils::create_threat()`; never
construct `QueuedThreat` directly.

**INV-004 — Chunk spatial authority.** The chunk system is the spatial
authority. Never filter or classify cells by raw `wx/wy` as a substitute for
chunk marking. Two spatial authorities produce bugs.

**INV-005 — Server-authoritative eviction.** The server decides which chunks a
client drops and says so with `Event::EvictChunks`; the client never evicts tile
data on its own. `server::systems::actor::do_incremental` diffs
`calculate_visible_chunks(new_chunk, FIXED_STREAM_RADIUS)` against
`VisibleChunkCache.sent`; `client::systems::world::evict_data` consumes it. One
authority prevents drift over which chunks are loaded.

## Patterns

**Position and movement.** `Position { tile: Qrz, offset: Vec3 }` is server
authority; `VisualPosition` is rendering interpolation only.
`WORLD_POS = map.convert(Position.tile) + Position.offset`. Canonical physics is
`movement::calculate_movement()`; `physics::apply()` is a thin wrapper.

**Client-side prediction.** `InputQueue` distinguishes local from remote
players. `predict_local_player` replays the queue from `Position.offset` into
`VisualPosition`. Keys push front, `controlled::tick` accumulates dt, the server
pops back, the client dequeues by `seq`.

**Network events.** `Try` (client→server) → server validates → `Do`
(server→client broadcast). Never write `Do` directly.

**Async mesh pipeline.** All mesh generation runs off the main thread via
`AsyncComputeTaskPool`. The old entity stays visible until its task completes.

**World event system.** Events implement `WorldEvent` (name, scale, survey,
deform, query). Two independent cascades: deform (index→index, structural) and
query (tile→tile, vertical). Deform never materializes tiles; query never
triggers deform. Events evaluate in dependency order — event N reads the
composite of 0..N-1. Each event owns a cell grid at its own scale, matched to
feature size. Indexes are TypeId-keyed in a shared `IndexRegistry`,
cell-partitioned for spatial scoping.

**NNTree.** `common-bevy/plugins/nntree.rs` wraps `RTree<NearestNeighbor>` keyed
on `Loc`. Membership is automatic — `on_add`/`on_remove` hooks plus re-insert on
`Changed<Loc>`. Metric is Hexhattan: `max(|Δq|, |Δr|, |Δs|) + |Δz|` where
`s = -q - r`, and `distance_2` returns it squared. Queries therefore take
squared distance: `locate_within_distance(loc, 100)` searches radius 10, not
100. `Point::Scalar` is `i64` because rstar multiplies dimension spans when
computing AABB area, which overflows `i32` on large maps.

## Pinned system ordering

Only three orderings are pinned, each because breaking it produces a bug:

- `input::do_input.after(controlled::tick)` — dt must accumulate before inputs
  are consumed.
- `advance_interpolation.before(actor::update)` — `VisualPosition` advances
  before `Transform` reads `current()`.
- `world::do_incremental.after(actor::apply_movement_intent)` —
  `MovementPrediction` must exist when the confirming `Loc` arrives, or the
  no-prediction fallback sets a wrong visual target.

Remote-entity interpolation is not its own system: `apply_movement_intent` seeds
`VisualPosition`, `actor::update` renders it.

## Anti-patterns

1. **Manual system ordering.** `.after()` has never fixed a bug here. Use
   `commands.get_entity()`, `Option<&Component>`, or review the Try/Do flow.
2. **Forgetting renet updates.** Adding an Event or Component means updating
   both `server/systems/renet.rs` and `client/systems/renet.rs`.
3. **Spatial search for hex neighbors.** Neighbors are coordinate offsets, not
   searches: `(±1, 0), (0, ±1), (+1, -1), (-1, +1)`. Look up by key. Never scan
   rings or compute distances — banned at every scale, from macro plates to
   chunks.
4. **Testing trivial code.** Test invariants and edge cases, not accessors. For
   tunable systems test shape, not magnitude — ordering, monotonicity,
   determinism, never exact values.
5. **Confusing `Position` with `VisualPosition`.** Authority versus render.
6. **Dropping world-space during `Loc` updates.** Causes teleporting and falling.
7. **Blending toward a neighbor's raw height.** `blended_terrain_y` blends
   toward exactly ±1 `rise` using only the direction of the difference, never
   its magnitude — a 5-tile drop next door must not yank the entity down.
   Upward `elevation_diff > 1` is a separate concern, handled in
   `calculate_movement` as blocking or air-time.
8. **Mixing schedules.** `controlled::apply` and `controlled::tick` belong to
   FixedUpdate; anything touching `Transform` belongs to Update.
9. **Pop-then-push on a queue front.** Use `front_mut()` so the queue is never
   momentarily empty (INV-002).

## Renet event checklist

Adding an Event or Component that needs network sync:

1. Define `Event` in `common-bevy/message.rs`
2. `server/systems/renet.rs`: `on_event()` + `send_do()`
3. `client/systems/renet.rs`: `on_event()`
4. Component sync also needs the `Component` enum plus both
   `Event::Incremental` handlers
