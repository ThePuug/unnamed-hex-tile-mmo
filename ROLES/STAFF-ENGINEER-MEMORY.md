# Staff Engineer Memory

Last updated: 2026-04-08

---

## Part 1: The Performance Mindset — Lessons from the Trenches

Every MMO that has ever launched has taught the same lesson in different ways: **performance is not a feature you add later. It is a structural property of your architecture.** When your beautiful systems grind to a halt under 200 concurrent players, the fix is never "optimize the hot loop." The fix is always "we should have designed the data flow differently from the start."

Principles carried from shipped titles and public postmortems:

### Principle 1: Your Worst Case IS Your Most Important Feature

Warhammer Online's signature feature was massive RvR battles. That was also the scenario most likely to break under load. The game shipped, the battles lagged, players left. WildStar promised 40-man raids with telegraphs — the hardest rendering scenario was the core gameplay loop.

**The rule:** Whatever your game is *about* — that is the scenario you must guarantee performance for. Not the quiet exploration. Not the solo experience. The thing that makes players stay. For a hex-tile MMO, that's the moment when multiple players are visible, the world is streaming around them, NPCs are chasing, combat is ticking, and the terrain extends to the horizon. Every system must be designed with that moment in mind.

### Principle 2: O(n^2) Visibility Will Find You

CCP's Veritas identified this as "one of the bounding scaling factors in large fleet fights — the unavoidable O(n^2) situation where n people do things that n people need to see." 100 players in mutual visibility = 9,900 state updates per tick. 500 = 249,500.

**The rule:** Every broadcast — every "send this to everyone who can see entity X" — is a multiplier on your tick budget. If you encode a message N times (once per observer), you pay N x encode cost. If you allocate per-observer, you pay N x alloc cost. The correct architecture: encode once, broadcast bytes. Track who sees what with a reverse index, not a forward scan.

### Principle 3: Never Block on a Single Client

New World's invulnerability exploit: the server waited on client input before processing outcomes. A player could drag their window, stalling the client, and the server couldn't progress that entity's state. This affected other players' experience.

**The rule:** No client's behavior — slow ACKs, dropped packets, malicious timing — should ever affect the server's processing of other clients. Timeouts. Independent processing. Never-wait-for-input. If a client is slow, the server moves on and reconciles later.

### Principle 4: Budget Every Millisecond

Riot's VALORANT team hit their 128-tick target (2.34ms budget) by tracking CPU time across 10 subsystem categories with telemetry. They set budgets per subsystem and followed up on discrepancies. They predicted hardware needs within 1% of actual.

**The rule:** At a 125ms server tick (8 Hz), you have a generous budget — but that budget covers ALL connected players' updates, world generation, AI pathfinding, combat resolution, AOI calculation, and network I/O. If you don't measure where the time goes, you can't know when you're approaching the wall. By the time you *feel* the lag, you've already lost.

### Principle 5: Death by a Thousand Cuts

No single allocation is slow. But:

- 200 `bincode::encode_to_vec()` calls per tick = 200 heap allocations
- 100 `Vec::collect()` in spatial queries = 100 more
- 50 `format!()` for UI text = 50 String allocations
- 30 `HashMap::new()` in mesh building = 30 more

After hours of play, memory fragments. The allocator searches longer free lists. Each frame takes 0.1ms longer. Then 0.5ms. Then 2ms. Nobody changed anything — but the game got slower.

**The rule:** Hot paths must be allocation-free. Pre-allocate. Reuse buffers. Use `ArrayVec` instead of `Vec` when the max size is known. Use `&'static str` instead of `String::from()`. Cache formatted text. The discipline isn't "never allocate" — it's "never allocate in code that runs every tick."

### Principle 6: Hot Path / Cold Path Separation

**Hot path:** Physics, movement, combat resolution, state replication, AOI calculation. This runs every tick. Every allocation, every lock acquisition, every HashMap probe here costs you at scale.

**Cold path:** Database persistence, analytics, chat, account management, terrain generation, mesh building. This runs asynchronously, on timers, or in response to events.

**The rule:** The hot path must never block on the cold path. Lockless queues between them. If terrain generation is slow, the game keeps running — the player just doesn't see distant chunks yet. If a mesh takes 50ms to build, that's fine — it's on a thread pool. But if the main thread locks a Mutex to read the Map during physics, and a terrain write is holding that lock, your tick budget just evaporated.

