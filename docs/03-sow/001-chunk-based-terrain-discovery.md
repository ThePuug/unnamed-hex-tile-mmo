# SOW-001: Chunk-Based Terrain Discovery

## Status

**Merged** - 2025-10-29

## References

- **RFC-001:** [Chunk-Based Terrain Discovery](../01-rfc/001-chunk-based-terrain-discovery.md)
- **ADR-001:** [Chunk-Based World Partitioning](../02-adr/001-chunk-based-world-partitioning.md)
- **Branch:** `terrain-chunking` → `main`
- **Implementation Time:** 7-9 days

---

## Implementation Plan

### Phase 1: Core Types (1 day)

**Goal:** Foundation types with no behavior changes

**Deliverables:**
- `ChunkId` newtype for type-safe chunk coordinates
- `TerrainChunk` struct with tiles and generation timestamp
- Conversion functions: `loc_to_chunk`, `chunk_to_tile`, `calculate_visible_chunks`
- `WorldDiscoveryCache` resource with LRU eviction
- `PlayerDiscoveryState` component tracking seen chunks

**Architectural Constraints:**
- Use Duration (not Instant) for timestamps
- LRU cache enforces hard limit (100,000 chunks)
- Chunk size configurable constant (spec: 16×16, implementation: 8×8 for debugging)

**Success Criteria:** All tests pass, types compile, no behavior changes

---

### Phase 2: Server-Side Discovery (2-3 days)

**Goal:** Implement chunk-based discovery logic on server

**Deliverables:**
- Modified `actor::do_incremental` detects chunk boundary crossings
- New `try_discover_chunk` system handles cache hit/miss
- Cache uses Arc for zero-copy sharing between players
- Emit `Event::ChunkData` to clients on discovery

**Architectural Constraints:**
- Server owns authoritative chunk cache
- FOV-based chunk radius calculation (10 hex FOV ≈ 2 chunks)
- LRU eviction on cache pressure
- Idempotent chunk generation (same seed → same output)

**Success Criteria:** Server generates chunks on boundaries, cache prevents redundant generation

---

### Phase 3: Protocol Changes (2 days)

**Goal:** Add chunk message types to network protocol

**Deliverables:**
- `Event::DiscoverChunk` (Try event, server-internal)
- `Event::ChunkData` (Do event, broadcast to client)
- ChunkId serialization
- Tile array serialization

**Architectural Constraints:**
- Use ArrayVec for fixed-size tile storage (no heap allocation)
- Chunk data ~680 bytes (8×8) or ~2.6 KB (16×16)
- Compatible with existing Do/Try event pattern

**Success Criteria:** Messages serialize correctly, network tests pass

---

### Phase 4: Client Integration (2-3 days)

**Goal:** Client receives and processes chunk data

**Deliverables:**
- Handle `Event::ChunkData` in client renet system
- Unpack tiles and spawn visual representations
- Track `LoadedChunks` resource
- Eviction system removes distant chunks (FOV_CHUNK_RADIUS + 1 buffer)

**Architectural Constraints:**
- Client mirrors server eviction strategy (same radius calculation)
- No duplicate tiles created
- Chunk-level granularity (not tile-level overlap)

**Success Criteria:** Client renders terrain from chunks, no visual regressions, performance improved

---

## Acceptance Criteria

**Performance:**
- ✅ Discovery events reduced by > 48× → achieved 138×
- ✅ Network messages reduced by > 48× → achieved 138×
- ✅ Memory usage < 1.5 GB → actual ~400 MB
- ✅ Support 1,000 concurrent players

**Correctness:**
- ✅ Deterministic chunk generation
- ✅ No missing tiles in FOV
- ✅ No duplicate tiles sent
- ✅ No visual seams at chunk boundaries

**UX:**
- ✅ Instant terrain rendering on spawn
- ✅ Smooth exploration (no loading boundaries)

---

## Discussion

### Implementation Deviation: Chunk Size (8×8 vs 16×16)

**Context:** ADR-001 specifies 16×16 chunks (256 tiles). Implementation uses 8×8 chunks (64 tiles).

**Rationale:**
- Smaller chunks make boundaries visible for debugging
- Easier to observe loading/eviction behavior during development
- Can be increased to 16×16 without architectural changes
- Developer freedom for debugging purposes

**Impact:**
- More frequent boundary crossings (2×)
- Smaller messages (~680 bytes vs ~2.6 KB)
- Still achieves 138× improvement (vs 275× target)
- All performance requirements met

**Decision:** Accepted. Valid engineering tradeoff for debuggability.

---

### Implementation Enhancement: Server Map Persistence

**Addition:** Server maintains persistent tile map separate from chunk cache.

**Rationale:**
- Cache eviction should not break physics/collision
- NPCs need stable terrain for pathfinding
- Separates concerns: cache = optimization, map = authority
- Idempotent insertion prevents duplication

**Impact:** Prevents evicted terrain becoming unwalkable, improves architectural clarity

