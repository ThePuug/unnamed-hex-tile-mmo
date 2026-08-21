# AGENTS.md

Read before changing code. Constraints that prevent known bugs, and the house
style for comments.

Design specs live in the `unnamed-indie-studio-internal` repo, sibling checkout,
`projects/unnamed-hex-tile-mmo/`. This repo carries only what binds the code.

| Location | Purpose |
|----------|---------|
| `design.md` | Game pitch / north-star |
| `design/` | Technical specs, one per system |
| `networking.md` | Transport, movement sync, visual interpolation |

Check the relevant spec before changing a system, and update it there when
behavior changes. Per-crate guidance sits alongside the crate it governs —
`crates/qrz/AGENTS.md` for the hex coordinate system.

## Commands

```bash
cargo build
cargo run -p server                # separate processes
cargo run -p client
cargo test                         # all tests
cargo test -p common-bevy physics  # specific module
cargo test -p server reaction_queue

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

- No history: no dates, tickets, ADR/SOW references, phase numbers, author
  names, or narration of what the code used to be. Git holds that, and holds
  deleted code — never leave a commented-out block as a record.
- Rustdoc states the contract — what it does, what breaks it. Inline comments
  state the non-obvious constraint. Nothing else gets a comment.
- Inline rationale caps at ~3 lines. Longer means it belongs in the crate or
  module doc, or the code needs restructuring.
- A comment that contradicts the code is worse than no comment. Update it in
  the same commit as the code, or delete it.

Docs describe what the code does. No deviations tables, gaps ledgers, status
checklists, phase plans, or roadmaps. If something is unbuilt, say so in one
line inline.

## Invariants

INV numbers are allocated here and nowhere else. Code cites one to say which
rule it upholds; a doc comment that mints its own number collides silently.

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

**INV-006 — LoD levels nest.** Summary scales triple (`LOD_LEVELS`), so every
coarse summary center is also a fine summary center, and `sample_center_z`'s 7
sample points at `d = scale/3` land exactly on the child level's centers. All
three producers — local `Map`, server `EventRegistry`, flyover
`AdminComposite` — use that one rule, or refinement changes the silhouette.

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

**World event system.** Events implement `WorldEvent` — required `name`,
`scale`, `deform`, `query`; optional `max_influence`, `register_indexes`,
`prepare`. A cell reads itself and one ring, never more: an event needing
wider reach needs a larger `scale`, which `max_influence` asserts at setup.
Deform places its own features from the indexes below — there is no predicate
or survey framework. Two independent cascades: deform (index→index,
structural) and query (tile→tile, vertical). Deform never materializes tiles;
query never triggers deform. Events evaluate in dependency order — event N
reads the composite of 0..N-1. Each event owns a cell grid at its own scale,
matched to feature size. Indexes are TypeId-keyed in a shared
`IndexRegistry`, cell-partitioned for spatial scoping.

**NNTree.** `common-bevy/plugins/nntree.rs` wraps `RTree<NearestNeighbor>` keyed
on `Loc`. Membership is automatic — `on_add`/`on_remove` hooks plus re-insert on
`Changed<Loc>`. Metric is Hexhattan: `max(|Δq|, |Δr|, |Δs|) + |Δz|` where
`s = -q - r`, and `distance_2` returns it squared. Queries therefore take
squared distance: `locate_within_distance(loc, 100)` searches radius 10, not
100.

## Pinned system ordering

Ordering appears in about twenty places, most of it UI setup chaining off
`camera::setup` and sequencing internal to one plugin. These three are the ones
that produce gameplay bugs when broken:

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

1. **Reaching for `.after()` first.** An apparent ordering bug is usually a
   missing entity or a misread Try/Do flow. Try `commands.get_entity()`,
   `Option<&Component>`, or tracing the Try/Do path before pinning an order —
   the orderings that genuinely earn one are listed above.
2. **Forgetting renet updates.** Adding an Event or Component means updating
   both `server/systems/renet.rs` and `client/systems/renet.rs`.
3. **Spatial search for hex neighbors.** Neighbors are coordinate offsets, not
   searches: `(±1, 0), (0, ±1), (+1, -1), (-1, +1)`. Look up by key. Never scan
   rings or compute distances — banned at every scale, from macro plates to
   chunks.
4. **Testing magnitude on a tunable system.** Test shape — ordering,
   monotonicity, determinism — never exact values.
5. **Dropping world-space during `Loc` updates.** Causes teleporting and falling.
6. **Blending toward a neighbor's raw height.** `blended_terrain_y` blends
   toward exactly ±1 `rise` using only the direction of the difference, never
   its magnitude — a 5-tile drop next door must not yank the entity down.
   Upward `elevation_diff > 1` is a separate concern, handled in
   `calculate_movement` as blocking or air-time.
7. **Mixing schedules.** `controlled::apply` and `controlled::tick` belong to
   FixedUpdate; anything touching `Transform` belongs to Update.
8. **Pop-then-push on a queue front.** Use `front_mut()` so the queue is never
   momentarily empty (INV-002).

## Writing a world event

The rules for `query`, `prepare`, and `deform` — what may read what, and what
each phase must publish — are the module doc on `crates/world/src/events/mod.rs`,
next to the trait they constrain. Read it before adding a layer.

## Renet event checklist

Adding an Event or Component that needs network sync:

1. Define `Event` in `common-bevy/message.rs`
2. `server/systems/renet.rs`: match arm in `write_try` for inbound, serialize
   arm in `send_do` for outbound
3. `client/systems/renet.rs`: match arm in `write_do`, plus the label in
   `get_message_type_name`
4. Component sync also needs the `Component` enum plus both
   `Event::Incremental` handlers
