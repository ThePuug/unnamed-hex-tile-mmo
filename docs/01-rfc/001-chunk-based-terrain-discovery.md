# RFC-001: Chunk-Based Terrain Discovery

## Status

**Approved** - 2025-10-28

## Feature Request

### Player Need

The current terrain discovery system cannot scale to support our target of 1,000 concurrent players in a 100 km² world with hub clustering patterns (50-200 players concentrated in hubs).

### Problem Statement

**Current System Crisis:**
- Every player movement triggers discovery of 121 tiles (FOV radius 10)
- 200-player hub scenario generates **24,200 discovery events/second**
- **24,200 network messages/second** (most redundant - same tiles discovered by multiple players)
- No shared state - terrain regenerated repeatedly for same coordinates
- System will collapse under production load

**Root Causes:**
1. No world-level discovery state (each player tracks independently)
2. Tile-level granularity (finest possible grain size)
3. Immediate discovery on every movement
4. No spatial grouping for batching/caching

### Scale Requirements

- **World size:** 100 km² (~160 million hex tiles)
- **Player count:** 1,000 concurrent
- **Distribution:** Clustered in hubs (50-200 players) with sparse exploration
- **Performance target:** Sub-50ms discovery latency per player

### Desired Experience

Players should experience:
- Instant terrain rendering on spawn
- Smooth terrain streaming as they explore
- No stuttering or lag when many players cluster in hubs
- Seamless world exploration without visible loading boundaries

### Priority Justification

**CRITICAL** - This is a foundational blocker for multiplayer testing. Without solving the hub clustering problem, we cannot validate core gameplay with realistic player counts.

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Chunk-Based Discovery with Shared World Cache**

#### Core Concept

Replace tile-level discovery with chunk-based spatial partitioning:
- Group tiles into 16×16 hex chunks (256 tiles per chunk)
- World-level cache shared across all players (Arc-based)
- Discovery only triggers on chunk boundary crossings
- LRU eviction for bounded memory

#### Performance Projections

**200-player hub (with chunks):**
- Discovery events: 24,200/sec → **88/sec** (275× reduction)
- Network messages: 24,200/sec → **88/sec** (275× reduction)
- Terrain generations: 2,400/sec → **88/sec first visit, then 0** (cache hits)
- Per-player bandwidth: 484 KB → **52 KB** (9.3× reduction)

**Memory requirements:**
- 1.2 GB cache covers 25M tiles (25% of world)
- LRU ensures hot areas (hubs) stay cached
- Bounded by max_chunks limit (no unbounded growth)

#### Technical Risks

1. **Implementation Complexity:** New abstractions (ChunkId, coordinate math, LRU cache)
   - *Mitigation:* Phased implementation with incremental validation

2. **Cache Stampede:** 200 players entering new area simultaneously
   - *Mitigation:* Rate limiting, priority queue (can defer to post-MVP)

3. **Chunk Boundary Seams:** Visible tile pop-in at chunk edges
   - *Mitigation:* 1-chunk buffer overlap, hysteresis on boundary detection

4. **Protocol Breaking Change:** Old clients incompatible
   - *Acceptance:* Acceptable for pre-production system

#### System Integration

**Affected Systems:**
- Server discovery logic (complete rewrite)
- Network protocol (new ChunkData message type)
- Client terrain rendering (batch tile insertion)
- Physics/collision (must handle chunk eviction)

**Compatibility:**
- ✅ ECS architecture (chunks fit naturally as resources/components)
- ✅ Deterministic world gen (Perlin noise with fixed seed)
- ✅ Existing map structure (chunks build on current Qrz system)
- ⚠️ Network protocol (breaking change required)

### Alternatives Considered

#### Alternative 1: Keep Tile-Level, Add Shared Cache
- Still 24,200 events/sec (no reduction in discovery rate)
- Network still chatty (121 messages per movement)
- Only reduces terrain generation cost
- **Verdict:** Insufficient - doesn't solve hub clustering

#### Alternative 2: Region-Based Discovery (64×64 chunks)
- Fewer boundary crossings
- Larger network messages (~40 KB per region)
- Coarser spatial granularity (less cache efficiency)
- **Verdict:** Too coarse - trades precision for marginal efficiency

#### Alternative 3: Predictive Loading Only
- Pre-generate chunks in movement direction
- Still requires chunking infrastructure
- Doesn't eliminate hub clustering problem
- **Verdict:** Good future enhancement, not standalone solution

**Decision:** Proceed with 16×16 chunk-based system as optimal balance.

---

## Discussion

### ARCHITECT Notes

The chunk-based approach is architecturally sound. The key insight is that **spatial locality** is the fundamental optimization opportunity - hub clustering creates massive redundancy at tile-level, but near-zero redundancy at chunk-level.

**Critical Invariants to Enforce:**
1. Server-client eviction symmetry (prevent missing chunks)
2. Deterministic chunk generation (cache consistency)
3. Bounded memory (hard limits required)

**Extensibility Benefits:**
This foundation enables future features:
- Persistent world state (save/load chunks to disk)
- Streaming systems (on-demand chunk loading)
- Predictive loading (pre-generate adjacent chunks)
- Chunk-level physics/AI optimizations

### PLAYER Validation

From a player perspective, this should be invisible - they just see faster, smoother terrain loading. The real win is enabling the multiplayer hub experience without performance collapse.

**Acceptance criteria:**
- ✅ Instant terrain on spawn
- ✅ No stuttering in hubs
- ✅ Smooth exploration (no visible chunk boundaries)
- ✅ Supports 1,000 concurrent players

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- PLAYER: ✅ Solves critical multiplayer blocker, maintains desired UX
- ARCHITECT: ✅ Technically sound, scalable, maintainable architecture

**Scope Constraint:** Fits in one SOW (estimated 7-10 days implementation)

**Next Steps:**
1. ARCHITECT creates ADR-001 documenting chunk partitioning decision
2. ARCHITECT creates SOW-001 with implementation plan
3. DEVELOPER begins implementation

**Date:** 2025-10-28
