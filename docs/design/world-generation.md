# Terrain Generation

## Design Philosophy

The world should feel like a place with real geography, not a noise field with textures. Terrain generation is built on the principle that **structure comes from geology, detail comes from noise**. Continental-scale features like mountain ranges, rift valleys, and highland plateaus emerge from procedural tectonic simulation, while local variation comes from layered noise parameterized by the geological context.

This architecture serves the hostile world thesis: the world's terrain is not static decoration but a living system that builds pressure and breaks. Tectonic boundaries are seams of geological instability where earthquakes, eruptions, and terrain-reshaping events concentrate. Players experience a world that is actively geologically hostile — and that hostility has visible, comprehensible structure they can learn to read.

## Generation Pipeline

Height at any tile is evaluated as a pure function of tile coordinates and world state. No global precomputation, no startup pass — every layer is procedurally evaluable per-tile, satisfying ADR-001's deterministic generation invariant.

```
tile_height(q, r) =
    continental_base_elevation(q, r)       // which continental plate, its base height
  + continental_boundary_contribution(q, r) // proximity to continental plate boundaries
  + regional_boundary_contribution(q, r)    // proximity to regional plate boundaries
  + feature_envelope(q, r)                  // nearby geographic features (mountains, swamps, etc.)
  + biome_detail_noise(q, r)               // biome-specific surface texture
  + post_processing(q, r)                   // stratification, channel carving, smoothing
```

Each layer depends only on tile coordinates, the world seed, and (for the dynamic layer) a small set of evolving boundary intensity parameters. Chunk caching means each tile is evaluated once and stored immutably per ADR-001.

---

## Layer 1: Tectonic Plates (Hierarchical Dual-Scale)

Terrain structure uses a **hierarchical two-scale plate system**. Continental plates define macro geography (continents, ocean basins, major mountain ranges). Regional plates exist *within* continental plates and add mid-scale structural variation (secondary ridges, rifts, and terrain texture) without contributing independent base elevation.

### Continental Plate Placement

Continental plates are defined by a Voronoi tessellation over procedurally placed plate centers. Centers live on a coarse grid (4,000 × 4,000 tiles per cell). Each cell's center position is determined by hashing `(cell_q, cell_r, seed)` to produce:

* **Position offset** (jitter within cell): fixed fraction (30%) of cell size
* **Plate character**: continental or oceanic, determined by hash threshold
* **Base elevation**: bimodal distribution driven by plate character — continental plates sit in the 400–1,200 range, oceanic plates sit in the 50–200 range. The ~200–400 gap between them is where coastlines naturally form.
* **Drift vector**: direction and magnitude of plate movement (used for boundary classification)

The continental/oceanic ratio is approximately 60/40, producing significant coastline without making the world feel mostly submerged.

Coastlines emerge naturally at Voronoi boundaries between continental and oceanic plates — a continental plate at base elevation 500 adjacent to an oceanic plate at 150 creates a slope that reads as shoreline. The boundary classification still applies: a convergent continental-oceanic boundary creates coastal mountain ranges (subduction zones), while a divergent one creates broad shallow coastal shelves.

For any tile, determining plate membership requires checking surrounding grid cells (5×5 neighborhood), computing distance to each plate center, and selecting the nearest — standard Voronoi evaluation.

### Regional Plates

Regional plates provide mid-scale structural variation within continental plate interiors. They live on a finer grid (1,200 × 1,200 tiles per cell) and contribute **boundary effects only** — no independent base elevation. A tile's base elevation comes exclusively from its continental plate.

Regional plate properties:

* **Grid size**: 1,200 tiles per cell (roughly 3× finer than continental)
* **Jitter**: noise-driven, varying between 15–45% of cell size. High-jitter regions produce more irregular plate shapes; low-jitter regions produce more regular ones.
* **Skip probability**: some regional grid cells produce no plate. Skip probability is inversely correlated with jitter intensity — chaotic (high-jitter) regions have more regional plates, stable (low-jitter) regions have fewer. Skip ranges from 0% to 80%.
* **Continental membership**: each regional plate belongs to the continental plate whose Voronoi cell contains its center. Regional plates do not cross continental boundaries. Membership is computed by running the continental Voronoi lookup on each regional center's position.

