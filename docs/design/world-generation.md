# Terrain Generation

## Design Philosophy

The world should feel like a place with real geography, not a noise field with textures. Terrain generation is built on the principle that **structure comes from geology, detail comes from noise**. Continental-scale features like mountain ranges, rift valleys, and highland plateaus emerge from procedural tectonic simulation, while local variation comes from layered noise parameterized by the geological context.

This architecture serves the hostile world thesis: the world's terrain is not static decoration but a living system that builds pressure and breaks. Tectonic boundaries are seams of geological instability where earthquakes, eruptions, and terrain-reshaping events concentrate. Players experience a world that is actively geologically hostile — and that hostility has visible, comprehensible structure they can learn to read.

## Generation Pipeline

Height at any tile is evaluated as a pure function of tile coordinates and world state. No global precomputation, no startup pass — every layer is procedurally evaluable per-tile, satisfying ADR-001's deterministic generation invariant.

```
tile_height(q, r) =
    plate_base_elevation(q, r)          // which plate, its base height
  + boundary_contribution(q, r)         // proximity to plate boundaries
  + feature_envelope(q, r)              // nearby geographic features (mountains, swamps, etc.)
  + biome_detail_noise(q, r)            // biome-specific surface texture
  + post_processing(q, r)               // stratification, channel carving, smoothing
```

Each layer depends only on tile coordinates, the world seed, and (for the dynamic layer) a small set of evolving boundary intensity parameters. Chunk caching means each tile is evaluated once and stored immutably per ADR-001.

---

## Layer 1: Tectonic Plates (Continental Structure)

### Plate Placement

Plates are defined by a Voronoi tessellation over procedurally placed plate centers. Centers live on a coarse grid (5,000 × 5,000 tiles per cell). Each cell's center position is determined by hashing `(cell_q, cell_r, seed)` to produce:

* **Position offset** (jitter within cell): 0–40% of cell size
* **Plate character**: continental or oceanic, biased by regional context (stable regions favor continental, chaotic regions favor oceanic)
* **Base elevation**: bimodal distribution driven by plate character — continental plates sit in the 400–1,200 range, oceanic plates sit in the 50–200 range. The ~200–400 gap between them is where coastlines naturally form.
* **Drift vector**: direction and magnitude of plate movement (used for boundary classification)

The continental/oceanic ratio should be approximately 60/40, producing significant coastline without making the world feel mostly submerged. This ratio is influenced by the jitter field: stable (low-jitter) regions favor continental plates, producing large landmasses. Chaotic (high-jitter) regions favor oceanic plates, producing island chains and fragmented coastlines between small plates.

Coastlines emerge naturally at Voronoi boundaries between continental and oceanic plates — a continental plate at base elevation 500 adjacent to an oceanic plate at 150 creates a slope that reads as shoreline. This produces distinct coastal terrain (rocky shores, tidal flats, coastal cliffs) without requiring a dedicated coastline feature type or water rendering. The boundary classification still applies: a convergent continental-oceanic boundary creates coastal mountain ranges (subduction zones), while a divergent one creates broad shallow coastal shelves.

For any tile, determining plate membership requires checking the surrounding grid cells (typically 9), computing distance to each plate center, and selecting the nearest — standard Voronoi evaluation.

### Variable Plate Size

Plate center jitter is not uniform. A very-large-scale noise field (scale ~1/50,000) controls jitter intensity regionally:

* **Low jitter regions**: Centers stay near cell centers, producing large stable plates with broad interiors. These are geologically quiet zones — good locations for havens and player settlement.
* **High jitter regions**: Centers cluster irregularly, producing small fractured plates with dense boundary networks. These are tectonically chaotic zones — broken terrain, frequent ridges, and concentrated geological event potential.

This creates a world where tectonic chaos is regionally clustered rather than uniformly distributed. Some parts of the world are geologically stable. Others are shattered. The jitter noise field is the coarsest, cheapest evaluation in the entire pipeline — it changes imperceptibly over thousands of tiles.

