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

Domain-warped inverted-F1 Voronoi on a jittered hex lattice. Creates disconnected continental blobs.

| Parameter | Value | Notes |
|-----------|-------|-------|
| CONTINENT_CELL_SIZE | 25,000 wu | Continental-scale cells |
| CONTINENT_JITTER | 0.35 | Seed scatter within cells |
| CONTINENT_WARP_AMPLITUDE | 4,000 wu | Domain warp for irregular shapes |
| CONTINENT_WARP_WAVELENGTH | 8,000 wu | Domain warp frequency |
| WORLD_GATE_SIGMOID_MIDPOINT | 0.35 | Gate activation point |
| WORLD_GATE_SIGMOID_STEEPNESS | 12.0 | Gate sharpness |

### Local fBm (Triple-Prime Octaves)

Three simplex octaves at prime wavelengths (B=25013, C=11003, D=4999) with weights 1.0/0.5/0.5, normalized by 2.0. Creates coastline irregularity.

### Regional Modulator

Low-frequency simplex remapped to [0.1, 1.15]. Ensures every world gets land but continent sizes vary.

### Output

`regime_value_at(wx, wy, seed)` returns [0, 1] after sigmoid. Values below `REGIME_LAND_THRESHOLD` (0.15) are water; above are land.

**Implementation:** `crates/terrain/src/plates.rs` (`regime_value_at`, `cellular_world_gate`)

---

## Layer 2: Two-Level Voronoi Skeleton

### Micro Cells (Primary Spatial Layer)

Micro cells tile the world uniformly. Each cell is a small hexagonal Voronoi region. Assignment is pure Euclidean distance — no macro dependency at generation time.

| Parameter | Value | Notes |
|-----------|-------|-------|
| MICRO_CELL_SIZE | 450 wu | Sub-grid hex lattice spacing |
| MICRO_JITTER_WAVELENGTH | 5,000 wu | Simplex jitter frequency |
| MICRO_SUPPRESSION_RATE | 0.0 | Flat everywhere (tunable) |

**Implementation:** `crates/terrain/src/microplates.rs` (`micro_cell_at`)

### Macro Plates (Geological Identity)

Macro plates are labels assigned to micro cells via **anisotropic warped distance**. Coastal plates stretch along the shore; interior plates stay equidimensional.

| Parameter | Value | Notes |
|-----------|-------|-------|
| MACRO_CELL_SIZE | 1,800 wu | Hex-lattice seed spacing |
| SUPPRESSION_RATE_MIN | 0.05 | At coastlines (many small plates) |
| SUPPRESSION_RATE_MAX | 0.70 | Deep inland/water (large calm plates) |
| WARP_STRENGTH_MAX | 600 wu | Per-candidate noise amplitude |
| MAX_ELONGATION | 8.0 | Max coastal plate stretch ratio |

**Anisotropic assignment:** `AnisoContext` compresses the along-coast axis of the distance metric based on regime gradient magnitude. High gradient (coastlines) → irregular, elongated plates. Low gradient (interiors) → regular convex plates. Expanded search (2 + MAX_ELONGATION rings).

**Implementation:** `crates/terrain/src/plates.rs` (`warped_plate_at`, `AnisoContext`)

### Orphan Correction

Bottom-up assignment can create disconnected fragments within a plate. `fix_orphans` runs connected-component analysis and reassigns minority fragments to surrounding majority plates. Small isolated plates (≤ STRANDED_ISLAND_MAX_SIZE=8 cells) are also reassigned.

Correction uses a margin-based approach (ORPHAN_CORRECTION_MARGIN=15,000 wu) to ensure sufficient context. Chunk authority invariant: only core chunks are marked corrected; margin chunks provide context only.

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

Locally deterministic via fixed-size evaluation chunks (SPINE_CHUNK_SIZE = 2 × SPINE_EXCLUSION_DIST). Epicenter candidates: Inland plates with all-Inland neighbors. Priority-ordered greedy exclusion ensures same placement regardless of viewport.

### Peaks

`Peak { wx, wy, height, falloff_radius }` — isotropic circular cones with power-curve falloff. Two arms grow laterally from each epicenter with curvature, width noise, and coastal attenuation.

