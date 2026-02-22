# Terrain Generation

## Design Philosophy

The world should feel like a place with real geography, not a noise field with textures. Terrain generation uses a **weighted influence field model** where every tile computes elevation from the smooth, continuous, weighted influence of all nearby tectonic plates. There is no discrete plate membership in the elevation function, no boundaries as objects, no profile catalog. Terrain features emerge naturally from tectonic stress between competing plate influences.

This produces terrain with identifiable continents, organic coastlines, mountain ranges at convergent zones, rift valleys at divergent zones, and broken ground at shear zones — all from a single unified model.

## Player Experience Goals

- **Continental structure**: Large landmasses with clear identity, separated by ocean basins
- **Readable geology**: Players can learn to read terrain — mountain ranges mean plate collision, rift valleys mean plates pulling apart
- **Traversal variety**: No monotonous ramps; terrain character changes as you cross plate boundaries
- **Coastal variety**: Coastlines range from cliffs to beaches to rocky shores depending on tectonic context
- **Interior texture**: Plate interiors have gentle variation, not dead flat
- **Dramatic boundaries**: Where plates meet, terrain is visually interesting and geologically motivated

## Generation Pipeline

Height at any tile is evaluated as a pure function of tile coordinates and world seed. No global precomputation, no startup pass — every layer is procedurally evaluable per-tile, satisfying ADR-001's deterministic generation invariant.

```
tile_height(q, r) =
    blended_base_elevation(q, r)           // weighted sum of nearby plate base elevations
  + compression_contribution(q, r)         // smooth uplift/depression from converging/diverging plates
  + shear_contribution(q, r)               // noise-driven disruption from sliding plates
  + broad_undulation(q, r)                 // continental texture (~1/800 scale)
  + micro_texture(q, r)                    // per-tile variation (~1/30 scale)
```

Each layer depends only on tile coordinates and the world seed. Chunk caching means each tile is evaluated once and stored immutably per ADR-001.

---

## Layer 1: Tectonic Plates

### Plate Placement

Plates are defined by a Voronoi tessellation over procedurally placed plate centers. Centers live on a coarse grid (~4,000 tiles per cell). Each cell's center position is determined by hashing `(cell_q, cell_r, seed)` to produce:

* **Position offset** (jitter within cell): fixed fraction of cell size
* **Base elevation**: range 25–600, producing a mix of low ocean plates and high continental plates
* **Drift vector**: direction and magnitude of plate movement (Vec2), used for stress computation

### Domain Warping (Curl Noise)

Voronoi lookup coordinates are domain-warped using curl noise, producing organic boundary shapes instead of geometric polygons. Curl noise uses a divergence-free vector field derived from a single scalar noise field, eliminating cusp artifacts that occur with independent-axis warping.

Implementation:
1. Evaluate a scalar noise field at the tile's Cartesian position
2. Compute finite-difference partial derivatives (dn/dx, dn/dy)
3. Rotate 90 degrees to get displacement: warp_x = dn/dy, warp_y = -dn/dx
4. Add displacement to tile coordinates before Voronoi lookup

Parameters:
* **Warp amplitude**: ~600 tiles
* **Noise scale**: ~1/3,000

---

## Layer 2: Weighted Influence Field

For each tile, elevation is computed from the weighted influence of ALL nearby plates, not just the nearest one. This is the core innovation — there are no discrete boundaries, only smooth transitions.

### Weight Function

Each plate's influence on a tile uses exponential decay:

```
weight_i = exp(-warped_distance_i / sigma)
```

Where `sigma` ~ plate_grid / 3 (~1,300 tiles). Weights are normalized so they sum to 1.0.

This gives:
- 95%+ weight for the owning plate at half-radius from center
- Smooth transitions over ~500-800 tiles at boundaries
- The blended zone (15-20% of plate radius) IS the coastline/boundary

### Blended Base Elevation

```
blended_elevation = sum(weight_i * base_elevation_i) for all significant plates
```

At world scale, continents are clearly defined — 80%+ of a plate's area is at 95%+ that plate's base elevation. The blended edges produce natural coastlines and transitions.

---

## Layer 3: Tectonic Stress Fields

Where multiple plates have significant weight, their competing drift vectors create tectonic stress. Stress is computed as pairwise interactions between all plates with non-trivial influence.

### Contest Factor

For each plate pair (i, j):
```
contest = weight_i * weight_j
```

This is only significant near shared boundaries where both plates have meaningful weight. Deep in a plate's interior, contest approaches zero.

### Compression

How much the plates push toward or pull away from each other along the line connecting their centers:

```
pair_direction = normalize(center_j - center_i)
relative_drift = drift_i - drift_j
compression = dot(relative_drift, pair_direction) * contest
```

- **Positive compression**: plates converging → uplift (mountains)
- **Negative compression**: plates diverging → depression (rifts)

### Shear

How much the plates slide past each other:

```
shear = |cross(relative_drift, pair_direction)| * contest
```

Shear drives terrain disruption — broken, chaotic ground at transform boundaries.

### Accumulation

Total compression and shear at a tile are summed across all significant plate pairs.

---

## Layer 4: Terrain from Stress

### Compression → Elevation

Compression drives smooth uplift or depression using a soft nonlinear curve:

```
compression_elevation = sign(compression) * |compression|^0.7 * amplitude
```

