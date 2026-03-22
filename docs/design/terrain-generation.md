# Terrain Generation

## Design Philosophy

Terrain is generated through the [World Event System](world-events.md) — a framework where events evaluate in dependency order over a flat hex substrate, each materializing per-tile output (tags, elevation, entity placements) into a composite that all layers above can read. This document covers the algorithm details for the currently implemented events.

Every value is procedurally evaluable per-tile from `(position, seed)`, satisfying ADR-001's deterministic generation invariant.

## Player Experience Goals

- **Readable geology**: Coastlines, mountain ranges, and valleys are visually distinct
- **Continental scale**: Large landmasses with irregular coastlines and interior mountain chains
- **Smooth transitions**: No hard edges between geological provinces
- **Deterministic**: Same seed always produces the same world
- **Streamable**: Any region evaluable on demand without global precomputation

---

## Plates (Event #0)

The first event in the pipeline. Uses `Survey::none()` — plates discover centroids internally via lattice iteration over cell bounds, not through tile enumeration. Produces tagged plates at two scales (macro and micro) with geological classification. Registers the `plate_centroids` index for use by higher events.

### Regime Field

The regime field determines where land and water exist. It multiplies three independent noise factors:

```
regime = sigmoid(local_fBm × world_gate × regional_mod)
```

**World Gate (Cellular):** Domain-warped inverted-F1 Voronoi on a jittered hex lattice. Creates disconnected continental blobs. Constants control cell size, jitter, warp amplitude/wavelength, and sigmoid activation.

**Local fBm (Triple-Prime Octaves):** Multi-octave simplex at prime wavelengths to avoid harmonic alignment. Creates coastline irregularity.

**Regional Modulator:** Low-frequency simplex remapped to a narrow band. Ensures every world gets land but continent sizes vary.

**Output:** `regime_value_at(wx, wy, seed)` returns [0, 1] after sigmoid. Values below the land threshold are water; above are land.

**Implementation:** `crates/world/src/plates.rs` (`regime_value_at`, `cellular_world_gate`)

### Two-Level Voronoi Skeleton

**Micro Cells (Primary Spatial Layer):** Micro cells tile the world uniformly. Each cell is a small hexagonal Voronoi region. Assignment is pure Euclidean distance — no macro dependency at generation time. Constants control lattice spacing, jitter frequency, and suppression rate.

**Implementation:** `crates/world/src/microplates.rs` (`micro_cell_at`)

**Macro Plates (Geological Identity):** Macro plates are labels assigned to micro cells via **anisotropic warped distance**. Coastal plates stretch along the shore; interior plates stay equidimensional. Constants control lattice spacing, suppression rate range, warp strength, and maximum elongation.

**Anisotropic assignment:** `AnisoContext` compresses the along-coast axis of the distance metric based on regime gradient magnitude. High gradient (coastlines) → irregular, elongated plates. Low gradient (interiors) → regular convex plates.

**Implementation:** `crates/world/src/plates.rs` (`warped_plate_at`, `AnisoContext`)

**Orphan Correction:** Bottom-up assignment can create disconnected fragments within a plate. `fix_orphans` runs connected-component analysis and reassigns minority fragments to surrounding majority plates. Small isolated plates below a size threshold are also reassigned. Correction uses a margin-based approach to ensure sufficient context. Chunk authority invariant: only core chunks are marked corrected; margin chunks provide context only.

**Implementation:** `crates/world/src/microplates.rs` (`MicroplateCache::populate_region`, `fix_orphans`)

### Classification

Plates are tagged with geological roles after Voronoi assignment:

| Tag | Meaning | Assignment Rule |
|-----|---------|-----------------|
| Sea | Open water | Regime < REGIME_LAND_THRESHOLD, no land neighbors |
| Coast | Shoreline | Warp > COASTAL_WARP_THRESHOLD OR has cross-regime neighbor |
| Inland | Interior land | Land regime, all neighbors also land |

Both macro `PlateCenter.tags` and micro `MicroplateCenter.tags` carry tags. Macro tags assigned by `PlateCache::classify_tags`; micro tags auto-populated by `populate_region` via `classify_micro_tags`.

**Implementation:** `crates/common/src/plate_tags.rs` (PlateTag enum, Tagged trait), `crates/world/src/plates.rs` (classify_tags)

---