**Decision:** Excellent addition, improves robustness.

---

### Implementation Enhancement: Initial Spawn Discovery

**Addition:** Separate system for initial chunk discovery on player spawn.

**Rationale:**
- Players need terrain immediately on spawn
- Prevents "blank world until first movement" edge case
- Clean separation: spawn vs movement discovery

**Decision:** Necessary implementation detail, good practice.

---

### Implementation Enhancement: Client Eviction Strategy

**Actual Implementation:** +1 chunk buffer (7×7 chunks retained vs 5×5 visible)

**Rationale:**
- Simpler than per-tile overlap
- Prevents aggressive eviction/reloading (hysteresis)
- Server mirrors logic for symmetry
- Chunk-level granularity maintains abstraction

**Impact:** 2× memory overhead (still bounded), no seam artifacts, simpler code

**Decision:** Different mitigation, equally effective.

---

### Implementation Enhancement: ArrayVec for Protocol

**Actual Implementation:** ArrayVec (stack-allocated, fixed-size) instead of Vec

**Rationale:**
- No heap allocation for network messages
- Fixed size matches chunk size exactly
- Better performance characteristics

**Decision:** Excellent choice, better than spec.

---

## Acceptance Review

**Review Date:** 2025-10-29
**Reviewer:** ARCHITECT Role
**Decision:** ✅ **ACCEPTED**

### Phase Completion Status

**All 4 phases complete:**
- ✅ Phase 1: Core Types (ChunkId, TerrainChunk, conversion functions, cache, tests)
- ✅ Phase 2: Server-Side Discovery (boundary detection, cache logic, initial spawn system)
- ✅ Phase 3: Protocol Changes (ChunkData message, ArrayVec serialization)
- ✅ Phase 4: Client Integration (chunk unpacking, rendering, eviction)

**Enhancements beyond plan:**
- Server map persistence
- Initial spawn discovery system
- ArrayVec optimization
- +1 chunk buffer eviction strategy

---

### Critical Invariants Verification

**✅ Server-Client Eviction Symmetry:** Both use `FOV_CHUNK_RADIUS + 1` with shared `calculate_visible_chunks`

**⚠️ Deterministic Chunk Generation:** Implementation sound (Perlin noise with fixed seed), test coverage gap (non-blocking)

**✅ Bounded Memory:** LRU enforces 100,000 chunk limit on server, client evicts chunks outside buffer

---

### Performance Verification

| Metric | Before | Target (16×16) | Actual (8×8) | Status |
|--------|--------|----------------|--------------|--------|
| Discovery events/sec | 24,200 | 88 (275×) | ~175 (138×) | ✅ |
| Network messages/sec | 24,200 | 88 (275×) | ~175 (138×) | ✅ |
| Message size/player | 484 KB | 52 KB (9.3×) | ~21 KB (23×) | ✅ |
| Memory usage | 0 MB | < 1.5 GB | ~400 MB | ✅ |

**Verdict:** All requirements satisfied, 138× improvement achieves 2 orders of magnitude reduction.

---

### Code Quality

**Architectural Quality:** ✅ EXCELLENT
- Clean module boundaries (`common/chunk.rs` → `server/systems/actor.rs` → `client/systems/world.rs`)
- Proper abstraction (ChunkId newtype, type-safe coordinates)
- Unidirectional dependencies

**Testing:** ✅ GOOD
- Comprehensive unit tests (coordinate conversions, visible chunks, LRU eviction, discovery flow)
- Recommended additions: determinism test, eviction symmetry test (non-blocking)

**Documentation:** ✅ EXCELLENT
- RFC-001, ADR-001, SOW-001 complete
- GUIDANCE.md updated
- Inline comments explain invariants

---

### Technical Debt

**MINOR:**
- Legacy discovery system (remove after production validation)
- Test coverage gaps (determinism, eviction symmetry)
- Cache observability (hit rate metrics)

**Timeline:** Before 1.0 release

---

### Recommended Follow-Up

1. Add determinism and eviction symmetry tests (2-4 hours)
2. Remove legacy discovery system after validation (1-2 days)
3. Consider increasing chunk size to 16×16 post-debugging (future)

---

## Conclusion

The chunk-based terrain discovery implementation demonstrates **excellent architectural discipline** and **sound engineering judgment**.

**Key Achievements:**
- Performance goals met: 138× reduction
- Architectural integrity maintained: clean separation, proper abstractions
- Critical invariants enforced: server-client symmetry via shared constants
- Smart enhancements: server map persistence, spawn discovery, ArrayVec

**Architectural Impact:** Provides foundation for persistent world state, streaming systems, predictive loading, and multiplayer scaling.

**The implementation achieves RFC-001's core goal: eliminating the hub clustering performance crisis while maintaining architectural integrity.**

---

## Sign-Off

**Reviewed By:** ARCHITECT Role
**Date:** 2025-10-29
**Decision:** ✅ **ACCEPTED**
**Status:** Merged to main
