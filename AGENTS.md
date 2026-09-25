# AGENTS.md

Read before changing code. Constraints that prevent known bugs, and the house
style for comments.

Design specs live in the `unnamed-indie-studio-internal` repo, sibling checkout,
`projects/unnamed-hex-tile-mmo/`. This repo carries only what binds the code.
Assets and their generators (texgen, modelgen) live in the private
`unnamed-hex-tile-mmo-assets` repo, mounted here as the `assets/` submodule; its
own `AGENTS.md` binds them.

| Location | Purpose |
|----------|---------|
| `design.md` | Game pitch / north-star |
| `design/` | Technical specs, one per system — `movement.md` covers heading, physics, input timing and remote simulation; `networking.md` covers transport and what crosses the wire |

Check the relevant spec before changing a system, and update it there when
behavior changes. Per-crate guidance sits alongside the crate it governs —
`crates/qrz/AGENTS.md` for the hex coordinate system.

## Commands

```bash
cargo build
cargo run --bin server             # separate processes
cargo run --bin client
cargo run --bin server -- arena      # archetype v archetype balance, headless; keys in server/src/arena.rs
cargo test                         # all tests
cargo test -p common-bevy physics  # specific module
cargo test -p server reaction_queue

cargo build --release --no-default-features -p server -p client   # optimized
```

Run binaries with `--bin`, not `-p`. Selecting one package resolves shared
dependencies (`syn`, `image`, `winit`, …) with a feature set no workspace-wide
`cargo build` or `cargo test` produces, and a binary's fingerprint records the
dependency set it was linked against, so every switch between `-p` and a
workspace command recompiles the binary.

Bevy links as a dylib by default (`dynamic` feature) so an edit rebuilds one
crate instead of the engine. That dylib carries no bitcode, so it cannot be
linked under the release profile's LTO — every static it owns comes out
undefined. `--no-default-features` drops it; the client also loses `admin`,
which is what a shipping build wants. `-p world` has no Bevy dependency, so its
release builds keep LTO either way.

`Cargo.lock` pins `windows` to 0.62.0, and a bare `cargo update` unpins it:
`gpu-allocator` accepts `<=0.62`, which excludes 0.62.2, while `wgpu-hal`
asks for `^0.62` and takes it, leaving the dx12 backend compiled against two
incompatible sets of D3D12 types. Update a package at a time, and if the
backend stops compiling over `ID3D12Device` or `ResourceCategory`, this is
why.

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
| `world-viewer` | CLI rendering the event stack, or one event's field or index, to an image; its README says what a view may read |
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

**INV-007 — Client-timed input is credit-bound.** Every millisecond of
player physics the server runs came from a `Try::Input` the client timed,
accepted by `server::systems::input::InputGuard` against a credit that
refills from `Time<Real>` and is capped. The server never times a player
input itself and never applies one that was not accepted. Refilling from the
virtual clock would let one long server frame clamp every honest client.

**INV-008 — Player changes are laid over every tile built.** A tile is its
generated cover with what players changed laid over it. Every path that
builds a tile for the map or the wire takes it through `laid_over`:
`server::systems::gathering::WorldChanges` in `actor::merge_and_pack`,
`client::systems::gathering::CoverChanges` in `world::do_spawn`. A
summary reads its samples through `WorldChanges::over`, in
`summary::dispatch_summary_tasks` and `summary::revise_summaries`. A path
that skips it serves the tree a player felled.

## Patterns

**Position and movement.** `Position { tile: Qrz, offset: Vec3 }` is
server authority; `VisualPosition` is rendering interpolation only.
`WORLD_POS = map.convert(Position.tile) + Position.offset`; the offset may
leave the tile, and `Position::rebase` moves it onto the tile it `reached`
— on the server in `actor::update`, on the client in `movement::do_loc`.
Movement is `Heading` (24 bearings) × speed × dt in
`movement::calculate_movement()`, the canonical physics;
`physics::apply()` is a thin wrapper for NPCs. Turning is physics too: a
held turn key steps the heading once per `TURN_REPEAT_MS` of input time
inside the same loop, with `Turn` carrying the clock, so the wire carries
keys and never a heading. The result must not depend on how dt is
partitioned — no per-call smoothing, no per-step constant unscaled by dt —
because the client replays in different slices what the server applied.
Inside a tile it may enter, a player's pill goes round the footprint of
each solid object above its waist, each as wide as it is drawn from the
shared forms (`movement::footprints`, `common::cover::TreeForm`), and
anything else walks through: the slide on the circle is solved in closed
form (`movement::round`), never stepped, so it holds to the same rule.

**Render origin.** The client draws about `client::resources::RenderOrigin`,
a tile near the player at `z = 0`. A rendered position is
`map.convert(tile − origin.tile) + offset`, never `map.convert(tile) − origin`:
the tile difference is exact, so the offset keeps its precision however far
out the world is, where a world vector tens of thousands of units out keeps
only millimetres and an actor's joints shake on screen. `rebase_origin` moves
it in PreUpdate and shifts every root `Transform` and `VisualPosition` in the
same pass. `Transform`, `VisualPosition` and the shaders' world position are
rendered coordinates; `Position`, `Loc`, the `Map`, the region lattice and the
wire are world. Cross at the read with `origin.render*` and `origin.world`, as
`camera::update` and `world::update_terrain_cut` do: a system that reads the
actor's `Transform` for the map has the origin to add.