### Boundary Classification

At any tile near a Voronoi edge (the boundary between two plates), the relationship between the two plates' drift vectors determines the boundary type. The key input is the dot product of each plate's drift vector with the boundary normal:

**Convergent** (plates pushing toward each other): Both drift vectors have positive components toward the boundary. This produces uplift — mountain ranges, highland ridges, elevated terrain along the boundary line. The elevation contribution scales with convergence intensity (how directly the plates collide). Strong head-on convergence creates major ranges. Oblique convergence creates lower, broader uplift.

**Divergent** (plates pulling apart): Both drift vectors have components away from the boundary. This produces subsidence — rift valleys, depressions, low points along the boundary. At extreme intensity, divergent boundaries become candidates for flooding or magma events.

**Transform** (plates sliding past each other): Drift vectors are roughly parallel to the boundary. This produces moderate terrain disruption — fault lines, broken terrain, but without the dramatic elevation changes of convergent or divergent boundaries. These create interesting traversal challenges without major altitude shifts.

### Boundary Elevation Contribution

The boundary's effect on tile height follows a falloff curve from the Voronoi edge:

```
boundary_contribution = intensity × envelope(distance_to_boundary)
```

The envelope tapers from maximum effect at the boundary to zero at a type-dependent distance:

* **Major convergent**: up to 2,000–3,000 tiles of influence, producing broad mountain range foothill zones
* **Minor convergent/divergent**: 500–1,500 tiles of influence
* **Transform**: 200–800 tiles, localized disruption

Convergent boundaries add elevation (positive contribution). Divergent boundaries subtract it (negative contribution). Transform boundaries add noise amplitude rather than elevation bias.

### Drift Vector Generation

Drift vectors need to produce a good mix of boundary types across the world. If all plates drift the same direction, most boundaries are transform (boring). The hash that generates drift vectors uses a rotational component — plates tend to drift with a regional rotational bias, creating natural convergence zones where rotation fields oppose each other.

This means some regions of the world have mostly convergent boundaries (mountain-heavy), some have mostly divergent (rift-heavy), and some have a healthy mix. The distribution emerges from the drift field rather than being manually tuned.

---

## Layer 2: Base Terrain (Ambient Geography)

On top of the tectonic structure, three noise sub-layers provide the surface variation that makes terrain feel natural rather than mathematical:

**Continental texture** (scale ~1/2,000, ±100 units): Broad undulation within plate interiors. This is what makes crossing a large stable plate feel like traversing varied terrain rather than a flat shelf. A player walking for thousands of tiles across a single plate's interior should experience gradual elevation changes from this layer.

**Regional texture** (scale ~1/500, ±40 units): Rolling hills, shallow valleys, the terrain you'd describe as "gently hilly." This layer provides the medium-scale variation that's visible from any given position — the hills on the horizon, the shallow depression ahead.

**Micro-texture** (scale ~1/80, ±3 units): Subtle per-tile variation preventing uncanny smoothness. Almost imperceptible in isolation but prevents flat areas from looking artificially generated.

These layers are simple Perlin noise evaluations with no conditional logic. They provide texture, not structure. The tectonic layer provides structure.

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

**Static** (determined by seed, never changes): Plate centers, Voronoi structure, drift vectors, base elevations, jitter field. This is the world's geological skeleton.

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
    plate_base_elevation(q, r)
  + boundary_contribution(q, r, boundary_state)   // intensity from dynamic state
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

**Feature grid cell size** (starting: 500 × 500 tiles): Controls feature density and minimum spacing. Interacts with plate grid size — features should be meaningfully smaller than plates so multiple features exist within a single plate interior. Adjust based on visual results and gameplay feel.

**Plate grid cell size** (starting: 5,000 × 5,000 tiles): Controls plate density and boundary frequency. Smaller cells produce more boundaries and more chaotic terrain.

