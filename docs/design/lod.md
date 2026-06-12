# Continuous Level-of-Detail

## Player Experience Goal

Terrain extends to the horizon in every direction. A player standing on a z=4000 peak looks down across an entire continent of foothills, valleys, and distant plains. No atmospheric fade hiding missing geometry. No terrain ending abruptly at a radius boundary. The world reads as a continuous landscape at every distance, coarsening naturally with perspective — the way real terrain looks from a mountaintop.

## Design Thesis

**Summary hexagons with continuous distance-driven radius.** Groups of tiles are consolidated into single flat hexagons at a representative z-level. The summary radius is a continuous function of camera distance — nearby terrain renders at full tile detail, distant terrain renders as progressively larger hexagons. Height differences between adjacent summaries produce vertical cliff faces (skirts), preserving the terrain's silhouette at every scale.

---

## Summary Hex Geometry

### Summary Lattice

Each summary radius `r` defines a lattice that tiles the world in non-overlapping hexballs. A hexball at radius r covers `3r²+3r+1` tiles. The lattice scale is `2r+1` — summary cell `(sq, sr)` centers at tile `(sq×scale, sr×scale)`.

At r=0, each summary is a single tile (full detail). At r=1, each summary covers 7 tiles. At r=3, each summary covers 37 tiles. The lattice provides gap-free, overlap-free tiling at every radius.

### Summary Surface

Each summary renders as a single flat-top hexagon:

- **Center vertex**: At the hexball center position, height = `center_z × RISE`
- **6 corner vertices**: At outer radius `(2r+1) × HEX_OUTER_RADIUS` from center, same height as center
- **6 triangles**: CCW fan from center to adjacent corner pairs
- **All vertices at the same Y**: The hex is truly flat — no slope blending within the summary

This produces 6 triangles per summary regardless of radius. Terrain detail within the hexball is lost; only the representative elevation is preserved.

### Center Z Selection

The center z is chosen to preserve the terrain's most visually prominent feature within the hexball. The selection rule is uniform across producers:

1. Compute the mean z of the sampled tiles
2. Select the sample with the greatest absolute deviation from the mean
3. Tiebreak: prefer higher z (peaks over valleys)

This approximates "most extreme point" — summaries tend to show peaks and ridges rather than averaging them away. The center z is always a discrete integer.

**Sampling strategy varies by producer**, because cost does:

- **Local Map** (producer 1): reads every tile in the hexball — up to 271 at r=9. The tile data is already in the Map (loaded for gameplay), so full-hexball sampling is effectively free. Uses `select_center_z(&all_tile_zs)`.
- **Server `EventRegistry`** (producer 2) and **flyover `AdminComposite`** (producer 3): sample **7 points** — the hexball center plus 6 axis-aligned offsets at distance `d = (2r+1)/3`. Full-hexball sampling is prohibitive here — each `elevation_at` is a procedural composite query. At r=9 this is 7 samples instead of 271, with no visible difference at LoD distance. Uses `sample_center_z(r, sq, sr, elevation_at)`.

The two strategies produce different center_z values for the same hexball. This is a known, accepted divergence — producer 1 only runs inside the loaded extent (r≥1 gated), producers 2 and 3 only run beyond it. Boundary cases (gated/ungated transition, flyover→gameplay) may exhibit a minor LoD pop; deemed acceptable given the compute cost differential.

### Canonical Vertex IDs

Each summary corner vertex has a deterministic doubled-integer ID: `(3×sq + VX2[i], sq + 2×sr + VZ2[i])`. Adjacent summaries sharing an edge produce matching IDs at shared corners. This enables stitching without explicit coordination — matching is by ID lookup, not spatial proximity search.

### Mesh Regions

Summaries are grouped into **mesh regions** for efficient rendering. Each mesh region is a radius-9 hexball in summary-lattice space, containing up to 271 summaries. One Bevy mesh entity per region.

Mesh region origin is the center summary's world position, used as the coordinate origin for all vertices in the region (f32 precision preservation at large world coordinates).