The regional Voronoi search uses a wider neighborhood (9×9) than continental (5×5) because the smaller grid size means more candidates at similar distances.

### Domain Warping (Curl Noise)

Both plate scales apply domain warping to their Voronoi lookup coordinates, producing organic boundary shapes instead of geometric polygons. The warp uses **curl noise** — displacement derived from the gradient of a single scalar noise field, rotated 90°.

Curl noise produces a divergence-free vector field: adjacent tiles always get smoothly varying displacement, with no opposing vectors. This eliminates cusp artifacts that occur with independent-axis warping, where two independent noise fields for q and r displacement can produce opposing vectors at adjacent tiles, causing boundaries to fold sharply where warp direction reverses.

Implementation:
1. Evaluate a scalar noise field at the tile's Cartesian position
2. Compute finite-difference partial derivatives (∂n/∂x, ∂n/∂y)
3. Rotate 90° to get displacement: warp_x = ∂n/∂y, warp_y = -∂n/∂x
4. Convert Cartesian displacement back to hex axial coordinates
5. Add displacement to tile coordinates before Voronoi lookup

Parameters:
* **Continental**: amplitude ~600 tiles, noise scale ~1/3,000
* **Regional**: amplitude ~150 tiles, noise scale ~1/800
* One noise seed per scale (curl uses a single field, not two)

#### Alternatives tried and rejected

* **Independent-axis warping** (two noise fields for q and r): Produced cusp artifacts where warp vectors reversed direction along boundaries.
* **Lower warp frequency** (1/8,000 continental): Reduced cusps but revealed polygon geometry by removing boundary meandering.
* **Center-based warping** (evaluate noise at plate centers, blend by proximity): Produced rigid plate translations with straight edges — too coarse for organic shapes.

### Boundary Classification (Per-Pair)

Boundary type is determined **per plate pair**, not per tile. The pair normal is the vector from plate A's center to plate B's center (constant along the entire shared boundary). The relative drift vector (`drift_A - drift_B`) dotted with this pair normal determines the boundary type:

**Convergent** (convergence > threshold): Plates pushing toward each other. Produces uplift — mountain ranges, highland ridges. Intensity scales with convergence magnitude above threshold.

**Divergent** (convergence < -threshold): Plates pulling apart. Produces subsidence — rift valleys, depressions. Intensity scales with divergence magnitude above threshold.

**Transform** (|convergence| ≤ threshold): Plates sliding past each other. Produces moderate terrain disruption — fault lines, broken terrain. The threshold (0.15) prevents weakly-convergent boundaries from flickering between types.

Per-pair classification ensures every tile along a shared boundary gets the same type — a convergent boundary produces a continuous mountain range along its entire length, not isolated hotspots.

### Boundary Intensity Variation (Per-Tile)

While boundary *type* is uniform per pair, boundary *intensity* varies along the edge via per-tile noise modulation:

```
final_intensity = pair_intensity × (0.5 + 0.5 × noise(tile_position, variation_seed))
```

This breaks up uniform polygon outlines while preserving continuous type classification. A convergent boundary becomes a mountain range with tall peaks and lower passes rather than a uniform ridge tracing the polygon edge.

* **Continental boundaries**: noise scale ~1/1,000 (~4 cycles per boundary)
* **Regional boundaries**: noise scale ~1/300 (~4 cycles per boundary)
* Dedicated noise seed, decorrelated from terrain and domain warp noise

### Triple Junction Dampening

Where three or more plates meet, the Voronoi boundary math breaks down — the second-nearest plate assignment changes rapidly and multiple boundary contributions interact unpredictably, producing spikes and discontinuities.

The fix tracks the **third-nearest** plate center alongside the nearest two:

```
junction_factor = ((d3 - d2) / d2).clamp(0.0, 1.0)
```

* Far from triple junction (d3 >> d2): junction_factor → 1.0 (full boundary contribution)
* At triple junction (d3 ≈ d2): junction_factor → 0.0 (suppressed)

Applied as a multiplier on boundary contribution at both continental and regional scales. Smoothly fades contributions to zero near junction vertices without affecting normal two-plate boundaries.

### Boundary Elevation Contribution

The boundary's effect on tile height follows a quadratic falloff curve from the Voronoi edge:

```
boundary_contribution = peak × pair_intensity × local_variation × falloff(distance) × junction_factor
falloff = 1 - (distance / max_distance)²
```

The influence distance and peak contribution depend on boundary type and scale:

| Scale | Type | Max Distance | Peak Elevation |
|-------|------|-------------|----------------|
| Continental | Convergent | 1,500 tiles | +800 |
| Continental | Divergent | 600 tiles | -300 |
| Continental | Transform | 300 tiles | ±40 (noise) |
| Regional | Convergent | 400 tiles | +100 |
| Regional | Divergent | 200 tiles | -80 |
| Regional | Transform | 150 tiles | ±15 (noise) |

Convergent boundaries add elevation (positive contribution). Divergent boundaries subtract it (negative contribution). Transform boundaries add noise-modulated variation rather than elevation bias.

Base elevation blends linearly across continental boundaries over 300 tiles, preventing cliff-height discontinuities where different-elevation plates meet.

### Drift Vector Generation

Drift vectors need to produce a good mix of boundary types across the world. If all plates drift the same direction, most boundaries are transform (boring). Drift direction uses a regional noise field (scale ~1/5,000) combined with per-plate hash variation (±π/2):

* The noise field creates regionally coherent drift patterns
* Per-plate hash variation creates local divergence within regions
* Drift magnitude ranges 0.5–1.0 (hash-determined)

This means some regions of the world have mostly convergent boundaries (mountain-heavy), some have mostly divergent (rift-heavy), and some have a healthy mix. The distribution emerges from the drift field rather than being manually tuned.

---

## Layer 2: Base Terrain (Ambient Geography)

On top of the tectonic structure, two noise sub-layers provide surface variation that makes terrain feel natural rather than mathematical. Tectonic plates provide ALL structural geography; noise provides only texture with no structural meaning.

**Continental texture** (scale ~1/800, ±100 units): Broad undulation within plate interiors. This is what makes crossing a large stable plate feel like traversing varied terrain rather than a flat shelf. A player walking for thousands of tiles across a single plate's interior should experience gradual elevation changes from this layer.

**Micro-texture** (scale ~1/30, ±3 units): Subtle per-tile variation preventing uncanny smoothness. Almost imperceptible in isolation but prevents flat areas from looking artificially generated.

The original three-layer design included a regional texture layer (~1/500, ±40 units), but this was dropped. At ~1/200 scale, regional texture puts ~6 noise cycles per regional plate — the regional plate system already provides mid-scale structural variation through boundary ridges and rifts. Adding smooth rolling-hill noise at a similar spatial frequency muddies the plate signal rather than enhancing it.

---

## Layer 3: Feature Envelopes (Geographic Landmarks)

Features are distinct geographic landmarks placed on top of the tectonic + base terrain. Each feature has a **heart** (its defining point — a mountain peak, a swamp's deepest point, a plain's central formation) and an **envelope** that modifies terrain height based on distance from the heart.

### Feature Placement

Features live on a placement grid (500 × 500 tiles per cell). Each cell hashes `(cell_q, cell_r, seed)` to determine: does this cell contain a feature? What type? Where within the cell? What are its parameters (intensity, radius, orientation)?

One feature per cell guarantees minimum spacing — no overlapping mountain peaks. For any tile, checking the surrounding grid cells (9–25 depending on maximum feature radius) identifies all nearby features whose envelopes might contribute.

Feature type selection is influenced by tectonic context:

* **Mountain peaks** strongly favor convergent boundary zones
* **Rift features** favor divergent boundaries
* **Plains formations** favor stable plate interiors
* **Swamps** favor areas that are locally low relative to surroundings (emergent from tectonic + base terrain)

This is a bias, not a hard rule. Occasionally a mountain rises from a plain. Occasionally a swamp sits in a mountain saddle. But the distribution follows geological logic.

### Feature Types

**Mountain**: The envelope rises dramatically toward the peak. Height contribution scales with `(1 - (distance / radius)²)` or similar, creating a broad base that accelerates upward. Ridge noise (`1.0 - abs(noise)`) creates linear features radiating outward — ridges and valleys that provide interesting traversal along the ascent. The peak is a genuine destination at potentially 2,000–3,000 units above local base terrain. Approach paths emerge naturally at saddle points and valley cuts. Most of the circumference is steep ridge face, creating the "not approachable from just anywhere" property.