### Principle 7: Design for Density, Test at Density

VALORANT found that going from 1 instance to 168 instances per host degraded per-instance frame time by 3.8x. The cause: L3 cache contention between cores. You cannot discover this by testing in isolation.

**The rule:** Your performance benchmarks must simulate production conditions. 50 simulated players moving. 200 NPCs with AI. 500 chunks loaded. Terrain streaming. All at once. If you only test with 1 player on localhost, you will ship a game that works for 1 player on localhost.

### Principle 8: Have an Explicit Degradation Strategy

EVE Online's Time Dilation: when the server can't keep up, it slows the simulation. Players prefer slow-but-correct over fast-but-desynced. WoW dynamically instances zones. GW2's megaserver splits populations.

**The rule:** When you WILL exceed your budget (and you will), what happens? Silent frame drops? Desync? Rubber-banding? Or a controlled degradation? This must be designed, not discovered in production. Options: reduce update frequency for distant entities (network LoD), reduce visual fidelity (rendering LoD), cap entity density (instancing), slow simulation (time dilation). Pick your strategy now.

---

## Part 2: Codebase Audit — The Good, The Bad, and The Ugly

163 Rust files. ~47k lines. 8 crates. Honest assessment through the lens above.

---

### THE GOOD

Real craft here. These aren't accidents.

#### 1. Authoritative Server Architecture *(Principle 3)*

The Try/Do pattern is textbook correct. Client sends `Try`, server validates, broadcasts `Do`. The server never trusts client state. Input queues with sequence numbers allow client-side prediction while maintaining server authority. This is the architecture that scales. Many shipped MMOs got this wrong.

#### 2. Async Mesh Pipeline *(Principle 6)*

All mesh generation runs on `AsyncComputeTaskPool`. The in-place update pattern (old mesh stays visible until replacement completes) prevents visual pops. The 3-system architecture (evict -> reconcile -> poll) keeps the main thread budget clean. This is the right design.

#### 3. Clean Crate Boundaries

`world` has zero Bevy dependency. `common` is pure Rust. `qrz` is a standalone hex math library. This means terrain generation, coordinate math, and shared data types compile independently and can be tested without spinning up a game engine. World generation can run on dedicated threads without pulling in the renderer's type system.

#### 4. Network Channel Separation

