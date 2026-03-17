# Terrain Generation

## Design Philosophy

The world is built from **tectonic plates** — large-scale geological provinces that define continents, coastlines, and mountain ranges. Terrain generation uses a **two-level Voronoi skeleton** where micro cells tile the world and macro plates label them, followed by **continental spine events** that raise elevation along inland mountain chains. Every value is procedurally evaluable per-tile from `(position, seed)`, satisfying ADR-001's deterministic generation invariant.

The pipeline flows bottom-up:
1. **Regime field** — multiplicative noise defines where land and water exist
2. **Two-level Voronoi** — micro cells provide spatial tiling; macro plates provide geological identity
3. **Classification** — plates tagged as Sea/Coast/Inland based on regime values
4. **Continental spines** — mountain chains with peaks, ridgelines, and ravine networks carve elevation

## Player Experience Goals

- **Readable geology**: Coastlines, mountain ranges, and valleys are visually distinct
- **Continental scale**: Large landmasses with irregular coastlines and interior mountain chains
- **Smooth transitions**: No hard edges between geological provinces
- **Deterministic**: Same seed always produces the same world
- **Streamable**: Any region evaluable on demand without global precomputation

---

## Layer 1: Regime Field

The regime field determines where land and water exist. It multiplies three independent noise factors:

```
regime = sigmoid(local_fBm × world_gate × regional_mod)
```

### World Gate (Cellular)

Domain-warped inverted-F1 Voronoi on a jittered hex lattice. Creates disconnected continental blobs. Constants control cell size, jitter, warp amplitude/wavelength, and sigmoid activation.

### Local fBm (Triple-Prime Octaves)

Multi-octave simplex at prime wavelengths to avoid harmonic alignment. Creates coastline irregularity.

### Regional Modulator

Low-frequency simplex remapped to a narrow band. Ensures every world gets land but continent sizes vary.

### Output

`regime_value_at(wx, wy, seed)` returns [0, 1] after sigmoid. Values below the land threshold are water; above are land.

**Implementation:** `crates/terrain/src/plates.rs` (`regime_value_at`, `cellular_world_gate`)

---

## Layer 2: Two-Level Voronoi Skeleton

### Micro Cells (Primary Spatial Layer)

Micro cells tile the world uniformly. Each cell is a small hexagonal Voronoi region. Assignment is pure Euclidean distance — no macro dependency at generation time. Constants control lattice spacing, jitter frequency, and suppression rate.

**Implementation:** `crates/terrain/src/microplates.rs` (`micro_cell_at`)

### Macro Plates (Geological Identity)

Macro plates are labels assigned to micro cells via **anisotropic warped distance**. Coastal plates stretch along the shore; interior plates stay equidimensional. Constants control lattice spacing, suppression rate range, warp strength, and maximum elongation.

**Anisotropic assignment:** `AnisoContext` compresses the along-coast axis of the distance metric based on regime gradient magnitude. High gradient (coastlines) → irregular, elongated plates. Low gradient (interiors) → regular convex plates.

**Implementation:** `crates/terrain/src/plates.rs` (`warped_plate_at`, `AnisoContext`)

### Orphan Correction

Bottom-up assignment can create disconnected fragments within a plate. `fix_orphans` runs connected-component analysis and reassigns minority fragments to surrounding majority plates. Small isolated plates below a size threshold are also reassigned.

Correction uses a margin-based approach to ensure sufficient context. Chunk authority invariant: only core chunks are marked corrected; margin chunks provide context only.

**Implementation:** `crates/terrain/src/microplates.rs` (`MicroplateCache::populate_region`, `fix_orphans`)

---

## Layer 3: Classification

Plates are tagged with geological roles after Voronoi assignment:

| Tag | Meaning | Assignment Rule |
|-----|---------|-----------------|
| Sea | Open water | Regime < REGIME_LAND_THRESHOLD, no land neighbors |
| Coast | Shoreline | Warp > COASTAL_WARP_THRESHOLD OR has cross-regime neighbor |
| Inland | Interior land | Land regime, all neighbors also land |

Both macro `PlateCenter.tags` and micro `MicroplateCenter.tags` carry tags. Macro tags assigned by `PlateCache::classify_tags`; micro tags auto-populated by `populate_region` via `classify_micro_tags`.

**Implementation:** `crates/common/src/plate_tags.rs` (PlateTag enum, Tagged trait), `crates/terrain/src/plates.rs` (classify_tags)

---

## Layer 4: Continental Spines

Mountain chains form along inland plate interiors. The spine system generates peaks, connects them with ridgelines, and carves ravine networks.

### Spine Placement

Locally deterministic via fixed-size evaluation chunks. Epicenter candidates: Inland plates with all-Inland neighbors. Priority-ordered greedy exclusion ensures same placement regardless of viewport.

### Peaks

`Peak { wx, wy, height, falloff_radius }` — isotropic circular cones with power-curve falloff. Two arms grow laterally from each epicenter with curvature, width noise, and coastal attenuation.

### Ridgelines

Explicit segments connecting nearby peaks with quadratic sag and lateral wobble. Each peak connects to a limited number of nearest neighbors within a distance threshold scaled by average falloff radius. Constants control perpendicular influence width, falloff steepness, saddle depth range, and meander amplitude.

### Cross-Section Tags

Distance from spine centerline determines sub-tags:
- **Ridge** (0–15%): Spine crest
- **Highland** (15–60%): Elevated plateau
- **Foothills** (60–100%): Transitional slopes