**Swamp**: The envelope gently depresses terrain toward the heart. Height variance is compressed — everything stays relatively flat with micro-channels from low-amplitude ridge noise. The swamp's effect is relative: it pulls height *down from whatever the ambient terrain is*, so a highland swamp and a lowland swamp both feel like depressions in their local context.

**Plains**: The envelope is nearly flat with very gentle rolling. The heart is a distinctive geological formation — a tor, a sinkhole, a crater with raised rim, a mesa. Something that says "this plain formed around this thing." Plains are where simple noise layers shine; the heart feature provides the landmark.

**Rift/Canyon**: Deep linear feature following divergent boundary geometry. The envelope cuts downward along a line rather than radiating from a point. Walls are steep, floor is relatively flat with river-channel texture.

Additional feature types will be defined as biome and event systems develop.

### Feature Interaction

Where multiple feature envelopes overlap (e.g., two mountains in adjacent cells forming a range), contributions blend by weighted proximity. The closest feature dominates, with distant features contributing diminishing influence. This prevents hard seams between features while preserving each feature's distinct identity.

Features placed along convergent plate boundaries naturally form ranges — a series of peaks connected by the boundary's own uplift, with saddle points between them where the feature envelopes dip but the boundary contribution maintains elevation.

---

## Layer 4: Biome-Specific Detail Noise

Each biome type applies its own detail noise profile on top of the structural layers. This is where the current system's Perlin layers live, but parameterized by biome context rather than applied universally:

**Mountain biome**: Ridge noise for sharp linear features, high-amplitude detail at steep slopes for cliff-face texture, stratification (height quantized to multiples of a step size) on high-slope areas to create cliff ledges.

**Swamp biome**: Compressed amplitude, channel-carving noise for water features, occasional hummocks.

**Plains biome**: Gentle rolling noise, minimal stratification, smooth transitions.

**Rift biome**: Vertical wall texture, flat-floor noise, ledge features along walls.

The biome is determined by the dominant feature influence at each tile. Boundary zones between biomes use blended noise parameters for smooth transitions.

---

## Layer 5: Post-Processing

Per-biome rules applied as final adjustments:

* **Stratification**: In mountain biomes, high-slope areas have height quantized to step multiples, creating visible cliff ledges. The step size and slope threshold are biome parameters.
* **Channel carving**: In swamp biomes, low points are deepened slightly along connected paths, suggesting water flow.
* **Smoothing**: In plains biomes, sharp transitions are softened.

---

## Dynamic Tectonic Layer

### Static vs Dynamic State

**Static** (determined by seed, never changes): Plate centers, Voronoi structure, drift vectors, base elevations. This is the world's geological skeleton.

**Dynamic** (evolves over game time, persisted as world state): Boundary intensities. One float per plate boundary — a tiny state footprint for the entire world. These values creep slowly over game time, representing geological pressure building or releasing.

### Pressure Cycle

Boundary intensity evolves through a repeating cycle:

1. **Accumulation**: Intensity creeps upward slowly over weeks/months of game time. Convergent boundaries build compression. Divergent boundaries build tension.

2. **Threshold events**: When intensity crosses thresholds, geological events trigger along segments of the boundary. Earthquakes reshape local terrain. Volcanic eruptions place permanent features.

3. **Release**: Events partially reduce intensity. The boundary settles, but permanent consequences remain — new terrain features, altered height maps.

4. **Recovery**: Intensity begins creeping again from the post-event baseline.

This cycle means the world's terrain is never truly static. Mountain ranges grow taller over months. Rift valleys deepen. The pressure is invisible until it breaks — then the world reshapes around the players.

### Per-Tile Evaluation with Dynamic State

The height function becomes:

```
tile_height(q, r, boundary_state) =
    continental_base_elevation(q, r)
  + continental_boundary_contribution(q, r, boundary_state)
  + regional_boundary_contribution(q, r)
  + feature_envelope(q, r)
  + biome_detail_noise(q, r)
  + post_processing(q, r)
```

The spatial structure (which boundary, distance to it) is static and cached per chunk. Only the intensity lookup changes, and it's a cheap table read. When boundary intensity changes, affected chunks are invalidated and regenerated — but this happens infrequently (event-driven) and only for chunks near the affected boundary segment.