The 0.7 exponent ensures moderate compression is visible without extreme compression producing absurd peaks. Amplitude in the range of 100-300 units — comparable to base elevation differences so features are visible.

### Shear → Disruption

Shear drives noise amplitude:

```
shear_noise = noise(tile_position, shear_seed) * shear_magnitude * amplitude
```

With 2-3 octaves at ~1/80 tile wavelength. The noise includes an elevation-bridging bias — trending from one base elevation toward the other across the shear zone — so the noise bridges elevation gaps rather than sitting on top of a cliff.

### Scaling

Both compression and shear effects scale with relative drift magnitude (faster plates = more dramatic terrain).

---

## Layer 5: Noise Layers

Two additive noise layers prevent flat interiors and add surface texture. These are simple Perlin noise with no plate interaction.

**Broad undulation** (~1/800 scale, +/-100 units): Makes plate interiors feel like varied terrain rather than flat shelves. A player walking thousands of tiles across a stable plate experiences gradual elevation changes.

**Micro-texture** (~1/30 scale, +/-3 units): Subtle per-tile variation preventing uncanny smoothness. Almost imperceptible in isolation but prevents flat areas from looking artificially generated.

---

## Coordinate Space

All coordinates use i32 for horizontal (q, r) and vertical (z) axes.

| Zone | Approximate Z Range | Purpose |
|------|-------------------|---------|
| Deep underground | -16,000 to 0 | Future cave systems |
| Sea level baseline | ~200 | Reference for ocean/coast |
| Surface band | 0 to 1,000 | Normal terrain |
| High atmosphere | 1,000+ | Future content |

Base plate elevations range 25-600. Compression can add 100-300 units. Total surface range roughly 0-1,000.

---

## Desired Outcomes

1. Continental structure: distinct landmasses with clear identity
2. Ocean basins: low-elevation plates clearly distinguishable from high
3. Mountain ranges at convergent plate boundaries
4. Rift valleys at divergent plate boundaries
5. Broken/chaotic terrain at transform (shear) boundaries
6. Smooth transitions between plate interiors (no hard edges)
7. Organic plate shapes (curl noise warp, not polygons)
8. Coastlines with varied character (cliffs, gradual slopes)
9. No monotonous ramps across large distances
10. Plate interiors with gentle texture (not dead flat)
11. No unintentional cliffs (bright lines in slope mode)
12. Geologically motivated worst-case elevation transitions
13. Deterministic: same seed always produces same terrain
14. Per-tile evaluable: no global precomputation required

---

## Tuning Parameters

These values are expected to change during development.

| Parameter | Value | Notes |
|-----------|-------|-------|
| Plate grid cell size | ~4,000 tiles | |
| Plate jitter | ~30% of cell size | |
| Base elevation range | 25–600 | |
| Curl warp amplitude | ~600 tiles | |
| Curl warp noise scale | ~1/3,000 | |
| Influence sigma | ~1,300 (grid/3) | Exponential decay |
| Compression exponent | 0.7 | Soft nonlinear curve |
| Compression amplitude | 100–300 | |
| Shear noise frequency | ~1/80 tiles | 2-3 octaves |
| Shear amplitude | 100–300 | |
| Broad undulation scale | ~1/800 | +/-100 units |
| Micro-texture scale | ~1/30 | +/-3 units |

---

## Chunk Streaming & Level of Detail

### Two-Ring Architecture (ADR-032)

Chunk loading splits into two concentric rings separated by `FOV_CHUNK_RADIUS`:

**Inner ring** (Chebyshev distance <= `FOV_CHUNK_RADIUS`): Full 64-tile chunks. Gameplay happens here — physics, pathfinding, combat use these tiles from the Map.

**Outer ring** (beyond, up to max_radius): Each chunk summarized as a single 7-vertex hex. One Map entry instead of 64. ~12 bytes network instead of ~2.6KB. Shape is asymmetric — extends toward valleys, stays tight toward ridges.

### Summary Mesh Rendering

Summary hexes render through the existing per-chunk mesh pipeline:
- Center vertex at chunk world position, y = chunk average elevation
- 6 corner vertices shared by 3 chunks, y = average of those 3 chunks' elevations
- Continuous terrain surface at distance
- Deferred if neighbor summaries missing

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

## Implementation Gaps

**Current (Phase 1)**: Full terrain generation rewrite — weighted influence field model replacing boundary deformation system

**Medium (LoD server-side)**: Server-side two-ring streaming — Event::ChunkSummary, VisibleChunkCache, do_incremental ring transitions

**Deferred**: Dynamic boundary intensity evolution, tectonic events, feature envelopes (mountains, swamps, plains), biome system, river systems, cave/underground generation

**Blocked by other systems**: Water rendering (ocean/coast), haven placement validation

---

**Related Design Documents:**
- [Haven System](haven.md) — Starter havens; biome types
- [Hub System](hubs.md) — Player settlements interacting with terrain
- [Siege System](siege.md) — Encroachment where terrain difficulty compounds with distance

**Related ADRs:**
- [ADR-001](../adr/001-chunk-based-world-partitioning.md) — Chunk-based caching; deterministic generation
- [ADR-032](../adr/032-two-ring-lod-chunk-loading.md) — Two-ring LoD: inner full-detail, outer summary hexes
