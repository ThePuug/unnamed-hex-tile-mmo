# ADR-001: Chunk-Based World Partitioning

## Status

Accepted - 2025-10-28

## Context

**Related RFC:** [RFC-001: Chunk-Based Terrain Discovery](../01-rfc/001-chunk-based-terrain-discovery.md)

We need to partition a 100 km² hex-tile world (~160 million tiles) for efficient discovery, caching, and network transmission in a multiplayer environment with hub clustering (50-200 players per area).

The fundamental architectural question: **What is the right spatial granularity for world partitioning?**

### Options Considered

#### Option 1: Tile-Level Granularity (Status Quo)
- **Grain size:** 1 tile
- **Operations:** Discovery, caching, network transmission at single-tile level
- **Pros:** Maximum precision, simple addressing
- **Cons:** Extremely chatty (121 operations per player movement), no batching opportunity, no spatial locality

#### Option 2: Chunk-Based Partitioning
- **Grain size:** 16×16 tile chunks (256 tiles)
- **Operations:** Discovery at chunk level, transmission as batches, caching at chunk granularity
- **Pros:** Massive batching opportunity, exploits spatial locality, cache-friendly (immutable chunks)
- **Cons:** Adds coordinate system complexity, potential for partial visibility at edges

#### Option 3: Region-Based Partitioning
- **Grain size:** 64×64 tile regions (4,096 tiles)
- **Operations:** Discovery at region level
- **Pros:** Fewer boundary crossings, maximum batching
- **Cons:** Too coarse (large network messages ~40 KB), poor cache granularity (evict too much)

### Performance Analysis

**FOV radius 10 = ~121 tiles visible**

| Approach | Tiles/Operation | Operations/Movement | Message Size | Cache Granularity |
|----------|-----------------|---------------------|--------------|-------------------|
| Tile-level | 1 | 121 | ~100 bytes | Poor (tiny units) |
| 16×16 Chunk | 256 | 5-9 chunks | ~2.6 KB | Optimal |
| 64×64 Region | 4,096 | 1-2 regions | ~40 KB | Too coarse |

## Decision

**We will partition the world using 16×16 hex tile chunks as the fundamental spatial unit for discovery, caching, and network transmission.**

### Chunk Definition

```rust
/// Unique identifier for a 16×16 hex tile chunk
#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub struct ChunkId(pub i16, pub i16);

/// Immutable terrain data for a chunk (cache-friendly)
pub struct TerrainChunk {
    tiles: Vec<(Qrz, EntityType)>,  // Always 256 tiles
    generated_at: Instant,
}
```

### Rationale for 16×16

**Spatial locality balance:**
- FOV radius 10 ≈ 2-3 chunk radius (5-9 chunks visible)
- Most player movements stay within same chunk (15/16 moves)
- Boundary crossings trigger work, same-chunk movements are free

**Network efficiency:**
- ~2.6 KB per chunk (well within UDP limits)
- Single packet vs 256 packets
- Batch serialization reduces overhead

**Cache efficiency:**
- Power of 2 for bit operations
- 100,000 chunk cache = 25M tiles = 25% of world (~1.2 GB)
- Immutable chunks (Arc-based sharing)

**Scalability:**
- Small enough: Responsive to player movement
- Large enough: Effective batching and caching
- Goldilocks zone for our requirements

## Consequences

### Positive

**Performance at scale:**
- 275× reduction in discovery events (24,200/sec → 88/sec for 200 players)
- 275× reduction in network messages
- Shared cache eliminates redundant terrain generation in hubs
- Hub clustering becomes tenable (was crisis scenario)

**Architectural benefits:**
- **Spatial locality principle:** World naturally organized into cache-friendly units
- **Immutability:** Chunks never change after generation (perfect for Arc sharing)
- **Extensibility:** Foundation for streaming, persistence, predictive loading
- **Testability:** Chunk generation isolated from discovery logic

**Memory bounds:**
- Hard limit via LRU eviction (no unbounded growth)
- Configurable budget (tune for server specs)

### Negative

**Complexity:**
- New coordinate system (chunk coords ↔ tile coords)
- LRU cache management overhead
- Client-server eviction symmetry must be maintained
- More moving parts than tile-level system

**Edge cases:**
- Chunk boundary seams (mitigated with +1 chunk buffer)
- Partial chunk visibility at world edges
- Rapid boundary crossing (zigzagging players)

**Breaking change:**
- Network protocol incompatible with old clients
- Cannot roll back without downtime
- Acceptable for pre-production system

### Invariants That Must Be Enforced

**1. Deterministic Generation**
- Same ChunkId + same seed → identical TerrainChunk
- Required for cache consistency
- Already satisfied by Perlin noise with fixed seed

**2. Server-Client Eviction Symmetry**
- Server must track what client has evicted
- Prevents sending chunks client already has
- Prevents missing chunks (client evicted but server thinks it still has)
- **Critical for correctness**

**3. Bounded Memory**
- LRU must enforce hard limit
- Client must evict distant chunks
- No unbounded growth on either side

## Alternatives Rejected

**Hybrid approach (chunks for discovery, tiles for caching):**
- Adds complexity without benefits
- Loses spatial locality advantage
- Rejected: Keep abstraction consistent

**Variable chunk sizes (LOD-based):**
- Adds significant protocol complexity
- Premature optimization
- Deferred to future ADR if needed

**No chunking (optimize tile-level):**
- Doesn't address fundamental hub clustering problem
- Network remains chatty
- Rejected: Doesn't scale

## Implementation Notes

See [SOW-001](../03-sow/001-chunk-based-terrain-discovery.md) for implementation details.

**Actual implementation used 8×8 chunks** (vs 16×16 specified) for visual debugging during initial development. Still achieves 138× performance improvement, meeting all requirements. See SOW-001 Discussion section for rationale.

## References

- **RFC-001:** Feature request and feasibility analysis
- **SOW-001:** Implementation plan and acceptance review
- **GUIDANCE.md:** Chunk system usage patterns

## Related ADRs

- (Future) ADR on persistent world state (chunk serialization to disk)
- (Future) ADR on protocol versioning and migration

## Date

2025-10-28