---

## Coordinate Space

All coordinates use i32 for horizontal (q, r) and vertical (z) axes. The i32 z-axis provides effectively unlimited vertical range. The surface band occupies a small fraction of the available space:

| Zone | Approximate Z Range | Purpose |
|------|-------------------|---------|
| Deep underground | -16,000 to 0 | Future cave systems, excavation |
| Sea level baseline | ~500 | Reference point for ocean/coastal features |
| Surface band | 0 to 4,000 | All normal terrain: plains, mountains, rifts |
| High atmosphere | 4,000 to 10,000 | Future sky biomes, floating islands |
| Space | 10,000+ | Future orbital/void content |

The continental base layer varies within roughly 200–1,200. Mountain peak features can push to 3,000–4,000. This gives mountains 2,000+ units of genuine ascent above surrounding terrain — enough to feel like a real climb over hundreds of tiles — while leaving the vast majority of the z-range available for future vertical content.

---

## Invariants

**1. Deterministic generation**: `tile_height(q, r, seed, boundary_state) → z` is a pure function. Same inputs always produce the same output. Required for chunk cache consistency per ADR-001.

**2. Local evaluation**: No tile's height depends on distant tiles. The lookup radius is bounded by grid cell sizes (plate grid, feature grid). A tile only needs to check nearby grid cells — never the full world.

**3. Tectonic structure is static**: Plate centers, Voronoi edges, drift vectors, and base elevations are determined entirely by the world seed and never change. Only boundary intensities evolve, and they are explicit persisted state, not derived from generation.

**4. Feature isolation**: One feature per placement cell. Features in adjacent cells may overlap envelopes but cannot share a heart location. Minimum feature spacing equals cell size minus maximum jitter.

**5. Biome follows structure**: Biome assignment is derived from feature influence and tectonic context, never assigned independently. Biome cannot contradict its geological source (no mountain biome in a rift depression).

---

## Open Design Questions

**Mountain approach paths**: Should preferred approach directions be explicitly designed per mountain feature (hash-determined "gentle side"), or should they emerge purely from ridge noise geometry?

**Biome blending width**: How many tiles of transition between biome detail noise profiles? Too narrow creates visible seams. Too wide creates zones that feel like neither biome.

**Plains heart features**: The spec suggests tors, sinkholes, craters, mesas — should these be sub-types with distinct envelopes, or a single "plains anomaly" with parameterized geometry?

**Vertical biome layering**: Mountains could transition through biome zones at different elevations (forest → alpine → snow). Is this a biome system concern or a terrain generation concern?

**River systems**: Should rivers be explicit features placed along divergent boundaries and drainage paths, or emergent from terrain geometry? Explicit placement is simpler and more reliable. Emergent requires flow simulation.

## Tuning Parameters

These values are expected to change during development. They become locked once world persistence is implemented (saved chunk state, player builds, hub positions tied to terrain).

| Parameter | Value | Notes |
|-----------|-------|-------|
| Continental grid cell size | 4,000 tiles | |
| Regional grid cell size | 1,200 tiles | Hierarchical within continental |
| Continental jitter | 30% fixed | |
| Regional jitter range | 15–45% noise-driven | |
| Regional skip range | 0–80% | Inversely correlated with jitter |
| Continental warp amplitude | ~600 tiles | Curl noise |
| Regional warp amplitude | ~150 tiles | Curl noise |
| Continental warp noise scale | ~1/3,000 | |
| Regional warp noise scale | ~1/800 | |
| Drift noise scale | ~1/5,000 | Regional coherence |
| Continental elevation | 400–1,200 | |
| Oceanic elevation | 50–200 | |
| Continental ratio | 60% | Hash threshold, no jitter bias |
| Continental convergent | 1,500 tiles max, +800 peak | |
| Continental divergent | 600 tiles max, -300 peak | |
| Continental transform | 300 tiles max, ±40 noise | |
| Regional convergent | 400 tiles max, +100 peak | |
| Regional divergent | 200 tiles max, -80 peak | |
| Regional transform | 150 tiles max, ±15 noise | |
| Base elevation blend width | 300 tiles | Linear blend at continental boundaries |
| Boundary intensity noise | ~1/1,000 (cont), ~1/300 (reg) | Per-tile variation |
| Transform threshold | 0.15 | Deadzone for per-pair classification |