| Parameter | Value | Notes |
|-----------|-------|-------|
| RIDGE_PEAK_ELEVATION | 4,000 wu | Maximum peak height |
| ELEVATION_PER_Z | 1.0 | World units per z-level |

### Ridgelines

Explicit segments connecting nearby peaks with quadratic sag and lateral wobble. Each peak connects to ≤4 nearest neighbors within MAX_RIDGE_DIST_SCALE (2.5) × avg falloff radius.

| Parameter | Value | Notes |
|-----------|-------|-------|
| RIDGE_HALF_WIDTH | 1,200 wu | Perpendicular influence |
| RIDGE_FALLOFF_EXPONENT | 1.2 | Power-curve steepness |
| RIDGE_SAG_MIN / MAX | 0.1 / 0.6 | Saddle depth fraction |
| RIDGE_LATERAL_WOBBLE | 150 wu | Meander amplitude |

### Cross-Section Tags

Distance from spine centerline determines sub-tags:
- **Ridge** (0–15%): Spine crest
- **Highland** (15–60%): Elevated plateau
- **Foothills** (60–100%): Transitional slopes

### Ravine Network

Sequential top-down stream generation. Origins distributed along ridgelines with jitter, sorted by elevation (highest first). Each stream grows step-by-step following terrain slope.

**Growth termination:** Stall detection (STALL_WINDOW=5 steps, MIN_DEPTH_GROWTH=0.5), sea level, spine territory radius, or ascending surface.

**Stream merging:** Streams check a hex-indexed spatial grid (HexSpatialGrid, 7-cell lookup) for nearby existing streams. On proximity hit (STREAM_PROXIMITY_RADIUS=150 wu), stream takes a final step onto the existing stream's centerline and stops. `propagate_merge_counts` walks the tree in reverse to compound merge counts.

**Depth model:** Slope-integrated accumulation (`cum_depth += base × slope_factor × merge_boost × step_size`). Depth grows naturally on steep terrain and stalls on flats.

**Carving:** Min-composited subtractive distance field with V-shaped cross-section. Wall exponent evolves from young (0.5) to mature (1.5) with merges. Order-independent (min compositing).

**Ridge paths:** Traversable crossings at ravine rims. Currently disabled (PATH_PROBABILITY=0.0).

**Implementation:** `crates/terrain/src/spine.rs` (`generate_spines`, `SpineInstance::elevation_at`, `RavineNetwork::carve`)

### Spine Caching

`SpineCache` provides lazy per-chunk generation with LRU eviction (SPINE_CACHE_MAX_CHUNKS=32). `elevation_at(wx, wy, plate_cache)` resolves the point's chunk + 1-ring (7 lookups). Evicted chunks regenerate deterministically on revisit.

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
warp_strength_at(wx, wy, seed) -> f64         // [0, 600]

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

**Inner ring** (Chebyshev distance <= `FOV_CHUNK_RADIUS`): Full 64-tile chunks. Gameplay happens here — physics, pathfinding, combat use these tiles from the Map.

**Outer ring** (beyond, up to max_radius): Each chunk summarized as a single 7-vertex hex. One Map entry instead of 64. ~12 bytes network instead of ~2.6KB.

### Invariants

**Ring separation**: Physics, movement, and pathfinding only read from the Map (inner ring), never from ChunkSummaries.

**Continuous surface**: Adjacent summary hexes share corner vertices. No gaps.

**No terrain vanishing**: Ring downgrade sends summary before client evicts full tiles.

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Chunk streaming | Two-ring LoD with server-side streaming | Client-side two-ring implemented; server-side Event::ChunkSummary not yet | See ADR-032 |
| 2 | Elevation | Full biome-aware height | Spine-only elevation (peaks + ridgelines - ravines) | Biome system not yet built |

## Implementation Gaps

**Current**: Biome system — classify terrain into biomes (forest, desert, plains) based on plate tags + elevation + moisture

**Medium (LoD server-side)**: Server-side two-ring streaming — Event::ChunkSummary, VisibleChunkCache, do_incremental ring transitions

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