**Continental/oceanic ratio** (starting: 60/40): Controls how much of the world is landmass vs sea floor. Influenced by jitter field bias.

**Base elevation ranges** (starting: continental 400–1,200, oceanic 50–200): The gap between ranges defines coastline width. Wider gap = more dramatic coastal transitions.

**Noise amplitudes and scales**: All noise layer parameters (continental texture, regional texture, micro-texture, biome detail) are tuning values. Starting points are documented in their respective layer sections.

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Height generation | Full pipeline: plates + features + biome noise + post-processing | Phase 1: tectonic plates + boundary elevation only (no noise, features, or biomes) | Incremental implementation; plates are the structural foundation |
| 2 | Terrain API | `tile_height(q, r)` as pure function of tile coords | `terrain::Terrain::get_height(q, r)` in standalone `crates/terrain/` library; server wraps with Bevy Resource | Enables `terrain-viewer` CLI without server dependency; hex coords are natural input |
| 3 | Base elevation blend | Not explicitly specified | Linear blend over 1,500 tiles at plate boundaries to prevent cliff-height discontinuities | Walkability invariant: adjacent tile height difference must be ≤ 1 for traversal |
| 4 | Feature placement | Grid-hashed feature seeds with distance-field envelopes | Not yet implemented | Phase 3 |
| 5 | Biome system | Biome derived from tectonic context + feature influence | Not yet implemented | Phase 4 |
| 6 | Dynamic tectonics | Evolving boundary intensities with event thresholds | Static terrain generation only | Event system not yet implemented |
| 7 | Stratification | Per-biome post-processing (mountain only) | Not yet implemented | Phase 4 |
| 8 | Base terrain noise | Three sub-layers (continental/regional/micro) | Not yet implemented | Phase 2 — plate interiors are currently flat |

### Phase 1 Tuning Parameter Values

| Parameter | Value | Notes |
|-----------|-------|-------|
| Plate grid cell size | 5,000 tiles | |
| Max jitter fraction | 40% | |
| Jitter noise scale | 1/50,000 | Very large scale, coherent over ~10 plate cells |
| Drift regional scale | 1/25,000 | Coherent over ~5 plate cells |
| Continental elevation | 400–1,200 | |
| Oceanic elevation | 50–200 | |
| Continental ratio | 60% (+ 15% bias in stable regions) | |
| Convergent max distance | 2,500 tiles | Peak elevation +800 |
| Divergent max distance | 1,000 tiles | Peak elevation -300 |
| Transform max distance | 500 tiles | Noise amplitude ±40 |
| Base blend width | 1,500 tiles | Ensures max gradient < 1/tile |

## Implementation Gaps

**Next (Phase 2)**: Base terrain noise layers (continental ~1/2,000 ±100, regional ~1/500 ±40, micro ~1/80 ±3)

**High (Phase 3)**: Feature placement grid and envelope evaluation, mountain feature type with ridge noise and peak geometry

**High (Phase 4)**: Biome assignment from feature/tectonic context, per-biome detail noise, biome transition blending, stratification as biome parameter

**Medium**: Swamp and plains feature types, rift/canyon features

**Deferred**: Dynamic boundary intensity evolution, tectonic event triggers (earthquakes, eruptions), vertical biome layering on mountains, river systems, cave/underground generation

**Blocked by other systems**: Event system (for tectonic pressure release), water rendering (for ocean plates and swamp features), haven placement validation (ensuring havens sit on stable plate interiors)

---

**Related Design Documents:**
- [Haven System](haven.md) — Starter havens placed on stable plate interiors; biome types (Mountain/Prairie/Forest)
- [Hub System](hubs.md) — Player settlements whose placement interacts with terrain traversability
- [Siege System](siege.md) — Encroachment system where terrain difficulty compounds with distance

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based caching; deterministic generation invariant