### Skirt Stitching

Height differences between adjacent summaries produce vertical cliff faces:

**Intra-region skirts**: All edges within a mesh region are collected by canonical vertex ID. Edges with two sides (shared between two summaries) at different heights get a skirt quad — two triangles bridging the height gap. Skirt normals point outward horizontally, perpendicular to the edge in the xz plane.

**Cross-region skirts**: Edges with one side (at the mesh region perimeter) are stored as `PerimeterEdge`. Adjacent mesh regions exchange perimeter edges and emit skirt quads where heights differ. This runs as a second pass after the base mesh is built.

**No gaps**: Adjacent summaries at the same height share vertices at identical positions (guaranteed by canonical vertex IDs). Adjacent summaries at different heights are connected by skirt quads. The surface is always closed.

---

## Distance Band Model

### Summary Radius from Distance

The summary radius at camera distance D (in world units) is:

```
r = ceil((MIN_SCREEN_PX × D / (tile_diameter × pixel_scale) − 1) / 2)
```

Where:
- MIN_SCREEN_PX = 12 pixels (minimum visual size for a summary hex)
- tile_diameter = 2 × HEX_OUTER_RADIUS = 2.0 WU
- pixel_scale = screen_height / (2 × tan(FOV/2))
- FOV = caller-provided vertical FOV (gameplay: 60°, flyover: 90°)

This ensures every summary subtends at least MIN_SCREEN_PX on screen. As distance increases, r increases — summaries cover more tiles but remain visually legible. At wider FOV, pixel_scale drops and bands shift inward — each summary covers fewer tiles to maintain the same screen-space size.

### Distance Bands

Band thresholds are player-centric horizontal distances: `band_outer_threshold(r) - CAMERA_DISTANCE`. At horizontal distance D from the player, the worst-case camera-to-ground distance is `D + CAMERA_DISTANCE` (point directly away from camera). Conservative — hexes on the camera side subtend more pixels.

`compute_active_bands(max_distance, fov)` returns a sorted list of `(r, inner_wu, outer_wu)` bands from the player to the visual horizon. Zero-width bands are skipped (e.g., r=0 at 90° flyover FOV where CAMERA_DISTANCE consumes the entire r=0 threshold).

### r=0 Full Detail

At r=0, each summary is a single tile. The mesh is built using the existing `compute_tile_geometry()` pipeline — full slope blending, cliff skirts, neighbor-aware vertex heights. r=0 regions require all 6 neighbor chunks for correct edge geometry.

---

## Two Rendering Regimes

### Gated (within loaded extent)

The server sends full 271-tile ChunkData within FIXED_STREAM_RADIUS (21 chunks / 599 WU). For mesh region computation, the client uses the actual loaded chunk extent (`local_max`) as the gated/ungated boundary — not the streaming constant. In normal gameplay these are equivalent. In flyover (where loaded chunks are fewer), `local_max` is smaller, so ungated rendering starts earlier.

Within the loaded extent:
- **r=0**: Full tile geometry from Map data via `compute_tile_geometry()`
- **r≥1**: `select_center_z()` from Map tile elevations, written to SummaryCache
- Mesh regions are **gated** on loaded chunks — a region is only dispatched if its chunks are present

The client recomputes visible regions when the camera moves, dispatching async mesh tasks for newly-visible regions.

### Ungated (beyond loaded extent)

Beyond the loaded extent, mesh regions are **ungated** — dispatched whenever SummaryCache has data, regardless of chunk presence. The server computes center_z values procedurally from EventRegistry and sends them as `SummaryBatch` messages on `ReliableUnordered`. In flyover, the `FlyoverSummaryTracker` provides the same data locally. The client builds mesh from cache values — it doesn't have the underlying tile data.

### Unified SummaryCache

`SummaryCache` is a `DashMap<MeshRegionKey, Arc<RegionData>>` — per-region locking, no global lock. Each `RegionData` holds all ~271 center_z values for one mesh region. Three producers write via `insert_region()`:

1. **Local Map**: For r≥1 regions within loaded extent, `dispatch_summary_tasks` reads tile elevations from Map, calls `select_center_z()`, writes complete region to cache
2. **Server SummaryBatch**: For regions beyond FIXED_STREAM_RADIUS, renet groups received additions by `MeshRegionKey` before inserting
3. **Flyover FlyoverSummaryTracker**: During admin flyover, `flyover_summary_dispatch` computes visible mesh regions via `visible_mesh_regions_in_band_ungated` (same function the mesh pipeline uses), diffs against tracked `MeshRegionKey` set. Each new region dispatches one async task that computes all 271 center_z values atomically via `AdminComposite.elevation_at()`, polled by `flyover_poll_summary_tasks`. No partial regions — a region is either fully cached or pending. Visible-set recomputation is throttled to 20 WU of horizontal movement.

One consumer reads it: the async mesh builder (`collect_and_build_summary_mesh`), which branches cleanly — r=0 reads Map (full tile geometry), r>0 calls `get_region()` (one DashMap shard lock, then 271 lock-free HashMap reads). Dispatch gates r>0 on `contains_region()` — mesh builds only dispatch when cache has complete data.

### Mesh Eviction

One rule for all r values: `dispatch_summary_tasks` evicts any mesh region not in the current `needed` set (computed from `compute_auto_mode_regions`). Mesh eviction is purely position-based — it does not depend on removal events from any producer.

### Cache Warming

SummaryCache entries persist when their mesh is evicted. Returning to a previously-visited area rebuilds instantly from cache hits rather than recomputing center_z. The server still sends removal `SummaryBatch` messages (tracking what each client has received via `VisibleSummaryCache`), but the client ignores them. Entries are only cleared on `clear()` (flyover toggle).

### Promotion and Demotion

When a player moves and a chunk crosses the streaming boundary:

- **Inward**: Server sends full ChunkData. Client computes local summaries, replacing server-provided values.
- **Outward**: Server sends SummaryBatch. Client drops Map tiles, relies on server-provided center_z values.

---

## Server-Side Summary Pipeline

### SummaryCache (Server)

Simple get/insert cache of center_z per `SummaryKey(r, sq, sr)`. No computation — populated by async tasks. Deterministic: same key always produces the same result.

### VisibleSummaryCache (Per-Client)

Component tracking which summaries have been sent to each client. Recomputes the visible set when the player moves >20 WU or >10 z-levels.

### dispatch_summary_tasks + poll_summary_tasks Systems

**dispatch_summary_tasks**: For each player, checks if movement threshold crossed. Computes active bands to visual horizon, enumerates visible **mesh regions** via `visible_mesh_regions_in_band_ungated()` (same granularity the client uses), expands to per-`SummaryKey` for per-client tracking, diffs against previously sent set.

Dispatch operates at `MeshRegionKey` granularity — each async task computes all ~271 center_z values for one region atomically. Regions are sorted **nearest-first** (by squared world-space distance to the player) so near terrain resolves before far terrain, and dispatch is clamped by `MAX_SUMMARY_TASKS = 16` in-flight regions across all players. Before dispatching, each region tries to satisfy from `SummaryCache` — fully-cached regions emit an immediate `SummaryBatch`; partially-cached regions dispatch async for the remainder. `SummaryTaskQueue` tracks in-flight `MeshRegionKey`s to prevent duplicates.

Each async task uses `sample_center_z()` (7 samples per summary — see Center Z Selection) over `EventRegistry::elevation_at()`.

**poll_summary_tasks**: Polls completed async tasks, inserts results into `SummaryCache`, updates per-client `VisibleSummaryCache`, and sends a single `SummaryBatch` per completed region. Per-region granularity means mesh builds on the client can proceed as each region lands rather than waiting for a whole frame's worth of keys.

### Wire Format

