# Continuous Level-of-Detail

## Player Experience Goal

Terrain extends to the horizon in every direction. A player standing on a z=4000 peak looks down across an entire continent of foothills, valleys, and distant plains — all rendered with the same visual vocabulary of flats, gentle slopes, and cliffs. No atmospheric fade hiding missing geometry. No terrain ending abruptly at a radius boundary. The world reads as a continuous landscape at every distance, coarsening naturally with perspective — the way real terrain looks from a mountaintop.

## Design Thesis

**Hex-native decimation with continuous distance-driven thresholds replaces QEM mesh decimation.** The unit of decimation is the hex tile, not the mesh vertex. Decimation consolidates groups of tiles into larger hexagonal regions at discrete z-levels, preserving the terrain's visual character (flat, gentle slope, cliff) at every scale. The LoD threshold is a continuous function of distance, not a fixed number of tiers.

---

## Problem: QEM Produces Visually Incongruous Terrain

The current QEM (Quadric Error Metrics) decimation optimizes for geometric proximity — the resulting surface is close to the original in world-space distance. But the terrain's visual identity is defined by discrete elevation steps, not continuous surfaces. QEM collapses cliff faces into gentle slopes, introduces fractional z-levels that don't exist in the terrain system, and produces elevation colors that don't correspond to any real terrain type.

The terrain has three visual modes: flat (all vertices at same z), gentle slope (±0.4 WU blend between adjacent z-levels), and cliff (vertical face where Δz exceeds blend range). QEM destroys this vocabulary by interpolating through it.

---

## Hex-Native Decimation

### Hexball Grouping

The decimation unit is a radius-1 hexball: a center tile plus its 6 neighbors (7 tiles). The center tile's z-level is selected to minimize maximum deviation from any tile in the group. When multiple z-levels tie, any may be chosen (deterministic tiebreaker by lowest z).

The center z is always a discrete integer. No fractional heights. No interpolation.

### Decimated Geometry

The decimated hexball does not render as a hexball mesh. Instead, each neighbor tile is split in half by connecting its two "close" vertices — the vertices shared with its adjacent neighbors (not shared with the center tile). This produces:

**Inner hex**: A regular hexagon whose 6 vertices sit at the close vertices of the 6 neighbors. The center vertex sits at the decimated z-level. 6 triangles fan from center to these vertices. This region absorbs the entire center tile plus the inward-facing half of each neighbor.

**Outer fans**: Each neighbor retains its 3 outward-facing triangles (the half beyond the split line), radiating from the neighbor's original center toward the hexball perimeter. These preserve the hexball's outer silhouette exactly.

Total mesh: 1 center vertex + 6 inner hex boundary vertices + 6 neighbor centers + 12 hexball perimeter vertices = 25 vertices, 24 triangles. Down from 7 × 7 = 49 vertices and 7 × 6 = 42 triangles at full detail.

### Vertex Heights

**Center vertex**: Discrete z-level chosen by minimizing max deviation across the group.

**Inner hex boundary vertices (6)**: These sit at original grid positions where two neighbor tiles meet. Their height is the real blended height from the original mesh — derived from the two adjacent neighbor centers. Not approximated, not snapped.

**Outer fan vertices**: All at original tile positions with original heights. No modification.

### Slope Blending at Scale

The existing rendering rule applies identically: each triangle blends from its center vertex z toward the boundary vertex height. At r=1 tile scale, the blend range is ±0.4 WU (half of RISE = 0.8 WU per z-level). The inner hex of a decimated hexball has roughly twice the center-to-vertex distance, so slope expression scales to ±0.8 WU — one full z-level.

This means:

- A 1-z-level step across the inner hex reads as a gentle slope (same visual character as a 1-z step across a single tile)
- A 3-z-level step that was three adjacent cliffs at full detail consolidates into one steeper cliff (accurate to the eye at distance — individual steps are sub-pixel)
- Flat regions remain flat (center z matches boundary z)

### Stitching

Adjacent decimated hexballs share boundary vertices at original grid positions with real blended heights. Deterministic, seamless, no coordination needed — the same property the current system relies on.

