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

The center z is chosen to preserve the terrain's most visually prominent feature within the hexball:

1. Compute the mean z across all tiles in the hexball
2. Select the tile with the greatest absolute deviation from the mean
3. Tiebreak: prefer higher z (peaks over valleys)

This approximates "most extreme point" — summaries tend to show peaks and ridges rather than averaging them away. The center z is always a discrete integer.

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
- MIN_SCREEN_PX = 16 pixels (minimum visual size for a summary hex)
- tile_diameter = 2 × HEX_OUTER_RADIUS = 2.0 WU
- pixel_scale = screen_height / (2 × D × tan(FOV/2))
- FOV = 60° (worst-case vertical)

This ensures every summary subtends at least MIN_SCREEN_PX on screen. As distance increases, r increases — summaries cover more tiles but remain visually legible.

### Distance Bands

Inverting `summary_radius(D)` produces discrete distance thresholds where r changes. `compute_active_bands(max_distance)` returns a sorted list of `(r, inner_wu, outer_wu)` bands from the camera to the visual horizon. Each band uses a single summary radius.

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

A single `SummaryCache` (Arc-wrapped) serves as the client's source of truth for all r≥1 summary data. Three producers write to it:

1. **Local Map**: For r≥1 regions within FIXED_STREAM_RADIUS, `dispatch_summary_tasks` reads tile elevations from Map, calls `select_center_z()`, writes to cache
2. **Server SummaryBatch**: For regions beyond FIXED_STREAM_RADIUS, received summaries are written directly
3. **Flyover FlyoverSummaryTracker**: During admin flyover, `flyover_summary_dispatch` computes the visible summary set (same `compute_active_bands` + `visible_summary_cells_in_band` as server), diffs against tracked state, and feeds both additions and removals through `apply_batch()`. Cache misses dispatch async tasks that compute center_z via `AdminComposite.elevation_at()`, polled by `flyover_poll_summary_tasks`. Visible-set recomputation is throttled to 20 WU of horizontal movement.

One consumer reads it: the async mesh builder (`collect_and_build_summary_mesh`), which branches cleanly — r=0 reads Map (full tile geometry), r>0 reads SummaryCache (one lookup per summary).

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

**dispatch_summary_tasks**: For each player, checks if movement threshold crossed. Computes active bands to visual horizon, enumerates summary cells via `visible_summary_cells_in_band()`, diffs against previously sent set. Cache misses are dispatched to `AsyncComputeTaskPool` — each task iterates the hexball's tiles, calls `EventRegistry::elevation_at()`, runs `select_center_z()`. `SummaryTaskQueue` tracks in-flight keys to prevent duplicates.

**poll_summary_tasks**: Polls completed async tasks, inserts results into SummaryCache. Sends completed summaries to clients as `SummaryBatch` (additions + removals). No per-frame budget — async tasks don't block the tick.

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
| MIN_SCREEN_PX | 16 px | Minimum visual size for a summary hex |
| THRESHOLD_FOV | 60° (π/3) | Worst-case vertical subtension |
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

(None — spec matches implementation)

## Implementation Gaps

**Deferred**: Dynamic resolution support, inscribed hex decimation, mega-chunk consolidation, per-direction frustum culling.

**Blocked by**: Nothing.

---

**Related Design Documents:**
- [Terrain Generation](terrain-generation.md) — Elevation pipeline, chunk streaming

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based world partitioning
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Superseded for rendering (QEM → summary hex); uniform full-detail send within FIXED_STREAM_RADIUS retained
