# Terrain Generation

## Design Philosophy

The world should feel alive — heat boils up through dense primordial material, breaking through at margins and erupting in identifiable plumes. Terrain generation uses a **three-layer pipeline** where each layer is a pure function of tile coordinates, world seed, and world tick. No global precomputation, no simulation state — every value is procedurally evaluable per-tile, satisfying ADR-001's deterministic generation invariant.

1. **Material distribution** — large-scale geological provinces (dense vs light)
2. **Hotspot convection** — sub-lid cellular structure where dense material traps heat
3. **Thermal diffusion** — additive Gaussian heat field radiating from active sources

Temperature is the primary terrain signal. Dense regions trap heat from convection cells below; thin margins let it through. The result: identifiable hot plumes at province margins, cooler interiors, and quiescent light regions.

## Player Experience Goals

- **Readable geology**: Dense provinces are hot, light regions are cool — players learn to read the landscape
- **Identifiable plumes**: Thermal breakthroughs at province margins create distinct warm zones
- **Temporal variation**: Hotspot lifecycle creates slow pulses of activity (rise → peak → collapse)
- **Smooth transitions**: Gaussian diffusion produces soft thermal gradients, no hard edges
- **Province variety**: Material density varies at continental scale — no two regions feel identical
- **Deterministic**: Same seed + tick always produces the same temperature at any tile

---

## Layer 1: Material Distribution

Material density defines the geological provinces of the world. Dense regions (above threshold) support sub-lid convection; light regions are quiescent.

### Simplex Noise Field

Material density is computed from three overlapping simplex noise waves at near-golden-ratio wavelength spacing (~1.618x). The incommensurate wavelengths ensure the sum never repeats, producing unique structure everywhere without any single wavelength being enormous.

**Implementation:** `crates/terrain/src/material.rs`

### Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| Wave 1 wavelength | 12,547 tiles | ~2x hotspot cell size — regional variation |
| Wave 1 amplitude | 1.0 | Reference amplitude |
| Wave 2 wavelength | 20,297 tiles | ~3.4x cell size — provincial character |
| Wave 2 amplitude | 0.7 | |
| Wave 3 wavelength | 32,833 tiles | ~5.5x cell size — broad structure |
| Wave 3 amplitude | 0.5 | |
| MATERIAL_AMPLITUDE | 0.8 | How much density varies from midpoint (0.0–0.5) |
| MATERIAL_CONTRAST | 0.7 | Power curve exponent for province separation |

### Output

`material_density(q, r)` returns a value in [0.0, 1.0]:
- **< 0.55** (HOTSPOT_THRESHOLD): Light region — no convection cells, no thermal sources
- **>= 0.55**: Dense region — supports hotspot activity, lid thickness = density - threshold

The contrast curve (`|x|^0.7 * sign(x)`) sharpens the boundary between dense and light provinces while keeping transitions smooth at the tile level.

---

## Layer 2: Hotspot Convection

Dense material acts as a lid trapping heat from below. Convection cells on a fixed grid represent sub-lid activity — rising plumes of heat that cycle through birth, peak, and collapse.

### Grid Structure

Hotspot cells sit on a fixed hexagonal grid at `HOTSPOT_GRID_SPACING` intervals. Not all grid cells are active — only those where the material density at their center exceeds `HOTSPOT_THRESHOLD`. This means:

- Dense province interiors: nearly all grid cells active → continuous cellular texture
- Province margins: sparse active cells → isolated hotspots at the boundary
- Light regions: no active cells → quiescent

### Lifecycle

Each active cell cycles through an asymmetric lifecycle:
- **Rise** (0–60% of cycle): Slow quadratic ramp (`t²`)
- **Peak** (60–70%): Full intensity plateau
- **Collapse** (70–100%): Fast quadratic decay (`(1-t)²`)

Phase offsets are deterministic per-cell (hashed from cell coordinates and seed), so neighboring cells peak at different times. The asymmetry means heat builds slowly and dissipates quickly — a geological "breathe in slowly, exhale fast" rhythm.

### Nearest-Cell Lookup

For the sub-lid diagnostic view (`Terrain::hotspot_temperature`), each tile finds its nearest active grid cell via brute-force ±2 search, then combines radial falloff with lifecycle vigor. This produces the raw cellular structure visible in the "hotspot" terrain viewer mode.

### Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| HOTSPOT_THRESHOLD | 0.55 | Minimum material density for activity |
| HOTSPOT_GRID_SPACING | 750 tiles | Fixed grid interval |
| HOTSPOT_CYCLE_TICKS | 1,000 | One full lifecycle in world ticks |

**Implementation:** `crates/terrain/src/hotspots.rs`

---

## Layer 3: Thermal Diffusion

Active hotspot cells become point sources of heat. Each source radiates outward as an additive Gaussian, and the sum of all nearby sources produces the surface temperature field.

### Source Intensity (Focal Point Drainage)

Every active hotspot cell (density >= threshold) produces energy that drains to a focal point — a local minimum in lid thickness. Multiple cells converge on the same focal point, accumulating energy into a single source.

**Phase 1 — Find focal point:** Walk the lid gradient from each active cell toward thinner lid (steepest descent on lid thickness). Stop at: local minimum, lid=0 (margin edge), or MAX_DRAIN_STEPS cap.

**Phase 2 — Accumulate energy:** Each cell's energy = `penetration × lifecycle`, where `penetration = exp(-lid × LID_SUPPRESSION)` at the cell's own lid thickness. Energy decays during migration: `migration_loss = exp(-dist × MIGRATION_DECAY)` where dist = hex steps × grid spacing.

**Phase 3 — Emit sources:** Each focal point with accumulated energy above noise floor becomes a single ThermalSource. `intensity = accumulated_energy × MAX_SOURCE_INTENSITY`.

```
lid         = density - HOTSPOT_THRESHOLD
penetration = exp(-lid × LID_SUPPRESSION)      // at origin cell
energy      = penetration × lifecycle
focal       = find_focal_point(gq, gr, seed)    // gradient descent
dist        = hex_steps(origin, focal) × GRID_SPACING
migration_loss = exp(-dist × MIGRATION_DECAY)
focal.accumulated += energy × migration_loss
...
intensity = focal.accumulated × MAX_SOURCE_INTENSITY
```

- **Convergence**: 30 margin cells → 3-5 focal point sources, not 30 individual sources
- **Structural determination**: Focal points are lid minima — concavities, narrow isthmuses, density peaks along margins
- **Natural gaps**: Between focal points, cells drain away. No sources exist between them.
- **Interior secondaries**: Local density peaks deep inside create shallow lid minima. Surrounding cells drain to them — weak but structurally real.

Sources below `NOISE_FLOOR` (0.001) are filtered for performance.

### Gaussian Diffusion

Each source radiates heat via:

```
contribution = intensity * exp(-dist² / 2σ²)
```

Sources overlap additively. The sum is clamped to [0, 1]. A 3σ cutoff skips negligible contributions.

With σ=1,200 tiles, a single strong source (intensity ~0.12) creates a warm spot ~7,200 tiles in diameter. Clusters of margin sources produce brighter, more identifiable plumes.

### Boundary Bleed

Gaussian tails carry heat past the dense/light boundary. Tiles in light regions near a dense margin can have non-zero temperature from nearby sources. This is intentional — it creates a soft warm halo around dense provinces rather than a hard temperature cliff.

### Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| THERMAL_SIGMA | 600 tiles | Gaussian spread; 3σ = 1,800 tile radius |
| MAX_SOURCE_INTENSITY | 0.12 | Cap per source; cluster of ~5 reaches 0.3–0.5 |
| LID_SUPPRESSION | 8.0 | Exponential decay rate for lid thickness |
| NOISE_FLOOR | 0.001 | Sources below this filtered out |
| MIGRATION_DECAY | 0.0001 | Energy loss per world unit of drainage travel |
| MAX_DRAIN_STEPS | 6 | Maximum grid steps for focal point walk |

**Implementation:** `crates/terrain/src/thermal.rs`

---

## Chunk Caching

Both hotspot and thermal layers use hex-Voronoi chunk caching to amortize noise evaluation costs across many tile lookups. See [ADR-033](../adr/033-hex-voronoi-chunk-addressing.md) for the addressing decision.

### Hex Voronoi Addressing

Chunks are assigned via cube-coordinate rounding — each tile maps to its nearest chunk center in hex distance. This produces hexagonal regions where 6 neighbors fully surround each chunk with no diagonal gaps.