Different LoD levels can tile together because all boundary vertices resolve to discrete or real-blended heights on the original grid. A decimated hexball next to full-detail tiles shares perimeter vertices that exist in both representations at identical positions.

---

## Threshold Model

### Continuous Threshold from Distance

The threshold at distance D (in world units) is derived from vertical visual acuity:

```
threshold(D) = floor(P × 2 × D × tan(FOV/2) / (RISE × screen_height))
```

Where:
- P = TILE_PIXEL_THRESHOLD (4 pixels)
- FOV = 60° (worst-case vertical acuity, used uniformly)
- RISE = 0.8 WU per z-level
- screen_height = 1080 (reference resolution; future: dynamic)

This yields integer thresholds at specific distance bands:

| Threshold | Distance (chunks @ 60° FOV) |
|-----------|-----------------------------|
| 0         | 0–7                         |
| 1         | 7–13                        |
| 2         | 13–20                       |
| 3         | 20–26                       |
| 4         | 26–33                       |
| 5         | 33–40                       |

### Threshold Semantics

**Threshold 0 (lossless)**: A hexball decimates only when the resulting 24-triangle mesh is geometrically identical to the original 42 triangles. This requires the elevation field across the 7-tile group to be planar — flat terrain or a uniform slope with no curvature and no internal variation. Every removed vertex must lie exactly on the plane of the triangle replacing it.

**Threshold N > 0**: The center z may deviate up to N z-levels from any tile in the group. Internal staircase structures (multiple small cliffs) consolidate into fewer, taller cliffs. The terrain character (flat/slope/cliff) is preserved — only the resolution of elevation steps changes.

### Recalculation from Original Geometry

