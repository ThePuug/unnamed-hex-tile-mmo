# World Event System

## Design Philosophy

The world is built from a flat, untagged hex substrate. Every feature — plates, mountains, forests, creature camps, rivers — is an **event** that reads the world below it, surveys for qualifying locations, and deforms the terrain. Events evaluate in dependency order. Each event builds structural indexes during deform and materializes tiles lazily via query — two independent cascades that prevent large features from triggering proportional tile computation in layers below.

### Core Thesis

**"A world that fights back"** requires a world that can be authored at scale. Dozens or hundreds of event types will layer onto the substrate — geological, ecological, creature-driven, player-driven, encroachment-driven. The framework must make authoring safe and performant without requiring authors to understand the full stack. Authors declare what they need. The framework controls how it's evaluated.

---

## Framework Contract

Every event implements five methods:

```rust
trait WorldEvent: Send + Sync {
    fn name(&self) -> &str;
    fn scale(&self) -> u32;
    fn survey(&self) -> Survey;
    fn deform(
        &self,
        cell_id: CellId,
        matched: &[(i32, i32)],
        indexes: &mut IndexRegistry,
        seed: u64,
    );
    fn query(
        &self,
        q: i32, r: i32,
        cell_id: CellId,
        indexes: &IndexRegistry,
        below: &dyn Fn(i32, i32) -> TileView,
        seed: u64,
    ) -> Option<TileOutput>;
}
```

Three functional methods:

**survey()**: Declarative predicates describing qualifying locations. The framework evaluates against index metadata or tile data. Returns tile coordinates that deform will receive as `matched`.

**deform(cell_id, matched, indexes, seed)**: Structural work. Reads indexes from lower layers, generates features (peaks, ridgelines, camps), registers results as indexes for higher layers. **Never materializes tiles.** Never receives CellView or CellOutput.

**query(q, r, cell_id, indexes, below, seed)**: Materializes one tile. Called lazily when the composite needs this event's contribution at (q,r). Reads indexes (from own and lower layers) and calls `below(q, r)` for the composite tile from all layers below. Returns `Option<TileOutput>` — None means this event contributes nothing at this position. Framework caches results in `CellOutput`.

**`&self` on deform and query**: Events may hold interior-mutable internal caches for computational optimization. These are the event's private concern — the framework doesn't see or manage them. Internal caches may change speed, never results.

### What the Framework Provides

- **Two cascades**: Independent deform (index → index) and query (tile → tile) evaluation paths
- **Lazy tile cache**: CellOutput populated per-tile by query results, not eagerly by deform
- **Cell grid management**: Independent spatial grid per event at the declared scale, LRU caching, deterministic regeneration on eviction
- **Survey evaluation**: Framework-controlled predicate evaluation against index metadata or tile data
- **Index registry**: Shared accumulating index storage that events populate during deform and reference in surveys and queries
- **Cascade metrics**: Per-event indexed cell gauge + cache hit% ring buffers. Composite-level cached tile gauge + tile hit%.

### What the Author Provides

- **Scale**: The natural spatial grain for this event's features
- **Survey**: Declarative predicates describing qualifying tiles
- **Deform**: Structural code that reads lower indexes and registers new indexes
- **Query**: Per-tile materialization code that reads indexes and the layer below
- Internal caches (optional): Event-private shared state that speeds up deform/query across cells

---

## Two Cascades

The deform/query split creates two independent evaluation paths. This is the key architectural mechanism that prevents large-scale events from triggering proportional tile computation in layers below.

### Deform Cascade (index → index)

When a cell needs evaluation, the framework ensures all cells in layers below have been deformed (indexes populated). This cascade is structural — index granularity, no tile materialization.

A spine cell triggers plate deform for overlapping plate cells. Plate deform discovers centroids and registers them in `PlateCentroidIndex`. Spine deform reads those centroids, generates spine instances, and registers them in `SpineInstanceIndex`. Zero tiles materialized.

### Query Cascade (tile → tile)

When `composite.tile_at(q, r)` is called, each layer resolves that single tile. If not cached, the framework calls `event.query()`, which may call `below(q, r)` to get the composite tile from all layers below. Purely vertical — one (q,r), N layers deep. No horizontal expansion.

### Independence

**Deform never materializes tiles.** The deform method has no access to CellOutput or CellView. It reads and writes indexes only.

**Query never triggers deform.** The framework ensures the deform cascade completes before any query for that cell. Query's `below` function returns cached query results from lower layers (triggering their query if cold, never their deform).

Survey predicates that need tile data (non-index filter closures) trigger the query cascade for tiles below. This is bounded by cell scale — acceptable for events with small cells (spawners, flora), not needed for index-scoped surveys (spines reading plate centroids).