### Ravine Network

Sequential top-down stream generation. Origins distributed along ridgelines with jitter, sorted by elevation (highest first). Each stream grows step-by-step following terrain slope.

**Growth termination:** Stall detection (insufficient depth growth over a rolling window), sea level, spine territory radius, or ascending surface.

**Stream merging:** Streams check a hex-indexed spatial grid (HexSpatialGrid, 7-cell lookup) for nearby existing streams. On proximity hit, stream takes a final step onto the existing stream's centerline and stops. `propagate_merge_counts` walks the tree in reverse to compound merge counts.

**Depth model:** Slope-integrated accumulation. Depth grows naturally on steep terrain and stalls on flats. Merge count boosts depth downstream.

**Carving:** Min-composited subtractive distance field with V-shaped cross-section. Wall exponent evolves from young to mature with merges. Order-independent (min compositing).

**Ridge paths:** Traversable crossings at ravine rims. Currently disabled.

**Implementation:** `crates/terrain/src/spine.rs` (`generate_spines`, `SpineInstance::elevation_at`, `RavineNetwork::carve`)

### Spine Caching

`SpineCache` provides lazy per-chunk generation with LRU eviction. `elevation_at(wx, wy, plate_cache)` resolves the point's chunk + 1-ring. Evicted chunks regenerate deterministically on revisit.

---

## Elevation Pipeline

Final elevation at any point:

```
raw_elevation = max(peak contributions, ridgeline contributions) + micro_noise - ravine_carving
get_height(q, r) = discretize(raw_elevation)    // quantized to z-levels
```

`SpineInstance::elevation_at(wx, wy)` computes the full chain: bounding check → max over peaks and ridgelines → ridge noise → ravine subtraction. `discretize_elevation` quantizes to integer z-levels.

Outside spine influence, elevation is 0 (sea level). The elevation system produces mountains and valleys; biomes and water rendering are not yet implemented.

---

## Public API

```rust
// Core terrain
Terrain::new(seed) -> Terrain
Terrain::get_height(q, r) -> i32              // Discretized elevation
Terrain::get_raw_elevation(q, r) -> f64       // Pre-discretization
Terrain::plate_info_at(q, r) -> (PlateCenter, MicroplateCenter)

// Batch generation (viewer/server)
generate_region(seed, cx, cy, radius, with_spines) -> RegionResult

// Regime field
regime_value_at(wx, wy, seed) -> f64          // [0, 1]
warp_strength_at(wx, wy, seed) -> f64

// Coordinate conversion
hex_to_world(q, r) -> (f64, f64)
```

---

## Visualization

The `terrain-viewer` crate renders terrain layers to PNG for development diagnostics:

```bash
cargo run -p terrain-viewer -- --layers plates,elevation --radius 200 --output terrain.png
```

| Layer | What It Shows |
|-------|---------------|
| `plates` | Sea/Coast/Inland coloring with per-plate hue variation |
| `regime` | Raw regime grayscale + red contour at land threshold |
| `elevation` | Height tinting with slope-gradient cliff shading |
| `spines` | Ridge (white) / Highland (orange) / Foothills (tan) |
| `centroids` | Macro (red) and micro (yellow) center markers |
| `ravines` | Floor (blue) / Wall (red) / Path (yellow) debug overlay |

Layers are composited bottom-to-top. Macro borders drawn as light grey lines; micro borders as subtle brightening.

---

## Chunk Streaming & Level of Detail

### Two-Ring Architecture (ADR-032)

Chunk loading splits into two concentric rings separated by `FOV_CHUNK_RADIUS`:

**Inner ring** (Chebyshev distance <= `FOV_CHUNK_RADIUS`): Full-detail chunks. Gameplay happens here — physics, pathfinding, combat use these tiles from the Map.

**Outer ring** (beyond, up to max_radius): Each chunk decimated via QEM (Quadric Error Metrics) into variable boundary vertices (6–54) + N interior vertices selected by terrain variance. Mesh reconstructed client-side via Delaunay triangulation.

**Boundary vertices**: 6 corner vertices (3-tile average, always retained) + up to 8 edge vertices per edge, RDP-decimated against `BORDER_ERROR_THRESHOLD`. Flat edges produce 0 extra vertices; complex edges retain tiles proportional to terrain variance. Adjacent chunks share boundary vertices deterministically. Interior vertices carry relative (q, r) + elevation.

### Invariants

**Ring separation**: Physics, movement, and pathfinding only read from the Map (inner ring), never from ChunkSummaries.

**Continuous surface**: Adjacent summary meshes share deterministic boundary vertices (corners + RDP-decimated edges). No gaps.

**No terrain vanishing**: Ring downgrade sends summary before client evicts full tiles.

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Elevation | Full biome-aware height | Spine-only elevation (peaks + ridgelines - ravines) | Biome system not yet built |

## Implementation Gaps

**Current**: Biome system — classify terrain into biomes (forest, desert, plains) based on plate tags + elevation + moisture

**Deferred**: Feature envelopes (swamps, plateaus), river systems beyond ravines, cave/underground generation, dynamic temporal events

**Blocked by other systems**: Water rendering (ocean/coast), haven placement validation

---

**Related Design Documents:**
- [Haven System](haven.md) — Starter havens; biome types
- [Hub System](hubs.md) — Player settlements interacting with terrain
- [Siege System](siege.md) — Encroachment where terrain difficulty compounds with distance

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based caching; deterministic generation
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Two-ring LoD: inner full-detail, outer summary hexes