## Continental Spines (Event #1)

Reads Inland plate tags from the composite, writes elevation deltas + Ridge/Highland/Foothills tags. Surveys at centroid granularity via the `plate_centroids` index.

### Spine Placement

Epicenter candidates: Inland plates with all-Inland neighbors, filtered via `Survey::from_index` with `min_spacing(10_000)`. Deterministic priority-ordered greedy exclusion at the survey level ensures same placement regardless of viewport or evaluation order.

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

**Implementation:** `crates/world/src/spine.rs` (`generate_spines`, `SpineInstance::elevation_at`, `RavineNetwork::carve`)

### Spine Caching

`SpineCache` provides lazy per-chunk generation with LRU eviction. `elevation_at(wx, wy, plate_cache)` resolves the point's chunk + 1-ring. Evicted chunks regenerate deterministically on revisit. Dead code on the server path (server uses `SpineInstanceIndex` via Composite); retained for client admin flyover and world-viewer.

---

## Elevation Pipeline

Final elevation at any point:

```
raw_elevation = max(peak contributions, ridgeline contributions) + micro_noise - ravine_carving
get_height(q, r) = discretize(raw_elevation)    // quantized to z-levels
```

`SpineInstance::elevation_at(wx, wy)` computes the full chain: bounding check → max over peaks and ridgelines → ridge noise → ravine subtraction. `discretize_elevation` quantizes to integer z-levels.

Outside spine influence, elevation is 0 (sea level). The elevation system produces mountains and valleys; biomes and water rendering are not yet implemented.

After event system migration, elevation becomes a flat read from the materialized composite rather than per-query geometry evaluation. See [World Event System — Materialization](world-events.md#materialization).

---

## Public API

```rust
// Core terrain
Terrain::new(seed) -> Terrain
Terrain::get_height(q, r) -> i32              // Discretized elevation
Terrain::get_raw_elevation(q, r) -> f64       // Pre-discretization
Terrain::plate_info_at(q, r) -> (PlateCenter, MicroplateCenter)
Terrain::tags_at(q, r) -> ArrayVec<[PlateTag; 2]>  // Base tag + optional spine tag
Terrain::spawners_near(q, r, radius) -> ...        // Query spawner cache for nearby spawner positions

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

The `world-viewer` crate renders terrain layers to PNG for development diagnostics:

```bash
cargo run -p world-viewer -- --layers plates,elevation --radius 200 --output terrain.png
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

### Chunk Loading (ADR-032)

Server sends full-detail chunks within `terrain_chunk_radius(max_z) + 1` — a simple hex radius based on the player's chunk elevation. All chunks are inserted into the server's Map and sent as full 271-tile `ChunkData` events.

### LoD Rendering

See **[lod.md](lod.md)** — hex-native decimation with continuous distance-driven thresholds. Client owns LoD within ~20 chunks; server generates and sends `DecimatedChunkData` beyond that.

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Elevation | Full biome-aware height | Spine-only elevation (peaks + ridgelines - ravines) | Biome system not yet built |

## Implementation Gaps

**Complete**: All 3 events migrated to deform/query split — PlateEvent (scale=1800), SpineEvent (scale=SPINE_INFLUENCE/15,225), SpawnerEvent (scale=9/271 tiles, min_spacing=50). Server queries route through EventRegistry → Composite. SpawnerCache, SpineCache, and old Terrain methods are dead code on server path (retained for client admin flyover + world-viewer). See [world-events.md](world-events.md) for framework spec and remaining gaps.

**Current**: Biome system — classify terrain into biomes (forest, desert, plains) based on composite tags + elevation + moisture

**Deferred**: Feature envelopes (swamps, plateaus), river systems beyond ravines, cave/underground generation — future events in the pipeline

**Blocked by other systems**: Water rendering (ocean/coast), haven placement validation

---

**Related Design Documents:**
- [World Event System](world-events.md) — Framework contract, deform/query split, composite materialization, index system
- [Level of Detail](lod.md) — Hex-native decimation, threshold model, client/server rendering regimes
- [Haven System](haven.md) — Starter havens; biome types
- [Hub System](hubs.md) — Player settlements interacting with terrain
- [Siege System](siege.md) — Encroachment where terrain difficulty compounds with distance

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based caching; deterministic generation
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Chunk loading; QEM rendering superseded by hex-native LoD
