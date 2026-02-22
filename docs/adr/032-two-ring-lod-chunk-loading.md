# ADR-032: Two-Ring Level-of-Detail Chunk Loading

## Status

Accepted - 2026-02-21 (client-side implemented 2026-02-22; server-side LoD messages pending)

## Context

The adaptive visibility system (per-chunk `visibility_radius`) already produces an asymmetric loading shape — extending toward valleys, retracting toward ridges. However, every discovered chunk transmits full 64-tile data regardless of distance. Continental elevations reach 200–600, giving high-elevation players visibility radii up to 12 chunks. At outer distances, tiles are sub-pixel: meshes are built for geometry nobody can distinguish, the Map grows by 64 entries per chunk, and network transmits ~2.6KB per chunk for terrain that reads as color blobs.

The waste is proportional to the feature that makes the world interesting — the higher the player climbs, the further they see, and the more wasted work grows quadratically with radius.

### Options Considered

#### Option 1: Reduce MAX_TERRAIN_CHUNK_RADIUS
- **Pros:** Simple, immediate savings
- **Cons:** Visible gaps at distance, defeats the purpose of elevation-aware loading, cliffs show terrain ending abruptly

#### Option 2: Two-Ring LoD (Inner Full-Detail, Outer Summary)
- **Pros:** Maintains visual continuity, 200× network reduction per outer chunk, Map stays small, meshes are trivial
- **Cons:** New network message type, client needs separate summary storage, summary mesh rendering with neighbor coordination

#### Option 3: Multi-Tier LoD (3+ Detail Levels)
- **Pros:** Smoother quality transitions, optimal bandwidth at every distance
- **Cons:** Significant protocol complexity, multiple mesh paths, diminishing returns — the jump from 64 tiles to 1 summary captures most of the savings

## Decision

**Two-ring level-of-detail chunk loading with per-chunk visibility filtering.**

Inner ring (Chebyshev distance ≤ `FOV_CHUNK_RADIUS`): Full 64-tile chunks. All gameplay happens here — physics, pathfinding, combat, tile-level interactions.

Outer ring (`FOV_CHUNK_RADIUS` < distance ≤ max_radius): Each chunk summarized as a single 7-vertex hex. One Map entry instead of 64. ~12 bytes network instead of ~2.6KB. The outer ring's shape is asymmetric, using the existing `visibility_radius` per-chunk filtering.

The boundary between rings is `FOV_CHUNK_RADIUS`, which already serves as the minimum gameplay-safe loading distance.

### Rationale

**Network**: A radius-12 worst case loads up to 625 chunks. With base radius 5, the inner ring is ~121 chunks (full detail). The remaining ~504 outer chunks drop from ~2.6KB to ~12 bytes each — saving ~1.3MB per player spawn at high elevation.

**Map size**: 504 outer chunks × 64 tiles = 32,256 Map entries eliminated per player. At 200 players with overlapping visibility, this prevents millions of unnecessary tile entries.

**Mesh cost**: Each summary hex is 7 vertices, 6 triangles. Versus 64 tiles × 4 vertices = 256 vertices per full chunk. Summary meshes are trivially cheap.

**Visual quality**: Summary hexes use averaged elevation with sloped corners (each corner vertex averages the 3 chunks sharing it). This produces a continuous terrain surface — mountain ridges slope naturally, valleys dip, the landscape reads as coherent from distance. At outer-ring distances, individual tiles are sub-pixel anyway.

**Incremental**: Builds on the existing adaptive visibility infrastructure. `visibility_radius`, `calculate_visible_chunks_adaptive`, `chunk_max_z`, and `VisibleChunkCache` already exist. The change adds a LoD dimension to what's already an asymmetric loading system.

## Consequences

### Positive

**Performance at scale**: Dramatic reduction in network, memory, and mesh cost for high-elevation players. The savings grow with exactly the players who need them most (high elevation = large radius = most outer chunks).

**Visual continuity**: Terrain silhouette extends to the horizon. No visible chunk edges or terrain ending abruptly at distance. Summary hexes slope into each other, reading as coherent landscape.

**Clean separation**: Physics, movement, and pathfinding never see summaries — they only read from the Map, which only contains inner-ring full-detail tiles. No risk of gameplay interacting with approximate data.

**Graceful transitions**: Ring upgrades (summary → full detail) and downgrades (full detail → summary) happen seamlessly. The chunk is never absent from both representations.

### Negative

**New network message**: `Event::ChunkSummary` adds a message type. Both `server/systems/renet.rs` and `client/systems/renet.rs` need update per the renet event checklist.

**Neighbor coordination**: Summary mesh corner vertices require neighbor chunk elevations. Missing neighbors defer rendering — adds a retry path in the mesh pipeline.

**Dual storage**: Client maintains both `Map` (full tiles) and `ChunkSummaries` (summaries). Ring transitions must keep these consistent — no chunk should exist in both simultaneously.

**Server complexity**: `VisibleChunkCache` gains inner/outer distinction. `do_incremental` diff logic handles four transition types (enter inner, enter outer, inner↔outer, leave outer) instead of two (enter, leave).

### Invariants

**INV-006: Ring separation** — Physics, movement, and pathfinding only read from the Map (inner ring tiles), never from `ChunkSummaries`.

**INV-007: Continuous surface** — Adjacent summary hexes share corner vertices via neighbor elevation averaging. No floating platforms or gaps.

**INV-008: No terrain vanishing** — Ring downgrade (inner → outer) sends a summary before the client evicts full tiles. The chunk is never absent from both representations simultaneously.

## Key Files

| File | Changes |
|------|---------|
| `crates/common/src/chunk.rs` | `calculate_visible_chunks_adaptive` returns `(Vec<ChunkId>, Vec<ChunkId>)` (inner, outer); `ChunkSummary` struct |
| `crates/common/src/message.rs` | `Event::ChunkSummary` variant |
| `crates/server/src/systems/actor.rs` | `VisibleChunkCache` gains inner/outer fields; `do_spawn_discover` and `do_incremental` send both tiers; ring transition diff logic |
| `crates/server/src/systems/renet.rs` | Serialize/send `Event::ChunkSummary` |
| `crates/client/src/systems/renet.rs` | Deserialize/handle `Event::ChunkSummary` |
| `crates/client/src/systems/world.rs` | `ChunkSummaries` resource; summary mesh rendering with neighbor-gated construction; two-pass eviction |
| `crates/client/src/systems/admin.rs` | Flyover uses both tiers |

## Unchanged Systems

- **AOI system** (`aoi.rs`): stays fixed at `AOI_RADIUS`
- **`FOV_CHUNK_RADIUS`**: kept as minimum floor, doubles as LoD boundary
- **`calculate_visible_chunks`**: kept for non-terrain uses (mesh neighbor cascade)
- **Terrain dependency**: stays optional behind admin feature on client

## Future Extensions

- **Camera-terrain collision**: Pass actual camera distance as `half_viewport`. Loading shape tightens dynamically. Function signature already supports this.
- **Additional LoD tiers**: Intermediate tiers (e.g. 4×4 sample grid) slot between inner and outer using the same ring architecture.
- **Summary generation shortcut**: Generate summaries directly from terrain noise at chunk center without computing full 64 tiles.

## Related ADRs

- [ADR-001](001-chunk-based-world-partitioning.md) — Chunk-based world partitioning (originally deferred variable chunk sizes as premature; this ADR implements the LoD layer that was anticipated)

## Date

2026-02-21