Reliable-ordered for gameplay events. Reliable-unordered for chunks (avoids head-of-line blocking — one slow chunk doesn't delay damage events). Unreliable for movement intent (latest wins, loss is self-correcting). Budget-based flow control with health checks. ADR-034 documents the rationale. Thoughtful.

#### 5. Change Detection Where It Matters Most

NNTree updates filter on `Changed<Loc>` — stationary entities don't trigger spatial index rebuilds. Reaction queue updates filter on `Changed<ActorAttributes>`. These are the two highest-leverage uses of change detection in the codebase.

#### 6. Summary Batching

Server-side summary system caps at `MAX_SUMMARIES_PER_FRAME = 200`, preventing frame spikes during horizon streaming. Rate-limited cold path work. Correct.

#### 7. Hex-Native LoD Design

The design spec in `docs/design/lod.md` is genuinely clever. Decimating at the hex tile level instead of the vertex level preserves the terrain's visual vocabulary (flat/slope/cliff). The self-similar geometry (same code path at every radius) is elegant. T-junction resolution through linear interpolation is mathematically clean.

---

### THE BAD

Patterns that work today but will break under load. Structural problems, not bugs.

#### 1. Encode-Per-Observer Broadcasting *(Principle 2)*

`server/src/systems/renet.rs` `send_do` — Every `Event::Incremental` is serialized with `bincode::encode_to_vec()` once per observing player. 50 players watching 20 entities = 1,000 encode calls per tick. Each one allocates a `Vec<u8>`.

`server/src/systems/aoi.rs` — Spawn events: each nearby entity generates 6 spawn events, each encoded separately. 100 nearby entities = 600 encode calls during AOI entry.

**The fix:** Encode once, clone the bytes, broadcast to all observers. Turns O(observers x encode_cost) into O(1 x encode_cost + observers x memcpy). At scale, this is the difference between 16ms ticks and 160ms ticks.

#### 2. Global RwLock on Map *(Principle 6)*

`common-bevy/src/resources/map.rs` — Single `Arc<parking_lot::RwLock<qrz::Map<EntityType>>>`. Every movement tick reads this lock (3+ times per entity — `convert()`, `get_by_qr()`, `is_tile_blocked()`). Every chunk discovery writes to it (272 inserts per chunk).

When a chunk write holds the write lock, ALL physics calculations stall. With 100 entities moving and chunks streaming, this is a contention bottleneck.

**The fix:** Two options. (a) Snapshot pattern: physics reads a frozen `Arc<Map>` clone, writes go to a pending buffer, merge once per tick. (b) Spatial partitioning: 16x16 grid of RwLocks so a chunk write in one region doesn't block physics in another.

#### 3. Per-Frame Allocations in Hot Paths *(Principle 5)*

**Server hot path allocations (per tick):**

- `aoi.rs`: `Vec<Entity>` per moved entity (spatial query results) + `HashSet<Entity>` conversion
- `chase.rs`: `Vec<Entity>` per unlocked NPC (target acquisition) + 6 NNTree queries per NPC movement step
- `input.rs`: `bincode::encode_to_vec()` per queued input confirmation
- `actor.rs`: `ArrayVec` wire tile conversion (duplicated in two code paths)

**Client hot path allocations (per frame):**

- `resource_bars.rs`: 3x `format!()` for health/stamina/mana text — every frame, even when values unchanged
- `ui.rs`: 2x `format!()` for time display and distance indicator — every frame
- `renet.rs`: `get_message_type_name()` allocates a `String` for every received network message (diagnostics)
- `world.rs`: `.clone()` on entire mesh position/normal/index buffers during region rebuilds

**Estimated allocation rate:** 450-550 String allocations/sec on the client from UI alone. Plus hundreds of Vec allocations/sec on the server from spatial queries.

**The fix:** Cache text with dirty flags (only `format!()` when values change). Use `&'static str` for diagnostics. Return `ArrayVec<[T; 6]>` from `neighbors()` instead of `Vec`. Pre-allocate spatial query buffers and reuse them.

#### 4. No Component Delta Encoding *(Principle 2)*

Every component update sends the full component state. Player stamina drops by 1 -> entire Stamina struct (~30 bytes) sent. 100 stamina updates/sec x 30 bytes = 3 KB/s per player of redundant data.

Gaffer on Games demonstrated a 98% bandwidth reduction (17 Mbps -> 256 Kbps) through layered delta compression. Even "only send if changed by more than X" would cut bandwidth significantly.

#### 5. Unbounded Client Message Processing

`client/src/systems/renet.rs` `write_do` — `while let Some(serialized) = conn.receive_message(...)` with no per-frame limit. If the server sends 1,000 messages in a burst (initial chunk load + entity spawns), the client processes all 1,000 in one frame. Frame time spikes.

**The fix:** Per-frame message budget. Process up to N messages per frame, defer the rest to next frame. N = 200-500 depending on profiling.

#### 6. Missing Change Detection in Movement/Physics

Movement and physics systems run every tick for every entity, regardless of whether anything changed. An NPC standing still gets the same physics tick as one sprinting. With 200 NPCs, 180 of which are idle at any moment, that's 180 wasted physics evaluations per tick.

**The fix:** Filter on `Changed<KeyBits>` or an `IsMoving` marker component. Only process entities that have input or are in motion.

#### 7. Targeting Systems Run Every Frame

`client/src/systems/targeting.rs` — `update_targets()` and `update_ally_targets()` run unconditionally every frame, querying all entities. Targeting data is tactical — it doesn't need 60Hz updates. 10Hz is more than sufficient.

**The fix:** `run_if(on_timer(Duration::from_millis(100)))`. Saves 90% of those queries.

---

### THE UGLY

Systemic problems. The ones that require conversation before writing another feature.

#### 1. The Dual Architecture Crisis

`world/src/lib.rs` — Two complete terrain systems coexist:

- **Old:** `Terrain { seed, caches: Mutex<TerrainCaches> }` with `PlateCache` + `SpineCache`
- **New:** `Composite` event system with `DashMap` per-layer cell caches, `IndexRegistry`, deform/query cascade

Both are maintained. Both are tested (there's a test verifying they produce identical output). The server uses `Composite` via `EventRegistry` for elevation queries. The client uses old `Terrain` for admin flyover. `SpineCache` exists in both paths.

This is technical debt paying compound interest. Every terrain change must be validated against both systems. New developers (or AI agents) must understand two architectures to modify terrain. The old system holds a global `std::sync::Mutex` that serializes all plate and spine access — a contention bomb that the new system was designed to eliminate.

**The fix:** Kill the old `Terrain` struct. Migrate the admin flyover to `Composite`. Delete `TerrainCaches`. One system, one truth.

#### 2. spine.rs: 3,746 Lines of Monolith

This is 47% of the entire `world` crate in a single file. It contains: peak generation, ridgeline building, ravine networks, stream carving, Catmull-Rom spline math, candidate resolution, cache management, elevation queries, and tag resolution.

**The algorithmic problems inside:**

- **O(peaks^2 log peaks) ridgeline building** — For each peak, computes distance to ALL other peaks, sorts, checks connectivity. 100 peaks = ~1M distance calculations.
- **O(peaks) per tile query in `tag_at()`** — No spatial index. Every tag query scans all peaks. Called through `SpineCache::tag_at()` which iterates 7 chunks x instances per chunk x peaks per instance. A single tile tag query can trigger 700+ distance calculations.
- **Ravine path search is linear** despite being static geometry — ridge paths have no spatial index, only streams use `HexSpatialGrid`.
- **LRU eviction is O(cache_len)** — linear scan to find eviction candidate.

This file is where terrain generation time lives. And because it's a monolith, it's nearly impossible to optimize one piece without understanding all of it.

**The fix:** Split into modules (peaks, ridgelines, ravines, cache). Add spatial indexing for peak queries. Pre-build a spatial grid for ridge paths. Use a proper LRU (linked list + hash map) instead of linear scan.

#### 3. No Coherent System Budget

There is no framework for answering "how much of the tick budget does system X use?" The metrics plugin exists (`server/src/plugins/metrics.rs`) but uses a `Mutex<Vec<SnapshotField>>` — every system that records metrics acquires the same lock. The metrics system itself is a contention point.

On the client, there's a diagnostics overlay but no per-system timing breakdown. When frame time increases from 12ms to 18ms, there's no way to see which system is responsible without attaching an external profiler.

**This means you're flying blind.** You can't budget what you can't measure. When performance degrades, you'll spend hours bisecting instead of looking at a dashboard.

**The fix:** Bevy's built-in `FrameTimeDiagnosticsPlugin` and system-level tracing (via `tracy` or `puffin`) should be wired up for development builds. Replace the Mutex-based metrics with atomic counters or thread-local accumulation.

#### 4. The "Growing Mess of Systems" Problem

Server `main.rs` registers systems across Update, FixedUpdate, FixedPostUpdate, and PostUpdate schedules with a mix of timers, `.after()` chains, and implicit ordering. There's a `combat::do_nothing()` system that exists as a scheduling workaround. `try_gcd` is registered but does nothing (vestigial). `diagnostics.rs` is an empty file.

**The lack of a coherent thread:** Systems were added to solve immediate problems without a unifying framework for:

- What schedule does this belong in and why?
- What is this system's tick budget allocation?
- What data flows into and out of this system?
- How does this system degrade under load?

The result is a system graph that works today but that nobody can reason about holistically. Adding the next feature means finding a slot in the schedule, hoping it doesn't conflict with existing systems, and not knowing what the performance impact will be until you test it.

**The fix:** This needs a system map. A document (or diagram) that shows every system, its schedule, its data dependencies, its expected budget, and its degradation strategy. Not as process overhead — as the navigational chart that prevents "I added a system and everything got 5ms slower and I don't know why."

#### 5. Triple-Redundant Map Storage

`qrz/src/map.rs` stores every tile in three data structures simultaneously:

- `BTreeMap<Qrz, T>` — for ordered iteration
- `HashMap<Qrz, T>` — for O(1) lookup
- `HashMap<(i32, i32), i32>` — for column lookup

3x memory for the same data. With 10,000+ tiles loaded, this is meaningful. And every insert/remove must update all three, tripling mutation cost.

**The fix:** Pick one primary structure. If you need ordered iteration (do you?), use `BTreeMap` alone — its lookup is O(log n), which is fine for the access patterns here. Or use `HashMap` alone and sort when needed (rare).

#### 6. Client Ships with World Generation Code

`client/Cargo.toml` defaults to `features = ["admin"]`, which pulls in the entire `world` crate (3,746-line `spine.rs` included). Every release build includes terrain generation code that only exists for developer flyover mode. This bloats the binary and compilation time.

**The fix:** `default = []`. Developers use `cargo run -p client --features admin`.

---

## Priority Matrix

### Tier 1: Fix Before Next Playtest *(hours, not days)*

1. **Encode-once broadcast** — Server `send_do` and AOI spawn encoding. Highest ROI.
2. **Client text caching** — `resource_bars.rs` and `ui.rs` format strings. 5 minutes per file.
3. **Client message budget** — Cap `write_do` at 500 messages/frame. One `if` statement.
4. **Diagnostics string allocation** — `get_message_type_name()` -> `&'static str`.

### Tier 2: Fix This Sprint *(structural, a few days)*

5. **System timing instrumentation** — Wire up per-system budget tracking so you can measure everything else.
6. **Kill the dual terrain architecture** — Migrate admin flyover to Composite. Delete `Terrain` struct.
7. **Map lock contention** — Snapshot pattern for physics reads.
8. **Movement change detection** — Skip idle entities in physics.

### Tier 3: Fix Before Scaling *(before 50+ players)*

9. **Spine.rs split and spatial indexing** — Unlock terrain generation performance.
10. **Map triple storage** — Consolidate to single backing store.
11. **NPC spatial query optimization** — Pre-computed occupancy grid instead of per-NPC NNTree queries.
12. **Network delta encoding** — At least for high-frequency components (stamina, mana).

### Tier 4: Architecture for the Future

13. **System map document** — Every system, its schedule, its budget, its degradation strategy.
14. **Load test harness** — Simulated players at production density.
15. **Degradation strategy** — What happens when tick budget is exceeded? Design it, don't discover it.

---

## The Coherent Thread

Every system must answer four questions:

1. **Schedule:** Which schedule do I run in, and why? (FixedUpdate = deterministic game state. Update = rendering/input. PostUpdate = derived state.)
2. **Budget:** How many milliseconds am I allowed? What happens if I exceed it?
3. **Data flow:** What do I read? What do I write? Who reads what I write?
4. **Density:** What is my cost when there are 200 entities in range instead of 20?

Systems that can't answer question 4 are the ones that will break at scale. Right now, most of them can't.

---

## Systems Reviewed

| System | Verdict | Key Concern |
|--------|---------|-------------|
| Try/Do authority model | GOOD | Correct architecture |
| Async mesh pipeline | GOOD | Proper hot/cold separation |
| Network channel separation | GOOD | Thoughtful design, documented in ADR-034 |
| NNTree change detection | GOOD | Correct use of `Changed<Loc>` |
| Summary batching | GOOD | Rate-limited cold path |
| Hex-native LoD spec | GOOD | Elegant math |
| Server `send_do` broadcast | BAD | O(observers x encode_cost) per tick |
| Map RwLock | BAD | Global contention on hot path |
| Client UI text rendering | BAD | ~450 String allocs/sec |
| Client message receive | BAD | Unbounded per-frame processing |
| Movement/physics | BAD | No idle entity filtering |
| Dual terrain architecture | UGLY | Compound technical debt |
| `spine.rs` monolith | UGLY | O(n^2) algorithms, 3,746 lines |
| System budget tracking | UGLY | Flying blind, metrics contend |
| `qrz::Map` triple storage | UGLY | 3x memory, 3x mutation cost |

## Deferred Risks (Time Bombs)

- **AOI at 50+ players:** O(n^2) spawn event encoding will dominate tick budget
- **Map contention at scale:** Single RwLock blocks all physics during chunk writes
- **spine.rs at world scale:** O(peaks^2) ridgeline building blocks terrain generation
- **No degradation strategy:** Unknown behavior when tick budget exceeded
- **No load testing harness:** All performance claims are theoretical until stress-tested