**Client-side prediction.** `InputQueue` distinguishes local from remote
players. `input::update_keybits` pushes a new `seq` at the front on any key
change, `input::tick` attributes the fixed tick to the front and puts it on
the wire, the server's `input::apply` runs exactly that dt and answers
`Event::Confirm` with its `Position` and `Turn` when the seq closes, and
`input::do_confirm` pops the back by `seq` and adopts them.
`movement::predict_local_player` replays the queue from `Position` and `Turn`
into `VisualPosition` and `Heading`. Remote entities run the same physics in
`movement::simulate_remote` from their last `MovementIntent`.

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

**Clips.** An actor's GLB names its animations — `_tee`, `idle`, `walk`,
`run`, `jump`, `attack`, `counter`, `chop`, `mine`, `pickup` — and
`client::systems::animator::Clips` finds each by name from
`Gltf::named_animations` when the scene is ready, so a new clip is a new
name and an actor lacking one has no node for it; the actor holds its
`Gltf` root (`animator::Rig`) from spawn or the names are gone by then. A
jump clip never rises: the armature node's `animgen` extras declare
`leave`, `freeze` and `land` in seconds, and `animator::Jumping` plays it
to the freeze as `AirTime` goes airborne, holds it there by speed (never
`pause`, which `AnimationTransitions::play` will not fade out), resumes it
when the fall at `GRAVITY` has `land − freeze` left, and plays it out from
`land` once the ground comes. Any clip may declare a `freeze`, the moment
it is held at: the jump adds `leave` and `land` to its own, and the pickup
plays to its freeze and holds there while the loot window is open. A gait,
the walk or the run, declares its `stride` and `seconds` there too;
`Clips::gait` plays the one whose rate keeps the feet planted at the drawn
speed nearest its authored pace.

**Worn pieces.** A piece loads from `models/<piece>-<actor>.glb`, scene =
style, as a child of the actor. `client::systems::equipment` points its
skin's joints at the actor's by name, or hangs a socket piece from the rig's
`socket.<name>` node, and drops the GLB's copy of the rig.
`client::systems::hiding` reads the node extras: `hides` regions in the
build's z-up frame, judged at `_CENTRE`; `covers`, triangles of the actor's
mesh; `socket`. It swaps index buffers on cached copies shared by every
actor in the same combination. `Piece::name` is both the asset stem and the
key `hides.over` names, so the two must agree.

**Water.** A tile's water is one surface, a z-level, published by dissection
and rounded once, in `Composite::water_at`: surface and ground round to the
same steps, a surface's step covers the tiles below it and leaves the tiles
at it dry, so a dry tile has none. `Map::water_at` holds it apart from the
ground so either may arrive first; `movement::is_deep_water` is the one entry
rule. Water at zero is never built: the sea is the client's one plane, and a
channel or lake surface at zero is under it.

## Pinned system ordering

Ordering appears in about twenty places, most of it UI setup chaining off
`camera::setup` and sequencing internal to one plugin. These two are the ones
that break loudly:

- `movement::do_loc.after(movement::apply_displace)` — a `Loc` that ends a
  slide must see the `Displacing` marker the slide inserted, or it snaps.
- `cover::draw::init_pipelines.after(MeshPipelineSystems)` — `MeshPipeline`
  is itself built in `RenderStartup`, so a system that clones it there finds
  no resource without the pin. Every render pipeline built on the mesh
  pipeline carries this, in Bevy's own plugins as much as ours.

`VisualPosition` needs no pin: `advance_interpolation` runs in `PreUpdate`,
and every re-target starts from `current()`, so `actor::update` and
`camera::update` read the same value in any order. A reader that takes the
actor's `Transform` instead sees this frame's or last frame's depending on
the executor, and the player shakes in a close frame.

Remote-entity interpolation is not its own system: `movement::apply_intent`
seeds the simulation, `movement::simulate_remote` advances `Position` and
points `VisualPosition` at it, `actor::update` renders it.

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
6. **A second ground function.** `common_bevy::surface` is the terrain
   surface: corners at the mean of the three cells meeting there, fans
   between. The mesh builder places vertices on it and `movement::surface_y`
   samples it, so feet stay on what is drawn. Never blend, clamp, or smooth
   toward a neighbour by another rule. Whether a tile may be entered is a
   separate concern on tile z — `is_tile_blocked` and air-time in
   `calculate_movement`.
7. **Mixing schedules.** Physics — `input::apply` on the server, `input::tick`,
   `movement::predict_local_player` and `movement::simulate_remote` on the
   client — belongs to the fixed schedules; anything touching `Transform`
   belongs to Update.
8. **Pop-then-push on a queue front.** Use `front_mut()` so the queue is never
   momentarily empty (INV-002).
9. **The two-product lerp on a world position.** `a.lerp(b, s)` is
   `a·(1−s) + b·s`: tens of thousands of units from the origin the two
   products round apart and the sum wanders by a float step as `s` moves,
   even with `a == b`. Interpolate a position as `a + (b − a)·s`, as
   `VisualPosition::current` does; a follower rounding on its own — the
   camera — turns that wander into the actor shaking on screen.
10. **A world-space f32 position.** `map.convert(tile) + offset` far from
    the origin keeps only float steps, and anything smaller than a step
    added to it — a tick's walk, a re-base, a joint's motion — rounds away.
    Compute in the tile's frame: the offset is the position, a neighbour is
    `map.convert(here − tile)`, a height is `(z − tile.z) × rise`, and only
    a lookup names the tile, as `calculate_movement`, `Position::rebase` and
    `RenderOrigin` do. The test is exact: the same input at the origin and
    millions of tiles out gives the same output, bit for bit. Sites still
    world-space are static and drawn, each off by one step and fixable the
    same way: the mesh builder's vertices (`p − mesh_origin`), a region's
    placement (`mesh_origin − origin`), the camera's ray march, and the
    terrain shader's bombing, which re-rolls at a rebase.

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