---

## Cell Grid

Each event defines its own cell scale. The framework tiles the world in cells at that scale. Cells are the evaluation unit: survey + deform run per-cell; query materializes tiles within the cell lazily.

Cell grids are **independent per event**. Different events have different cell sizes. There is no shared grid hierarchy and no alignment constraint.

**Scale should reflect the event's natural feature size.** With lazy materialization, large cells don't cause cascade blowup. A spine event can use a cell scale matching its natural influence radius — deform discovers features in that area, query materializes individual tiles on demand. The deform/query split eliminates the tradeoff between cell size and cascade cost.

### Margin

Spatial predicates in the survey require neighborhood context beyond the cell boundary. The framework computes the required margin from the survey's spatial predicate radii and ensures indexes are populated to that depth before evaluation.

---

## Materialization

Events do not cache generators. They cache **per-tile results**, populated lazily by query.

### Data Types

```rust
struct CellOutput {
    tiles: HashMap<(i32, i32), TileOutput>,
}

struct TileOutput {
    tags_added: TagSet,
    tags_removed: TagSet,
    elevation_delta: f64,
    entities: SmallVec<[EntityPlacement; 1]>,
}

struct TileView {
    pub q: i32, pub r: i32,
    pub wx: f64, pub wy: f64,
    pub tags: TagSet,
    pub elevation: f64,
}
```

**CellOutput** is a lazy cache. Created empty when deform runs. Populated tile-by-tile as query is called. Previously-queried tiles are cache hits. Tiles with no contribution from this event are not stored — sparse.

**TileOutput** is what a single event's query returns for a single tile position.

**TileView** is the composite state at a tile — tags and elevation from all layers. Returned by `below(q, r)` in query and by `tile_view_at` on indexes.

### Terrain Output is Immediate

Events register the existence of terrain features immediately during deform (in indexes). Query materializes per-tile data lazily. From the caller's perspective, terrain is always available — calling `composite.tile_at(q, r)` triggers any needed evaluation and returns the correct answer.

NPC spawning is a **separate runtime system** that reads the composite and manages ECS entities. The NPCs are not event output. The terrain the camp sits on is.

---

## Index System

Events register spatial indexes during deform. Higher events read these indexes in their surveys, deforms, and queries. Indexes are the cross-event communication mechanism — a spine event finding plate centroids, a hilltop guardian finding peaks, a fishing spot finding water collection points.

### IndexRegistry

Shared across all events. One registry in the Composite. Keyed by Rust type (`TypeId`). Accumulates across cell evaluations.

```rust
struct IndexRegistry { /* HashMap<TypeId, Box<dyn AnyIndex>> */ }

impl IndexRegistry {
    fn get_or_create<T: EventIndex + Default + 'static>(&mut self) -> &mut T;
    fn get<T: EventIndex + 'static>(&self) -> Option<&T>;
    fn remove_cell(&mut self, cell_id: CellId);
}
```

Type-keyed storage prevents naming collisions. Two events can't accidentally collide because they'd need to reference the same concrete type.

### EventIndex Trait

Entries partitioned by source cell ID so the framework can scope queries spatially.

```rust
trait EventIndex: Send + Sync + Default + 'static {
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)>;
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)>;
    fn tile_view_at(&self, q: i32, r: i32) -> Option<TileView>;
    fn remove_cell(&mut self, cell_id: CellId);
}
```

`tiles(cell_ids)` returns entries from only the specified cells. The framework computes which source-event cell IDs overlap the querying event's bounds and passes those IDs in. Every index query is spatially scoped — no global scans.

`neighbors(q, r)` returns graph-connected neighbors for spatial predicates. For a centroid index, these are plate-graph-adjacent centroids. For a river index, upstream/downstream segments.

`tile_view_at(q, r)` returns metadata about the entry at (q,r) — tags, elevation — sufficient for survey predicate evaluation **without triggering tile materialization**. This is what prevents index-scoped surveys from cascading into query. PlateCentroidIndex stores TagSet per centroid; spine survey checks centroid tags via `tile_view_at`, not via `composite.tile_at`.

`remove_cell(cell_id)` cleans entries when the source event's cell is evicted from LRU. The framework calls this on all indexes during eviction. On regeneration, deform repopulates deterministically.

### Index Registration

Indexes are populated during deform as a natural side effect:

```rust
fn deform(&self, cell_id: CellId, matched: &[(i32, i32)], indexes: &mut IndexRegistry, seed: u64) {
    // Read lower layer indexes
    let plate_index = indexes.get::<PlateCentroidIndex>().unwrap();
    let inland_centroids = plate_index.tiles(&overlapping_cell_ids);

    // Generate features
    let spines = self.generate_spines(&inland_centroids, seed);

    // Register for higher layers and own query
    let spine_index = indexes.get_or_create::<SpineInstanceIndex>();
    spine_index.insert(cell_id, spines);
}
```

### Internal vs External Caches

**External indexes** (in IndexRegistry) serve the framework contract. Centroids for spine surveys. Spine instances for spine queries. Peaks for hilltop guardian placement. The framework manages their lifecycle, scopes their queries, and cleans them on eviction.

**Internal caches** (held by the event via interior mutability) are purely computational optimization — warm Voronoi lookups, memoized noise fields. The IndexRegistry handles cross-cell data sharing through the deform cascade; internal caches are not needed for working around framework scope limitations. They must not affect determinism: output depends only on `(cell_id, matched/tile, indexes, seed)`. Internal caches change speed, never results.

---

## Predicate System

The survey is declarative. Authors build predicates from framework-provided combinators. The framework controls evaluation.

### Two Concerns, Two Mechanisms

**Spatial predicates** are the framework's domain. They touch tiles outside the current one. The framework controls traversal strategy, manages index-backed lookups, and guarantees bounded cost.

**Filter** is the author's domain. It looks at one tile. Tags, elevation, noise, arbitrary math — all O(1) per tile.

### Evaluation Against Index Metadata

When a survey is scoped through an index (`from_index`, `all_neighbors_in`, `near`), the framework builds TileView from the index's `tile_view_at` method. **This prevents survey from triggering the query cascade for index-scoped predicates.** The survey reads structural metadata that was registered during deform, not materialized tile data.

For non-index filter closures (e.g., spawner tag/elevation checks), TileView comes from the query cascade — the framework calls `composite.tile_at` for each candidate tile. This is bounded by cell scale and acceptable for events with small cells.

### Evaluation Order

1. **Tile source** — all tiles in cell, or tiles from a typed index
2. **Spatial predicates** — graph traversal or proximity checks (using index metadata)
3. **Filter closure** — per-tile evaluation on survivors

### Tile Source

```rust
Survey::all()                        // every tile in cell
Survey::from_index::<T>()            // tiles from typed index T
Survey::none()                       // no tile enumeration (empty matched)
```

`Survey::none()` produces an empty `matched` slice — the event's deform discovers features internally using the seed and cell bounds. Useful when the event's spatial structure doesn't align with tile enumeration (e.g., plates discovering centroids via lattice iteration).

When using `from_index`, the framework computes which source-event cell IDs overlap the current cell's bounds (derived from the two events' scales), then calls `index.tiles(overlapping_ids)` to get the scoped tile set.

### Graph Predicates

Connected traversal through a neighbor graph.

```rust
// Hex adjacency (default)
.all_neighbors(|tile: &TileView| -> bool, radius)
.any_neighbor(|tile: &TileView| -> bool, radius)

// Index neighbor graph
.all_neighbors_in::<T>(|tile: &TileView| -> bool, radius)
.any_neighbor_in::<T>(|tile: &TileView| -> bool, radius)
```

At radius 1 with hex adjacency: checks 6 hex neighbors. At radius N: BFS flood to depth N. With an index: walks the index's neighbor graph instead. TileView from index-scoped variants comes from `tile_view_at`.

### Proximity Predicates

Spatial distance check against an index. No graph — pure geometric distance.

```rust
.near::<T>(|tile: &TileView| -> bool, radius)
.not_near::<T>(|tile: &TileView| -> bool, radius)
```

"Is there an entry in index T within `radius` hexes of the current tile that satisfies the closure?"

The framework optimizes this using cell IDs: compute which source-event cells could contain entries within `radius`, query only those cells from the index.

### Spacing Predicate

Post-filter density control. Ensures selected tiles are at least N hexes apart.

```rust
.min_spacing(distance)
```

Applied after filter. Deterministic priority-ordered greedy exclusion — each candidate gets a priority from `hash_u64(q, r, seed)`, sorted highest first. Walk the sorted list; skip any candidate within `distance` hexes of an already-accepted tile. Same seed + same candidates → same selection.

### Filter Closure

```rust
.filter(|tile: &TileView, seed: u64| -> bool)
```

Runs last, on the smallest candidate set. The author writes whatever O(1) per-tile logic they need. The framework cannot optimize this and doesn't try.

### Survey Examples

