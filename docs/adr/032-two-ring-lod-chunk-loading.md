# ADR-032: Two-Ring Level-of-Detail Chunk Loading

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

**Uniform server-side chunk loading with client-side LoD rendering.**

Server sends full 271-tile chunks within `terrain_chunk_radius(max_z) + 1` — a simple hex radius based on the player's chunk elevation. All chunks are inserted into the Map. No server-side inner/outer distinction.

Client renders distant chunks at reduced detail using QEM (Quadric Error Metrics) decimation with RDP (Ramer-Douglas-Peucker) boundary edges.

### Rationale

**Simplicity**: Uniform chunk loading eliminates server-side ring transition complexity (4 transition types, eviction set computation, inner/outer cache tracking). `VisibleChunkCache` tracks only `{sent, chunk_id}`.

**Mesh cost**: Client-side QEM produces 6–54 boundary vertices + N interior vertices per summary mesh, reconstructed via Delaunay triangulation. Flat chunks compress to 6 corner vertices; high-variance chunks retain proportional detail.

**Visual quality**: QEM (Garland-Heckbert) decimation for interior vertices, RDP for boundary edges. 6 corner vertices (3-tile averages) always retained; edge vertices decimated per-edge against `BORDER_ERROR_THRESHOLD`. Adjacent chunks share boundary vertices deterministically, ensuring seamless tiling.

**Consistent loading**: At sea level, ~1,519 chunks / 7.5 MB. Scales consistently with elevation rather than exploding with frustum-based adaptive filtering.

## Consequences

### Positive

**Visual continuity**: Terrain silhouette extends to the horizon. No visible chunk edges or terrain ending abruptly at distance. QEM summary meshes slope into each other, reading as coherent landscape.

**Server simplicity**: `VisibleChunkCache` has 2 fields (`sent`, `chunk_id`). No ring transition logic, no eviction set computation, no adaptive per-chunk filtering.

**Clean separation**: ChunkSummaries are rendering-only. Physics, movement, and pathfinding read the Map.

### Negative

**Neighbor coordination**: QEM decimation requires neighbor elevations for boundary vertex computation. Missing neighbors defer rendering — adds a retry path in the mesh pipeline.

**Dual client storage**: Client maintains both `Map` (full tiles) and `ChunkSummaries` (QEM-decimated rendering data).

### Invariants

**INV-006: Summary separation** — ChunkSummaries are rendering-only. Physics, movement, and pathfinding read the Map, never ChunkSummaries.

**INV-007: Continuous surface** — Adjacent summary meshes share deterministic boundary vertices (corners + RDP-decimated edge vertices). No floating platforms or gaps.

## Key Files

| File | Changes |
|------|---------|
| `crates/common-bevy/src/chunk.rs` | `calculate_visible_chunks`, `terrain_chunk_radius`, visibility helpers |
| `crates/common-bevy/src/qem.rs` | `SummaryHexData`, `decimate_chunk()`, boundary helpers, Delaunay mesh reconstruction |
| `crates/common-bevy/src/message.rs` | `Event::DiscoverChunk` (no `summary_only`), `Event::ChunkData` |
| `crates/server/src/systems/actor.rs` | `VisibleChunkCache { sent, chunk_id }`, uniform chunk loading |
| `crates/server/src/systems/renet.rs` | Serialize/send `Event::ChunkData` |
| `crates/client/src/systems/renet.rs` | Deserialize/handle `Event::ChunkData` |
| `crates/client/src/systems/world.rs` | `ChunkSummaries` stores `SummaryHexData`; client-side QEM for LoD rendering |
| `crates/client/src/systems/admin.rs` | Flyover chunk generation |

## Unchanged Systems

- **AOI system** (`aoi.rs`): stays fixed at `AOI_RADIUS`
- **Terrain dependency**: stays optional behind admin feature on client

## Future Extensions

- **Error threshold tuning**: `SUMMARY_ERROR_THRESHOLD` (currently 2.0 wu) can be tightened after reviewing p95 geometric error metrics in the console.
- **Server-side LoD**: If bandwidth becomes a bottleneck, server can send QEM summaries for distant chunks instead of full data.

## Related ADRs

- [ADR-001](001-chunk-based-world-partitioning.md) — Chunk-based world partitioning (originally deferred variable chunk sizes as premature; this ADR implements the LoD layer that was anticipated)

## Date

2026-02-21