When a chunk crosses a threshold boundary (the threshold function returns a new integer for that chunk's distance), the decimation is recalculated from the original 271-tile data. Never cascaded from a previous threshold. This eliminates compounding approximation error and ensures every threshold level is an exact function of the original tiles.

---

## Two Rendering Regimes

### Client-Owned (Threshold 0–3, within ~20 chunks)

The server sends full 271-tile ChunkData, same as today. The client stores tiles in the Map, owns the full LoD lifecycle, and runs hex-native decimation locally. The client recalculates decimation when a chunk crosses a threshold boundary due to player movement.

The 20-chunk boundary (~3 threshold bands at 60° FOV) ensures the client has enough headroom to render all terrain up to threshold 3 from local data.

### Server-Owned (Threshold 4+, beyond ~20 chunks)

The server generates and caches decimated terrain, sending a separate message type. The client renders what it receives without recalculation — it doesn't have the underlying tile data. Decimated chunks are replaced only when the server sends a new version (on threshold band crossing) or evicts them.

**Separate message type, not compressed ChunkData.** The two regimes have fundamentally different contracts:

- ChunkData → client stores tiles, runs decimation, recalculates on distance changes
- DecimatedChunkData → client renders as-is until replaced, never enters the Map

This preserves INV-001/INV-006: server-decimated chunks are rendering-only. Physics, movement, and pathfinding never see them.

### Promotion and Demotion

When a player moves and a chunk crosses the 20-chunk boundary:

- **Inward (demotion → promotion)**: Server sends full ChunkData. Client drops the decimated rendering, inserts tiles into Map, takes over LoD lifecycle.
- **Outward (promotion → demotion)**: Server sends DecimatedChunkData at the appropriate threshold. Client removes tiles from Map, switches to server-provided rendering.

---

## Server-Side Decimation Pipeline

### Generation

The server runs the same hex-native decimation as the client. Input: full 271-tile chunk from WorldDiscoveryCache. Output: decimated geometry at a specific threshold. The function is pure: `decimate(tiles, threshold) → DecimatedChunk`.

### Caching

Cache key: `(ChunkId, threshold)`. The output is deterministic — same tiles and threshold always produce the same result. Multiple players at different positions but similar distances to the same chunk share the cache entry.

LRU eviction is viable if decimation proves fast (expected — it's a local scan over 7-tile groups with integer comparisons). The cache is a bandwidth optimization, not a correctness requirement.

### Visibility Calculation

The server must fill the player's entire screen with terrain, including buffer to prevent edge flicker. The visible range depends on the camera's height above distant terrain.

**Camera geometry**: Camera at 90 WU above player + player elevation × RISE, pitched ~36.3° below horizontal. Vertical FOV ~36° (60° horizontal on 16:9). Top-of-frustum ray is ~6.3° below horizontal.

**Frustum sweep**: The server sweeps outward from the player ring by ring. For each chunk, it checks whether the terrain in that chunk (using `chunk_max_z`) is high enough to intersect the frustum at that distance. Once every chunk in a ring falls below the frustum floor from that direction, that direction terminates.

This produces an asymmetric visible set driven by actual terrain elevation:
- A valley direction extends far (terrain is low, camera looks over it)
- A ridge direction terminates sooner (terrain rises to meet the frustum)
- A z=4000 peak player sees a vast area, but extreme-distance chunks are at very high thresholds and nearly free on the wire

**Conservative approach**: Sea-level player at 60° FOV sees ~24 chunks to frustum termination. Mountain player (z=4000) could theoretically see ~1045 chunks assuming sea-level terrain, but real terrain elevation truncates this significantly per-direction. At extreme thresholds (100+), entire chunks reduce to a single elevation value — bytes per chunk.

### Wire Format

DecimatedChunkData is a purpose-built message. It does not reuse ChunkData encoding.

Exact format TBD during implementation, but the payload per chunk at high thresholds is small: a ChunkId, the threshold it was built at, surviving hexball centers with z-levels, and any un-decimatable tiles. At extreme distances, most chunks are a single z-value.

---

## Invariants

**INV-001 (extended)**: DecimatedChunkData is rendering-only. Physics, movement, and pathfinding read the Map, never decimated chunks. This extends the existing summary separation invariant to cover server-side decimated data.

**INV-005 (extended)**: Server and client agree on the 20-chunk full-detail boundary. Promotion/demotion is server-authoritative via new message types.

**INV-007 (preserved)**: Adjacent chunks at any LoD threshold share deterministic boundary vertices. No gaps. Guaranteed by all boundary vertices resolving to original grid positions with real blended heights.

**INV-008 (new)**: Decimation at any threshold recalculates from original tile data, never from a previous threshold's output. No cascading error.

**INV-009 (new)**: All decimated center vertices are at discrete integer z-levels. No fractional heights on centers.

---

## Constants

| Constant | Value | Derivation |
|----------|-------|------------|
| TILE_PIXEL_THRESHOLD | 4 px | Visual acuity floor |
| THRESHOLD_FOV | 60° (π/3) | Worst-case vertical subtension |
| FULL_DETAIL_RADIUS | ~20 chunks | Threshold 3 boundary at 60° FOV |
| REFERENCE_SCREEN_HEIGHT | 1080 px | Baseline; future: dynamic |
| RISE | 0.8 WU/z-level | Existing constant |

---

## Future Extensions

- **Dynamic screen resolution**: Replace hardcoded 1080 with actual client resolution for threshold calculation. Requires client→server communication of display parameters.
- **Mega-chunks**: If extreme-distance bandwidth remains a concern, the same decimation philosophy can apply at a coarser spatial unit — groups of chunks consolidated into larger hexagonal regions.
- **Frustum culling within threshold bands**: Currently all chunks within a threshold band are sent. Directional frustum culling can further reduce the set.

---

## Implementation Deviations

(None yet — spec precedes implementation)

## Implementation Gaps

**Current**: Wire format for DecimatedChunkData not yet defined. Visibility sweep algorithm not yet specified in detail.

**Deferred**: Dynamic resolution support, mega-chunk consolidation.

**Blocked by**: Nothing — this system is independently implementable.

---

**Related Design Documents:**
- [Terrain Generation](terrain-generation.md) — Elevation pipeline, chunk streaming

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based world partitioning
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Superseded for rendering (QEM → hex-native); uniform full-detail send within FULL_DETAIL_RADIUS retained