```rust
// Plates: centroids discovered internally, no tile enumeration
Survey::none()

// Spines: plate-level survey via centroid index + spacing (no tile materialization)
Survey::from_index::<PlateCentroidIndex>()
    .all_neighbors_in::<PlateCentroidIndex>(
        |tile| tile.tags.has(PlateTag::Inland), 1)
    .filter(|tile, _seed| tile.tags.has(PlateTag::Inland))
    .min_spacing(10_000)

// Spawners: per-tile filter + spacing (triggers query cascade, small cell)
Survey::new()
    .filter(|tile, seed| {
        tile.tags.has_any(&[Highland, Foothills])
            && tile.elevation > 0
            && simplex_2d(tile.wx / 800.0, tile.wy / 800.0,
                          seed + SPAWNER_SEED) > 0.1
    })
    .min_spacing(50)

// Flora (notional): proximity + filter
Survey::new()
    .any_neighbor(|tile| tile.tags.has(Sea), 2)
    .filter(|tile, _seed| {
        tile.tags.has_any(&[Inland, Coast])
            && tile.elevation >= 1 && tile.elevation <= 4
    })

// Hilltop guardian (notional): cross-index spatial join
Survey::from_index::<SpawnerCampIndex>()
    .near::<SpinePeakIndex>(|tile| tile.elevation > 200, 10)
    .filter(|tile, _seed| tile.tags.has(Camp))
```

---

## Composite

Owns ordered event layers, cell caches, IndexRegistry. Provides the query API.

```rust
struct Composite { /* layers, caches, IndexRegistry, seed */ }

impl Composite {
    fn add_event(&mut self, event: Box<dyn WorldEvent>);
    fn tile_at(&self, q: i32, r: i32) -> TileView;
    fn elevation_at(&self, q: i32, r: i32) -> i32;
    fn tags_at(&self, q: i32, r: i32) -> TagSet;
}
```

### Deform Flow

```
cell deform requested → check if already deformed → no →
    ensure all overlapping lower-layer cells deformed (recursive) →
    event.survey() → evaluate against index metadata / tile data → matched →
    event.deform(cell_id, matched, &mut indexes, seed) →
    create empty CellOutput → mark cell as deformed
```

### Query Flow

```
tile_at(q, r) requested → for each layer bottom-up:
    find cell for (q, r) at this layer's scale →
    ensure cell deformed →
    check CellOutput cache for (q, r) → miss →
        event.query(q, r, cell_id, &indexes, below, seed) →
        cache result in CellOutput →
    apply TileOutput to running TileView
```

---

## Event Examples

### PlateEvent (Event #0)

**Scale**: Matches centroid spacing (~1800 tiles). Large cells avoid redundant centroid discovery across overlapping regions.

**Survey**: `Survey::none()` — plates discover centroids internally via lattice iteration over cell bounds. No tile enumeration needed.

**Deform**: Discover plate centroids within cell bounds. Evaluate regime at centroid locations. Build Voronoi neighbor graph. Classify centroids (Sea/Coast/Inland). Register `PlateCentroidIndex` with centroids, tags, and neighbor graph.

**Query(q, r)**: Evaluate regime field at (q,r). Find nearest plate via Voronoi distance. Return `TileOutput` with the plate's tag (Sea/Coast/Inland).

### SpineEvent (Event #1)

**Scale**: Matches spine influence radius. Large cells contain full spine features — no cross-cell lookups needed in query.

**Survey**: `Survey::from_index::<PlateCentroidIndex>().all_neighbors_in(Inland).filter(Inland).min_spacing(10_000)` — reads centroid tags via `tile_view_at`. No tile materialization. Finds Inland centroids with all-Inland plate-graph neighbors, then greedy exclusion enforces minimum 10,000-hex separation between epicenters.

**Deform**: Read qualifying centroids (already spaced by survey). Generate peaks, arms, ridgelines, ravine networks directly from matched centroids. Register `SpineInstanceIndex` with generated instances and their parameters.

**Query(q, r)**: Read `SpineInstanceIndex` for nearby spine instances. Evaluate elevation contribution (peaks + ridgelines - ravines). Call `below(q, r)` for base tags. Return `TileOutput` with elevation delta + spine tags (Ridge/Highland/Foothills) where elevation > 0.

### SpawnerEvent (Event #2)

**Scale**: Game-chunk size. Small cells — query cascade during survey is bounded.

**Survey**: `Survey::new().filter(tags + elevation + noise).min_spacing(50)` — triggers query cascade for tiles in the cell to evaluate filter predicates, then greedy exclusion enforces minimum 50-hex separation. Replaces runtime distance checks in the engagement spawner.

**Deform**: Read matched tiles. Determine archetypes from composite tags (Highland→Berserker, Foothills→Juggernaut, Ridge→Defender, flat Inland→Kiter). Register `SpawnerPlacementIndex` with camp positions and archetypes.