`SummaryData { r: u32, sq: i32, sr: i32, center_z: i32 }` — 16 bytes per summary. Minimal payload: the client builds geometry locally from center_z. No mesh data on the wire.

---

## Invariants

**INV-001 (extended)**: Summary data is rendering-only. Physics, movement, and pathfinding read the Map, never summary geometry. Server-sent summaries never enter the Map.

**INV-005 (extended)**: Server and client agree on the full-detail boundary (FIXED_STREAM_RADIUS = 21 chunks). Promotion/demotion is server-authoritative via SummaryBatch messages.

**INV-007 (preserved)**: Adjacent summaries sharing an edge produce matching canonical vertex IDs. No gaps — height differences are bridged by skirt quads.

**INV-008 (preserved)**: Summary center_z is always computed from original tile data (via Map or EventRegistry), never from previously-computed summaries. No cascading error.

**INV-009 (preserved)**: All summary center vertices are at discrete integer z-levels. No fractional heights.

---

## Constants

| Constant | Value | Derivation |
|----------|-------|------------|
| MIN_SCREEN_PX | 12 px | Minimum visual size for a summary hex |
| DEFAULT_VFOV | 60° (π/3) | Default vertical FOV for `summary_radius()` (gameplay). Callers of `compute_active_bands` pass FOV explicitly. |
| FIXED_STREAM_RADIUS | 21 chunks / 599 WU | Full-detail streaming boundary |
| HEX_OUTER_RADIUS | 1.0 WU | Single tile outer radius |
| Z_SCALE / RISE | 0.8 WU/z-level | Existing constant |
| MESH_REGION_RADIUS | 9 | Summaries per mesh region (271) |

---

## Future Extensions

- **Per-direction frustum culling**: Currently all regions within a band are rendered. Frustum sweep can reduce the set for asymmetric terrain.
- **Dynamic screen resolution**: Replace hardcoded 1080 with actual client resolution. Requires client→server communication.
- **Inscribed hex decimation**: Detail-preserving geometry (inner hex + residual wedges) for near-distance summaries where the flat hex approximation is too coarse. Would replace r=1–3 flat hexes with slope-aware decimated geometry.
- **Mega-chunks**: Same summary philosophy at a coarser spatial unit for extreme-distance bandwidth reduction.

---

## Implementation Deviations

**Nested-level redesign in progress** (see `.claude/plans/well-knit-world.md`):

- **Distance bands** are now one per nested LoD level (scales 1, 3, 9, 27,
  81, 243 — `LOD_LEVELS` in summary.rs), not one per integer r. Thresholds
  use a single FOV-independent quality constant `BAND_QUALITY_K = 119.75`,
  anchored so the scale-3 band's outer edge equals FIXED_STREAM_RADIUS_WU
  (ownership boundary == band boundary). The MIN_SCREEN_PX/pixel_scale
  formula in this spec is superseded.
- **Center Z Selection** is unified: `sample_center_z` (7 samples) is the
  only rule, for all producers including the local Map. The "accepted
  divergence" between full-hexball and 7-sample no longer exists. New
  invariant INV-010: a level's 7 sample points are exactly the child
  level's summary centers (tested: `lod_levels_sample_points_are_child_centers`).
- Producers (`visible_lod_regions`) also cover regions whose footprint
  straddles the stream radius, so boundary regions complete from server
  data; values agree with Map-computed ones by construction.
- Pending phases: cross-level frontier curtains (P2), hysteresis +
  build-before-evict transition protocol (P3). This spec needs a full
  ARCHITECT rewrite once those land.

## Implementation Gaps

**Deferred**: Dynamic resolution support, inscribed hex decimation, mega-chunk consolidation, per-direction frustum culling.

**Blocked by**: Nothing.

---

**Related Design Documents:**
- [Terrain Generation](terrain-generation.md) — Elevation pipeline, chunk streaming

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based world partitioning
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Superseded for rendering (QEM → summary hex); uniform full-detail send within FIXED_STREAM_RADIUS retained