```
tile_to_hex_chunk(q, r, spacing):
    hex_round(q / spacing, r / spacing)
```

### Hotspot Chunk Cache

Precomputes which grid cells are active (density >= threshold) for chunks in a 1-ring neighborhood (center + 6 neighbors). Used by `Terrain::hotspot_temperature_cached` for the sub-lid diagnostic view.

| Parameter | Value |
|-----------|-------|
| CHUNK_RADIUS | 750 tiles |
| CHUNK_SPACING | 1,500 tiles |

### Thermal Chunk Cache

Precomputes thermal sources (position + intensity) per chunk. Gathers sources from the center chunk + 6 hex neighbors for each query. The 1-ring of chunks at `THERMAL_CHUNK_SIZE >= 3σ` ensures ring-2 sources contribute < 1% (verified by test).

| Parameter | Value |
|-----------|-------|
| THERMAL_CHUNK_SIZE | 4,500 tiles |
| GRID_CELLS_PER_CHUNK | 6 |

### Boundary Invariant

The `missed_sources_beyond_neighborhood_are_negligible` test bounds worst-case contribution from ring-2+ sources. With focal point drainage (MAX_DRAIN_STEPS=6), sources can walk up to 4500 world units from their origin. The conservative upper bound is < 60% (ignoring penetration attenuation of deep-interior cells); realistic error is well under 5%. Cross-chunk focal point convergence happens naturally through Gaussian superposition — two sources at the same position from different chunks sum identically to one combined source.

---

## Coordinate Space

All terrain functions operate in hex tile coordinates (q, r). Cartesian conversion for noise evaluation:

```
hex_to_world(q, r) → (x, y)   // q + r*0.5, r*√3/2
```

Temperature is a `f64` in [0.0, 1.0]. Material density is a `f64` in [0.0, 1.0].

Height is currently a placeholder (`get_height` returns 0) — the elevation system will be rebuilt on top of material + thermal layers.

---

## Public API

```rust
// Core evaluation
Terrain::new(seed) → Terrain
Terrain::with_tick(seed, world_tick) → Terrain
Terrain::material_density(q, r) → f64       // [0, 1]
Terrain::temperature(q, r) → f64            // [0, 1] (uncached)
Terrain::temperature_cached(q, r, cache) → f64  // [0, 1] (cached)
Terrain::hotspot_temperature(q, r) → f64    // Sub-lid diagnostic
Terrain::evaluate(q, r) → TerrainEval       // { height: 0, temperature }

// Caching
ThermalChunkCache::new(seed, world_tick)
HotspotChunkCache::new(seed)

// Chunk addressing
tile_to_chunk(q, r) → (chunk_q, chunk_r)           // Hotspot chunks
tile_to_thermal_chunk(q, r) → (chunk_q, chunk_r)   // Thermal chunks
```

---

## Visualization

The `terrain-viewer` crate renders terrain layers to PNG for development diagnostics:

| Mode | What It Shows |
|------|---------------|
| Material | Material density field — bright = dense, dark = light |
| Hotspots | Sub-lid cellular structure — radial falloff × lifecycle |
| Thermal | Surface temperature field — additive Gaussian plumes |

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
| 2 | Height | Height derives from material + thermal | `get_height` returns 0 (placeholder) | Elevation system rebuild pending |

## Implementation Gaps

**Current**: Height/elevation system — rebuild from material density + thermal field to produce terrain features

**Medium (LoD server-side)**: Server-side two-ring streaming — Event::ChunkSummary, VisibleChunkCache, do_incremental ring transitions

**Deferred**: Biome system, feature envelopes (mountains, swamps, plains), river systems, cave/underground generation, dynamic temporal events

**Blocked by other systems**: Water rendering (ocean/coast), haven placement validation

---

**Related Design Documents:**
- [Haven System](haven.md) — Starter havens; biome types
- [Hub System](hubs.md) — Player settlements interacting with terrain
- [Siege System](siege.md) — Encroachment where terrain difficulty compounds with distance

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based caching; deterministic generation
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Two-ring LoD: inner full-detail, outer summary hexes
- [ADR-033](../adr/033-hex-voronoi-chunk-addressing.md) — Hex Voronoi chunk addressing via cube-coordinate rounding