**Query(q, r)**: Check `SpawnerPlacementIndex` for camp presence at this tile. Return camp tags if present.

---

## Cascade Metrics

Metrics reflect the two-cascade architecture. Gauges show current working set size (not lifetime accumulation — LRU eviction would double-count). Hit percentages use **ring buffers** (256 samples per cell, 4096 per tile) — the displayed value holds steady when activity stops rather than decaying to zero.

### Composite Level (query cascade)

- **cached** — gauge. Unique (q,r) positions currently cached in CellOutput across all layers.
- **tile hit%** — ring buffer. Recent `tile_hits / (tile_hits + tile_misses)`.

### Per Event (deform cascade)

- **indexed** — gauge. Warm cells in LRU cache (`cache.cells.len()`).
- **cell hit%** — ring buffer. Recent `cell_hits / (cell_hits + cell_misses)`.

Gauges breathe with player movement — exploration grows them, LRU eviction shrinks them. Ring buffers give stable readout during steady-state operation.

---

## Invariants

**INV-E01 — Lazy materialization**: Events cache per-tile results lazily via query, never eagerly during deform. Every query against the composite resolves to cached data or a single vertical cascade.

**INV-E02 — Deterministic generation**: Same cell ID + same seed + same indexes below → identical deform output. Same tile + same indexes + same below → identical query output. Internal caches may change speed, never results.

**INV-E03 — Dependency order**: Events evaluate in declared order. Event N reads the composite of events 0..N-1. No cycles. No upward reads.

**INV-E04 — Survey is declarative**: Authors cannot write arbitrary iteration. The framework owns the evaluation loop.

**INV-E05 — Spatial predicates are framework-controlled**: `all_neighbors`, `any_neighbor`, `near`, `not_near`. The framework controls which tiles are visited.

**INV-E06 — Index dependency is explicit**: `Survey::from_index::<T>()` and index-scoped predicates are explicit typed dependencies. Missing index = no results. No silent fallback.

**INV-E07 — Terrain output is immediate**: Deform registers feature existence in indexes immediately. Query materializes per-tile data lazily. From the caller's perspective, terrain is always available.

**INV-E08 — Independent cell grids**: Each event has its own cell scale. No shared hierarchy.

**INV-E09 — Indexes are cell-partitioned**: Every index entry has a source cell ID. The framework scopes queries by spatial overlap of cell IDs. No global scans.

**INV-E10 — Cascade independence**: Deform never materializes tiles. Query never triggers deform. The two cascades are independent. Survey may trigger the query cascade for non-index filter predicates, bounded by cell scale.

---

## Deferred

**Baking**: Collapsing bottom N layers into the substrate. Indexes from baked events must promote to substrate-level infrastructure or be rebuildable. Not needed for the three-event stack.

**Invalidation**: Dirty upper layers when a dynamic event mutates below. Not needed until runtime dynamic events exist.

**Parallel evaluation**: Independent events evaluating concurrently. Not needed while cold-path generation is infrequent.

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | EntityPlacement | In TileOutput | Omitted from TileOutput | Pass 1 spawners record in SpawnerPlacementIndex only; terrain deform (camp assets) is Pass 2 |
| 2 | Spawner archetype | Determined during deform from survey | Deferred to query (below closure resolves tags) | Deform has no CellView or below access; archetype stored as placeholder, resolved at query time |
| 3 | Spawner positions | Same positions as old SpawnerCache | Different positions (hex ball grid vs world-space offset chunks) | Grid topology changed; same noise/tag/archetype logic, different evaluation points |
| 4 | below closure caching | Below resolves and caches intermediate tiles | resolve_below is read-only (may recompute uncached sub-tiles) | Avoids mutable borrow conflicts in the closure; main tile_at loop handles caching at each layer |

## Implementation Gaps

**Cleanup**: SpawnerCache, SpineCache, and old Terrain methods are dead code on the server path. Retained for client admin flyover and terrain-viewer compatibility.

**Deferred**: Client admin flyover — still uses Arc<terrain::Terrain> directly.

**Deferred**: terrain-viewer — uses generate_region() separate code path.

**Post-migration**: Campsite terrain output (flora clearing, entity placements) — SpawnerEvent Pass 2

---

**Related Design Documents:**
- [Terrain Generation](terrain-generation.md) — Algorithm details for plates, spines, elevation (event-specific internals)
- [Combat Balance](combat-balance.md) — NPC archetypes and positioning
- [Siege System](siege.md) — Encroachment as a future dynamic event
- [Hub System](hubs.md) — Player settlements interacting with terrain events
