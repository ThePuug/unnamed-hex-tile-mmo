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

### The Inscribed Hex

For any radius-N hexball (containing 3N²+3N+1 tiles), there exists a regular flat-top hexagon fully inscribed within the hexball boundary. This hex is defined by 6 vertices, each located at a specific outer vertex of an edge tile on ring N.

The vertex selection follows two alternating patterns based on radius parity, reflecting the half-offset stagger of the hex grid. Odd radii (1, 3, 5...) use one consistent set of vertex indices; even radii (2, 4, 6...) use another. Both produce a regular hexagon centered on the hexball center.

The inscribed hex grows with radius. At r=1 the vertices sit at distance ~2R from center; at r=4 they sit at ~7R. The hex always has the same flat-top orientation as the constituent tiles.

### Decimated Mesh Structure

The decimated mesh has exactly two components:

**Inner Hex (always 6 triangles)**: A single center vertex fans to the 6 inscribed hex boundary vertices. Identical topology to a single tile at any scale.

The center vertex sits at a discrete integer z-level, chosen to minimize maximum deviation from all tiles within the hexball. The 6 boundary vertices sit at original grid positions — their heights are the real blended heights from the original mesh, derived from adjacent tile centers. Not approximated, not snapped.

**Residual Wedges (6 identical clusters)**: Each of the 6 hexball "points" produces a wedge of surviving geometry radiating outward. A wedge contains two types of tiles:

- *Partial tiles*: Their centers sit exactly on the inscribed hex edge. They are split — inward-facing triangles absorbed, outward-facing triangles survive as a fan (3 triangles per partial tile) radiating from the tile center to hexball perimeter vertices. Partial tile centers snap to the linearly interpolated z between the two adjacent hex boundary vertices (T-junction resolution — see below).
- *Full residual tiles*: Sit entirely outside the inscribed hex. All 6 triangles survive unmodified.

The residual structure pairs by radius:

| Radius pair | Partial fans per side | Full tiles per side | Residual tri per side |
|-------------|----------------------|--------------------|-----------------------|
| r=1, r=2    | 1                    | 0                  | 3                     |
| r=3, r=4    | 2                    | 1                  | 12                    |
| r=5, r=6    | 3                    | 2                  | 21                    |
| r=N, r=N+1  | ceil(N/2)            | ceil(N/2) - 1      | 9·ceil(N/2) − 6       |

### Triangle Budget

| Radius | Tiles | Original tri | Inner hex | Residual | Eliminated | Reduction |
|--------|-------|-------------|-----------|----------|------------|-----------|
| 1      | 7     | 42          | 6         | 18       | 18         | 43%       |
| 2      | 19    | 114         | 6         | 18       | 90         | 79%       |
| 3      | 37    | 222         | 6         | 72       | 144        | 65%       |
| 4      | 61    | 366         | 6         | 72       | 288        | 79%       |

Original triangles grow as O(r²). Residual triangles grow as O(r). The decimation ratio improves with radius — at large radii the inner hex dominates and the residual is a thin fringe.

### T-Junction Resolution

Partial tile centers sit exactly on the inscribed hex edge. If left at their original z-level they would be T-junctions — vertices of residual fan triangles touching the inner hex triangle edge without being vertices of those triangles. This causes rasterization cracks.

**Resolution**: Partial tile centers snap to the linearly interpolated z between the two hex boundary vertices at either end of the edge they sit on. Since the inner hex triangle already interpolates linearly across that edge, the snapped center lies exactly on the triangle's plane. No crack, no floating-point precision issue.

Snapped partial tile centers are generally not integer z-levels. This is the single exception to discrete center z. INV-009 applies to the inner hex center only.

The residual fan triangles then express the slope from the snapped center (on the inner hex surface) outward to real hexball perimeter vertices — a smooth transition from decimated interior to surviving perimeter detail.

### Vertex Heights

| Vertex type                  | Height source                                      | Discrete z? |
|------------------------------|---------------------------------------------------|-------------|
| Inner hex center             | Chosen to minimize max deviation across all tiles  | Yes         |
| Inner hex boundary (6)       | Real blended height, clamped to ±N×RISE from center | Real grid   |
| Partial tile center (on edge)| Interpolated from the two adjacent hex vertices    | No (snapped)|
| Residual tile centers        | Original discrete z-level                          | Yes         |
| Residual perimeter vertices  | Original grid positions, original heights          | Real grid   |

### Slope Blending at Scale

The terrain's visual vocabulary has a maximum slope rate: **±0.4 WU per tile radius of horizontal distance** (half of RISE per tile). This rate is constant in the original mesh and must be respected in decimated geometry.

For a radius-N hexball the total height budget from center to perimeter is:

```
max_drop = (2N + 1) × (RISE / 2) = (2N + 1) × 0.4 WU
```

This budget splits between the inner hex and the residual fans:

| Component | Height budget |
|-----------|--------------|
| Inner hex (center → boundary vertex) | N × RISE = N × 0.8 WU |
| Residual fan (boundary vertex → perimeter vertex) | ±0.4 WU (one half-tile) |

**Clamping**: After computing the real blended height for an inner hex boundary vertex, it is clamped:

```
boundary_y = clamp(real_blended_y, center_y − N×RISE, center_y + N×RISE)
```

The real blended height is used for residual fan outer vertices (where they interface with external geometry). Those are additionally clamped so the total center-to-perimeter drop does not exceed `(2N+1) × 0.4 WU`.

**Cliff faces at the perimeter**: The difference between the clamped perimeter height and the actual height of adjacent external geometry becomes a cliff face (skirt quad). This is where z-range that the hexball cannot express as slope is pushed. The hexball reads as a gently sloping plateau with a cliff edge — accurate to the eye at distance when individual steps cannot be resolved.

Flat regions remain flat (center z matches boundary z, no clamping activates). Threshold 0 only decimates planar regions, so clamping never activates at threshold 0.

### Stitching

All boundary vertices between adjacent decimated hexballs resolve to positions and heights that exist in the original grid. Adjacent hexballs sharing a perimeter edge compute identical vertex positions from identical underlying tile data. Deterministic, seamless, no coordination protocol needed.

Different LoD levels (different decimation radii or undecimated full-detail tiles) tile together because all perimeter vertices are real grid vertices at real heights.

### Scaling Property

This geometry is self-similar. A radius-1 decimation and a radius-4 decimation produce the same structural pattern — an inner hex of 6 triangles plus residual wedges. The only differences are the hex size, the number of tiles in each residual wedge, and the z-threshold required to absorb the interior. A single code path handles all decimation radii.

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

**Threshold 0 (lossless)**: A hexball decimates only when the resulting mesh is geometrically identical to the original full-detail triangles. This requires the elevation field across all tiles in the group to be planar — flat terrain or a uniform slope with no curvature and no internal variation. Every eliminated vertex must lie exactly on the plane of the triangle replacing it.

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

**INV-009 (new)**: The inner hex center vertex is at a discrete integer z-level.

**INV-010 (new)**: No decimated hexball expresses a steeper slope than the original mesh's maximum rate (±0.4 WU per tile radius). Inner hex boundary vertices are clamped to ±N×RISE from the center. Any z-range that exceeds this budget is expressed as a cliff face (skirt) at the hexball perimeter. Exception: partial tile centers on inscribed hex edges are snapped to the linearly interpolated z between their two adjacent hex boundary vertices (T-junction resolution). These are not integer z-levels, but they lie exactly on the inner hex triangle surface — no crack, no visual artifact.

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

(None — spec precedes full implementation)

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
