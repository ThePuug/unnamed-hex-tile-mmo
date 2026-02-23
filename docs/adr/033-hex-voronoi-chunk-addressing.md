# ADR-033: Hex Voronoi Chunk Addressing

## Status

Accepted - 2026-02-23

## Context

The terrain crate uses chunk caching to amortize noise evaluation costs. Both the hotspot layer (grid cell activity) and thermal layer (source gather) partition the world into chunks and precompute per-chunk data. Each tile lookup then gathers from the center chunk plus its immediate neighbors.

The original implementation used **parallelogram (independent Q/R division)** to assign tiles to chunks:

```rust
fn tile_to_chunk(q: i32, r: i32) -> (i32, i32) {
    (div_floor(q, CHUNK_SPACING), div_floor(r, CHUNK_SPACING))
}
```

This divides each axis independently, producing parallelogram-shaped regions. The problem: parallelogram chunks have 6 edge neighbors but also 2 corner neighbors (along the diagonal). A simple 6-neighbor hex gather misses the corner directions entirely. Sources in those diagonal gaps can contribute non-negligibly to tiles in the center chunk, violating the boundary invariant (< 1% error from missed sources).

The alternative — gathering 8 neighbors (6 hex + 2 corner) — adds complexity and still leaves irregular gaps depending on the parallelogram's aspect ratio.

### Options Considered

#### Option 1: 8-Neighbor Gather (Keep Parallelogram Chunks)

- **Pros:** Minimal code change, just add 2 more neighbor offsets
- **Cons:** 8/9 chunks gathered instead of 6/7 (29% more work). Parallelogram regions have non-uniform neighbor distances — corner neighbors are further than edge neighbors, making worst-case distance bounds harder to reason about. The gather radius varies by direction.

#### Option 2: Hex Voronoi Chunks (Cube-Coordinate Rounding)

- **Pros:** Hexagonal regions have 6 equidistant neighbors with no diagonal gaps. 7-chunk gather is both necessary and sufficient. Worst-case distance to ring-2 is uniform in all directions. Reasoning about boundary invariants is straightforward.
- **Cons:** Requires cube-coordinate rounding math (hex_round). Chunk boundaries aren't axis-aligned, making iteration slightly more complex.

#### Option 3: Larger Chunks (Keep Parallelogram, Increase Size)

- **Pros:** Simple — just make chunks big enough that diagonal gaps don't matter
- **Cons:** Wastes memory and computation. Doesn't fix the architectural mismatch between hex-based terrain and rectangular chunks. Boundary invariant becomes even harder to verify.

## Decision

**Hex Voronoi chunk addressing via cube-coordinate rounding.** Each tile is assigned to its nearest chunk center in hex distance using `hex_round(q/spacing, r/spacing)`.

### Rationale

The terrain system is fundamentally hex-based — hotspot cells sit on a hex grid, Gaussian sources radiate isotropically, and the game world uses hexagonal tiles. Rectangular chunk addressing creates an impedance mismatch that requires workarounds (extra neighbors, larger chunks, direction-dependent bounds).

Hex Voronoi chunks align the caching topology with the data's natural geometry:
- **6 equidistant neighbors** → uniform gather radius in all directions
- **No diagonal gaps** → 7-chunk gather is provably complete
- **Clean boundary proofs** → minimum ring-2 distance is `chunk_size * √3/2`, uniform in all directions
- **Shared utility** → both hotspot and thermal caches use the same `tile_to_hex_chunk` function

The cube-coordinate rounding is a well-known algorithm (4 multiplies, 3 rounds, a few comparisons) with no performance impact.

## Consequences

### Positive

**Uniform boundary invariant**: The `missed_sources_beyond_neighborhood_are_negligible` test can use a single minimum distance for all directions. No special-casing for diagonals.

**Reduced gather work**: 7 chunks (center + 6) instead of 9 (center + 8) per query. 22% fewer chunks to compute and scan.

**Shared implementation**: `tile_to_hex_chunk(q, r, spacing)` and `hex_round(fq, fr)` live in `lib.rs` and serve both hotspot chunks (spacing=1500) and thermal chunks (spacing=4500).

**Future-proof**: Any new hex-based caching layer can reuse the same addressing.

### Negative

**Non-axis-aligned boundaries**: Iterating "all grid cells in a chunk" requires scanning a bounding box and filtering by `grid_cell_to_chunk(gq, gr) == (cq, cr)`. Slightly more complex than a rectangular range.

**Cube-round math**: `hex_round` converts to cube coordinates, rounds, and corrects the largest error. Not complex, but less obvious than `div_floor`.

## Key Files

| File | Changes |
|------|---------|
| `crates/terrain/src/lib.rs` | Added `hex_round()`, `tile_to_hex_chunk()` shared utilities |
| `crates/terrain/src/hotspots.rs` | `tile_to_chunk` → `tile_to_hex_chunk(q, r, CHUNK_SPACING)`, removed CHUNK_RADIUS offset from `chunk_center_grid_cell` |
| `crates/terrain/src/thermal.rs` | `tile_to_thermal_chunk` → `tile_to_hex_chunk(q, r, THERMAL_CHUNK_SIZE)`, added `grid_cell_to_chunk` for source ownership, updated `compute_chunk` iteration |

## Related ADRs

- [ADR-001](001-chunk-based-world-partitioning.md) — Chunk-based world partitioning (gameplay chunks)

## Date

2026-02-23