---

## Chunk Streaming & Level of Detail

### Problem

Standing on a cliff, the camera sees far toward lower terrain but barely past the cliff wall uphill. The existing adaptive loading (per-chunk `visibility_radius`) already produces an asymmetric shape that extends toward valleys and retracts toward ridges. However, every discovered chunk transmits full 64-tile data (~2.6KB) and inserts 64 entries into the client's Map regardless of distance. At outer-ring distances, tiles are sub-pixel — meshes are built for geometry nobody can distinguish, and the Map grows with entries that physics, movement, and pathfinding never touch.

### Two-Ring Architecture

Chunk loading splits into two concentric rings separated by `FOV_CHUNK_RADIUS`:

**Inner ring** (Chebyshev distance ≤ `FOV_CHUNK_RADIUS`): Full 64-tile chunks. Gameplay happens here — physics, pathfinding, combat, and all tile-level interactions use these tiles from the Map. This ring is symmetric and always loaded.

**Outer ring** (`FOV_CHUNK_RADIUS` < distance ≤ max_radius): Each chunk is summarized as a single 7-vertex hex. One Map entry instead of 64. ~12 bytes network instead of ~2.6KB. The outer ring's shape is asymmetric — extends toward valleys, stays tight toward ridges — using the existing `visibility_radius` per-chunk filtering.

The boundary between rings is `FOV_CHUNK_RADIUS`, which doubles as the minimum gameplay-safe loading distance. Chunks transitioning between rings upgrade (summary → full detail) or downgrade (full detail → summary) without terrain vanishing.

### ChunkSummary

Each outer-ring chunk is represented by a lightweight summary:

- **chunk_id**: Which chunk
- **elevation**: Average elevation across the chunk's 64 tiles
- **biome**: Dominant terrain type (most common EntityType)

The server computes summaries from full chunk data (average elevation, most common EntityType) or generates them cheaply from terrain noise at the chunk center point if the full chunk hasn't been generated yet.

### Summary Mesh Rendering

Summary hexes render through the existing per-chunk mesh pipeline with simpler geometry:

- Center vertex at the chunk's world position, y = chunk average elevation
- 6 corner vertices, each shared by 3 chunks — y = average of those 3 chunks' elevations
- This produces a continuous terrain surface: mountain ridges slope naturally between adjacent summaries, valleys dip, the landscape reads as coherent from distance
- 6 triangles (center to each edge), single color from biome
- If any neighbor chunk's summary hasn't arrived, skip rendering that hex entirely — retry next pass (deferred rendering for missing neighbors)

### Ring Transitions

When a player moves and chunks change rings:

- **Entering inner from outer**: Server sends full `ChunkData` (upgrade)
- **Entering outer from nothing**: Server sends `ChunkSummary`
- **Leaving inner to outer**: Server sends `ChunkSummary` (downgrade — terrain doesn't vanish)
- **Leaving outer entirely**: Client evicts summary

### Client Storage

Summaries are stored in a separate `ChunkSummaries` resource, not in the tile Map. This prevents summary entries from colliding with real tiles during physics, movement, or pathfinding. On receiving full `ChunkData`, the corresponding summary is removed. On receiving a `ChunkSummary`, any existing full tiles for that chunk are removed from the Map.

### Cache on Boundary Crossing

The adaptive visibility set is only recomputed when the player crosses a chunk boundary, not on every `Loc` change. `do_incremental` fires ~3×/sec/player — at 200 players with radius-12 worst cases, that's 375K visibility checks/sec if recomputed every tick. Caching on boundary crossing reduces this to ~0.5 recomputations/sec/player (average boundary crossing rate). The `VisibleChunkCache` component stores both inner and outer ring sets plus eviction mirrors.

### Eviction

Client eviction runs in two passes:

1. **Full-detail chunks**: Chebyshev distance > `FOV_CHUNK_RADIUS + 1` → evict from Map
2. **Summary chunks**: Per-chunk `visibility_radius(player_z, elevation, 40.0) + 1` → evict from `ChunkSummaries`

Server eviction mirrors this logic using `chunk_max_z` for worst-case guarantees.

### Flyover

Flyover mode uses the same two-ring architecture: inner ring gets full tiles for immediate terrain, outer ring gets summaries filling the horizon. The flyover-specific `half_viewport` (20.0 × camera scale) flows through the same `visibility_radius` function.

### Invariants

**6. Ring separation**: Physics, movement, and pathfinding only read from the Map (inner ring tiles), never from `ChunkSummaries`. Summary data is rendering-only.

**7. Continuous surface**: Adjacent summary hexes share corner vertices via neighbor elevation averaging. No floating platforms or gaps between summaries.

**8. No terrain vanishing**: Ring downgrade (inner → outer) sends a summary before the client evicts full tiles. The chunk is never absent from both representations simultaneously.

### Future Work

- **Camera-terrain collision**: Pass actual camera distance as `half_viewport` instead of fixed 40.0. Loading shape tightens dynamically. Function signature already supports this.
- **Additional LoD tiers**: Intermediate tiers (e.g. 4×4 sample grid) slot between inner and outer using the same ring architecture.
- **Summary generation shortcut**: Generate summaries directly from terrain noise at chunk center without computing full 64 tiles. Saves server-side generation cost for chunks that may never enter the inner ring.

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Height generation | Full pipeline: plates + features + biome noise + post-processing | Phase 1: dual-scale tectonic plates + boundary elevation only (no noise, features, or biomes) | Incremental implementation; plates are the structural foundation |
| 2 | Terrain API | `tile_height(q, r)` as pure function of tile coords | `terrain::Terrain::get_height(q, r)` in standalone `crates/terrain/` library; server wraps with Bevy Resource | Enables `terrain-viewer` CLI without server dependency; hex coords are natural input |
| 3 | Feature placement | Grid-hashed feature seeds with distance-field envelopes | Not yet implemented | Phase 3 |
| 4 | Biome system | Biome derived from tectonic context + feature influence | Not yet implemented | Phase 4 |
| 5 | Dynamic tectonics | Evolving boundary intensities with event thresholds | Static terrain generation only | Event system not yet implemented |
| 6 | Stratification | Per-biome post-processing (mountain only) | Not yet implemented | Phase 4 |
| 7 | Base terrain noise | Two sub-layers (continental texture + micro-texture) | Not yet implemented | Phase 2 — plate interiors are currently flat |
| 8 | Chunk streaming | Two-ring LoD: inner full-detail, outer summary hexes | Client-side two-ring implemented: `ChunkSummaries` resource, summary mesh rendering, two-pass eviction, flyover two-tier. Server-side `Event::ChunkSummary` not yet implemented — summaries generated client-side from evicted tile data. | See ADR-032 |

## Implementation Gaps

**Next (Phase 2)**: Base terrain noise layers (continental texture ~1/800 ±100, micro-texture ~1/30 ±3)

**High (Phase 3)**: Feature placement grid and envelope evaluation, mountain feature type with ridge noise and peak geometry

**High (Phase 4)**: Biome assignment from feature/tectonic context, per-biome detail noise, biome transition blending, stratification as biome parameter

**Medium**: Swamp and plains feature types, rift/canyon features

**Medium (LoD server-side)**: Server-side two-ring streaming — `Event::ChunkSummary` network message, `VisibleChunkCache` inner/outer fields, `do_incremental` ring transition diff logic (enter inner, enter outer, inner↔outer upgrade/downgrade, leave outer), server eviction mirrors. Client-side LoD is complete: `ChunkSummaries` resource, async summary mesh rendering, two-pass eviction, flyover two-tier with async tile generation. See ADR-032.

**Deferred**: Dynamic boundary intensity evolution, tectonic event triggers (earthquakes, eruptions), vertical biome layering on mountains, river systems, cave/underground generation

**Blocked by other systems**: Event system (for tectonic pressure release), water rendering (for ocean plates and swamp features), haven placement validation (ensuring havens sit on stable plate interiors)

---

**Related Design Documents:**
- [Haven System](haven.md) — Starter havens placed on stable plate interiors; biome types (Mountain/Prairie/Forest)
- [Hub System](hubs.md) — Player settlements whose placement interacts with terrain traversability
- [Siege System](siege.md) — Encroachment system where terrain difficulty compounds with distance

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based caching; deterministic generation invariant
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Two-ring LoD: inner full-detail, outer summary hexes
