# ADR-001: Chunk-Based Terrain Discovery with Shared World Cache

## Status

Accepted

## Context

### Current System Problems

The existing terrain discovery system operates at single-tile granularity with per-player discovery state:

1. **Every player movement** triggers discovery of 121 tiles (1 center + 120 FOV tiles at distance 10)
2. **No shared state** - if 200 players are in the same hub, the same tiles are discovered 200 times
3. **Chatty protocol** - 121 network messages per player movement
4. **No caching** - terrain generation happens repeatedly for the same coordinates

### Scale Requirements

- **World size:** 100 km² (~160 million hex tiles)
- **Player count:** 1,000 concurrent
- **Distribution:** Clustered in hubs (50-200 players) with sparse exploration

### Performance Crisis in Hubs

**200-player hub scenario (current system):**
- Discovery events: 200 players × 121 tiles/move × 1 move/sec = **24,200 events/sec**
- Network messages: **24,200 messages/sec**
- Most discoveries are redundant (players discovering same tiles)
- Terrain regenerations: ~2,400/sec (with 90% cache)

**This is unsustainable.** The system will collapse under production load.

### Root Architectural Issues

1. **No world-level discovery state** - each player tracks their own discoveries independently
2. **Tile-level granularity** - operations at finest possible grain size
3. **Immediate discovery** - every tile transition triggers full FOV discovery
4. **No spatial grouping** - misses opportunities for batching and caching

## Decision

We will implement a **chunk-based terrain discovery system with shared world-level caching**.

### Core Changes

#### 1. Introduce Chunk Abstraction

**Chunk Definition:**
- 16×16 hex tile grid (256 tiles per chunk)
- Identified by `ChunkId(i16, i16)` - chunk coordinates in world space
- Immutable once generated (cache-friendly)

**Rationale for 16×16:**
- Balances spatial locality with message size (~2.6 KB per chunk)
- FOV radius 10 ≈ 2 chunk radius (5-9 chunks visible at once)
- Power of 2 for efficient bit operations
- Small enough for network messages, large enough for batching

#### 2. World-Level Discovery Cache

```rust
#[derive(Resource)]
struct WorldDiscoveryCache {
    /// Shared cache of generated chunks (Arc for cheap cloning)
    chunks: HashMap<ChunkId, Arc<TerrainChunk>>,

    /// LRU tracker for memory management
    access_order: LruCache<ChunkId, ()>,

    /// Memory budget: 100,000 chunks = ~1.2 GB
    max_chunks: usize,
}
```

**Key Properties:**
- **Shared across all players** - chunk generated once, used by everyone
- **LRU eviction** - keeps hot areas (hubs) in memory, evicts cold areas
- **Arc<TerrainChunk>** - cheap cloning when sending to multiple players
- **Bounded memory** - hard limit prevents unbounded growth

#### 3. Per-Player Discovery State

```rust
#[derive(Component)]
struct PlayerDiscoveryState {
    /// Chunks this player has been sent
    seen_chunks: HashSet<ChunkId>,

    /// Last chunk position (for delta detection)
    last_chunk: Option<ChunkId>,
}
```

**Purpose:**
- Track what each player already knows
- Prevent redundant sends
- Enable resumption on reconnect (future)

#### 4. Boundary-Triggered Discovery

Discovery only triggers when crossing chunk boundaries:

```rust
pub fn do_incremental(...) {
    let new_chunk = loc_to_chunk(*loc);

    // Only process if player crossed chunk boundary
    if player_state.last_chunk == Some(new_chunk) {
        continue;  // Same chunk, skip
    }

    // Calculate visible chunks (FOV distance 10 ≈ 2 chunk radius)
    let visible_chunks = calculate_visible_chunks(new_chunk, 2);

    for chunk_id in visible_chunks {
        if !player_state.seen_chunks.contains(&chunk_id) {
            writer.write(Try::DiscoverChunk { ent, chunk_id });
            player_state.seen_chunks.insert(chunk_id);
        }
    }
}
```

**Key Insight:** Most player movements stay within same chunk (15/16 moves). Only boundary crossings trigger work.

#### 5. New Network Protocol

**New Message Types:**

```rust
enum Event {
    // Existing events...

    /// Server-side only: request to discover a chunk
    DiscoverChunk { ent: Entity, chunk_id: ChunkId },

    /// Server → Client: chunk data containing 256 tiles
    ChunkData {
        ent: Entity,
        chunk_id: ChunkId,
        tiles: Vec<(Qrz, EntityType)>,  // 256 tiles
    },
}
```

**Properties:**
- `DiscoverChunk` is server-internal (Try event)
- `ChunkData` replaces 256 individual tile spawns
- Batch serialization (single allocation, single network packet)

### Discovery Flow (New)

```
Player crosses chunk boundary
        ↓
actor::do_incremental detects boundary crossing
        ↓
Calculate visible chunks (7-9 chunks for FOV=10)
        ↓
Filter already-seen chunks
        ↓
Emit Try::DiscoverChunk for new chunks
        ↓
try_discover_chunk checks WorldDiscoveryCache
        ↓
Cache HIT: Arc::clone (cheap) → send to player
Cache MISS: generate_chunk() → insert cache → send to player
        ↓
Event::ChunkData sent to client (256 tiles)
        ↓
Client unpacks and renders tiles
```

## Consequences

### Positive

#### Massive Performance Improvements

**200-player hub:**
- Discovery events: 24,200/sec → **88/sec** (275× reduction)
- Network messages: 24,200/sec → **88/sec** (275× reduction)
- Terrain generations: 2,400/sec → **88/sec first visit, then 0** (cache hits)

**Memory efficiency:**
- 1.2 GB cache covers 25M tiles (25% of world)
- LRU ensures hot areas stay cached
- Hubs remain permanently cached (high hit rate)

**Network bandwidth:**
- Per-player cost: 484 KB → **52 KB** (9.3× reduction)
- Batch serialization reduces protocol overhead
- Single packet vs 121 packets reduces latency

#### Architectural Benefits

1. **Shared state eliminates redundancy** - fundamental fix for hub clustering problem
2. **Spatial locality** - chunks are natural units for caching and batching
3. **Testability** - chunk generation isolated from discovery logic
4. **Extensibility** - chunks enable future features:
   - Persistent world state (save/load chunks)
   - Streaming (load chunks on-demand from disk)
   - Predictive loading (pre-generate adjacent chunks)
   - Chunk-level physics/AI optimization

#### Scalability

- **Scales to 1,000 players** in current architecture
- **Scales to 10,000 players** with minor tuning
- **Memory bounded** by LRU policy
- **Network traffic proportional to unique area explored**, not player count

### Negative

#### Implementation Complexity

1. **New abstractions** - ChunkId, TerrainChunk, chunk coordinate math
2. **Cache management** - LRU eviction, memory monitoring
3. **Protocol changes** - new message types, serialization
4. **Client updates** - handle batch tile insertion

**Mitigation:** Each phase builds on previous, incremental validation

#### Memory Overhead

- **1.2 GB dedicated cache** (vs current ~0 persistent memory)
- Need memory monitoring and alerts
- May need tuning for different server configurations

**Mitigation:** LRU provides hard limit, configurable budget

#### Edge Cases

1. **Chunk boundary seams** - player sees tiles pop in at edges
2. **Rapid boundary crossings** - player zigzagging could thrash
3. **Partial chunk visibility** - edge chunks only partially visible

**Mitigation:**
- 1-tile overlap between chunks (smooth transitions)
- Hysteresis on boundary detection
- Accept partial visibility (minor UX issue vs massive performance gain)

#### Cache Stampede Risk

If 200 players enter new area simultaneously:
- 200 × 7 chunks = 1,400 chunk requests
- Could spike CPU/memory

**Mitigation:**
- Rate limiting on chunk generation
- Priority queue (nearby players first)
- Stagger player movements (already happens naturally)

### Neutral

#### Breaking Change

- Old clients cannot understand `ChunkData` messages
- Requires coordinated server/client update
- Cannot roll back without downtime

**Acceptance:** This is acceptable for pre-production system. Future ADRs should address versioning.

#### Determinism

- Chunk generation must be deterministic (same seed → same output)
- Already true for Perlin noise with fixed seed
- Must preserve when adding caching

**Verification:** Automated tests comparing cached vs fresh generation

## Implementation Phases

### Phase 1: Core Types (1 day)

**Goal:** Foundation types with no behavior changes

```rust
// New types
struct ChunkId(i16, i16);
struct TerrainChunk { tiles: Vec<(Qrz, EntityType)>, generated_at: Instant }

// Conversion functions
fn loc_to_chunk(loc: Loc) -> ChunkId;
fn chunk_to_tile(chunk: ChunkId, offset_q: u8, offset_r: u8) -> Qrz;
fn calculate_visible_chunks(center: ChunkId, radius: u8) -> Vec<ChunkId>;
```

**Resources:**
```rust
#[derive(Resource)]
struct WorldDiscoveryCache {
    chunks: HashMap<ChunkId, Arc<TerrainChunk>>,
    access_order: LruCache<ChunkId, ()>,
    max_chunks: usize,
}

impl Default for WorldDiscoveryCache {
    fn default() -> Self {
        Self {
            chunks: HashMap::new(),
            access_order: LruCache::new(NonZeroUsize::new(100_000).unwrap()),
            max_chunks: 100_000,
        }
    }
}
```

**Components:**
```rust
#[derive(Component)]
struct PlayerDiscoveryState {
    seen_chunks: HashSet<ChunkId>,
    last_chunk: Option<ChunkId>,
}
```

**Tests:**
- Chunk coordinate conversions
- Visible chunk calculation (boundary cases)
- LRU eviction behavior

**Success Criteria:** All tests pass, types compile

### Phase 2: Server-Side Discovery (2-3 days)

**Goal:** Implement chunk-based discovery logic on server

**Modify `actor::do_incremental`:**
```rust
pub fn do_incremental(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut player_states: Query<&mut PlayerDiscoveryState>,
    query: Query<(&Loc, &Heading)>,
) {
    for Do { event: Event::Incremental { ent, component } } in reader.read() {
        let Component::Loc(loc) | Component::Heading(_) = component else { continue };

        let Ok(mut player_state) = player_states.get_mut(ent) else { continue };
        let Ok((current_loc, heading)) = query.get(ent) else { continue };

        let new_chunk = loc_to_chunk(**current_loc);

        // Skip if still in same chunk
        if player_state.last_chunk == Some(new_chunk) {
            continue;
        }

        // Calculate visible chunks based on FOV
        let fov_chunk_radius = 2;  // FOV distance 10 ≈ 2 chunks
        let visible_chunks = calculate_visible_chunks(new_chunk, fov_chunk_radius);

        for chunk_id in visible_chunks {
            if !player_state.seen_chunks.contains(&chunk_id) {
                writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id } });
                player_state.seen_chunks.insert(chunk_id);
            }
        }

        player_state.last_chunk = Some(new_chunk);
    }
}
```

**New System: `try_discover_chunk`**
```rust
pub fn try_discover_chunk(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut world_cache: ResMut<WorldDiscoveryCache>,
    terrain: Res<Terrain>,
    map: Res<Map>,
) {
    for Try { event: Event::DiscoverChunk { ent, chunk_id } } in reader.read() {
        // Check cache first
        let chunk = if let Some(cached) = world_cache.chunks.get(chunk_id) {
            // Cache hit - update LRU and clone Arc
            world_cache.access_order.get_or_insert(*chunk_id, || ());
            Arc::clone(cached)
        } else {
            // Cache miss - generate chunk
            let generated = Arc::new(generate_chunk(*chunk_id, &terrain, &map));

            // Insert into cache (may trigger LRU eviction)
            if world_cache.chunks.len() >= world_cache.max_chunks {
                if let Some((evicted_id, _)) = world_cache.access_order.pop_lru() {
                    world_cache.chunks.remove(&evicted_id);
                }
            }

            world_cache.chunks.insert(*chunk_id, Arc::clone(&generated));
            world_cache.access_order.get_or_insert(*chunk_id, || ());

            generated
        };

        // Send chunk to player
        writer.write(Do {
            event: Event::ChunkData {
                ent,
                chunk_id: *chunk_id,
                tiles: chunk.tiles.clone(),
            }
        });
    }
}

fn generate_chunk(chunk_id: ChunkId, terrain: &Terrain, map: &Map) -> TerrainChunk {
    let mut tiles = Vec::with_capacity(256);

    for offset_q in 0..16 {
        for offset_r in 0..16 {
            let qrz = chunk_to_tile(chunk_id, offset_q, offset_r);

            // Check if tile already exists in map (player-modified or pre-placed)
            let typ = if let Some((_qrz, typ)) = map.find(qrz + Qrz{q:0,r:0,z:30}, -60) {
                typ
            } else {
                // Generate new procedural tile
                let px = map.convert(qrz).xy();
                let z = terrain.get(px.x, px.y);
                let qrz_with_height = Qrz { q: qrz.q, r: qrz.r, z };
                EntityType::Decorator(Decorator { index: 3, is_solid: true })
            };

            tiles.push((qrz, typ));
        }
    }

    TerrainChunk { tiles, generated_at: Instant::now() }
}
```

**Add to server Update schedule:**
```rust
app.add_systems(Update, (
    actor::do_incremental,  // Modified
    actor::try_discover_chunk,  // New
    // ... existing systems
));
```

**Tests:**
- Chunk boundary detection
- Cache hit/miss behavior
- LRU eviction
- Multiple players discovering same chunk (verify shared cache)

**Success Criteria:**
- Server generates chunks on boundary crossings
- Cache prevents redundant generation
- `Event::ChunkData` emitted (client not yet updated)

### Phase 3: Protocol Changes (2 days)

**Goal:** Add new message types to network protocol

**Update `message.rs`:**
```rust
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub i16, pub i16);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    // Existing events...
    Spawn { ent: Entity, typ: EntityType, qrz: Qrz, attrs: Option<ActorAttributes> },
    Despawn { ent: Entity },
    // ...

    /// Server-side discovery request (Try event only)
    DiscoverChunk { ent: Entity, chunk_id: ChunkId },

    /// Server sends chunk data to client (Do event)
    ChunkData {
        ent: Entity,
        chunk_id: ChunkId,
        tiles: Vec<(Qrz, EntityType)>,
    },
}
```

**Serialization concerns:**
- `ChunkData` message size: ~2.6 KB (acceptable for renet)
- Vec serialization efficient with bincode
- ChunkId is Copy (cheap to serialize)

**Tests:**
- Serialize/deserialize round-trip
- Message size validation
- Protocol version compatibility (document breaking change)

**Success Criteria:**
- Messages serialize correctly
- Network tests pass
- No protocol errors in logs

### Phase 4: Client Integration (2-3 days)

**Goal:** Client receives and processes chunk data

**Update `client/systems/renet.rs`:**
```rust
pub fn do_manage_connections(
    mut reader: EventReader<Do>,
    mut commands: Commands,
    // ... existing parameters
) {
    for &message in reader.read() {
        match message {
            // Existing event handling...

            Do { event: Event::ChunkData { ent, chunk_id, tiles } } => {
                // Unpack chunk into individual tile spawns
                for (qrz, typ) in tiles {
                    // Check if tile already exists
                    if map.get(qrz).is_some() {
                        continue;  // Don't overwrite existing tiles
                    }

                    // Insert tile into map
                    map.insert(qrz, typ);

                    // Spawn visual representation (if decorator)
                    if let EntityType::Decorator(decorator) = typ {
                        commands.spawn(/* tile mesh bundle */);
                    }
                }

                debug!("Received chunk {:?} with {} tiles", chunk_id, tiles.len());
            }
        }
    }
}
```

**Update `client/systems/world.rs`:**
```rust
// Remove old single-tile discovery handling (now deprecated)
// pub fn do_spawn(...) { ... }  ← Remove or mark deprecated
```

**Initialize PlayerDiscoveryState on connection:**
```rust
// In connection init
commands.entity(player_ent).insert(PlayerDiscoveryState {
    seen_chunks: HashSet::new(),
    last_chunk: None,
});
```

**Tests:**
- Client receives and unpacks chunks correctly
- Tiles render properly
- No duplicate tiles created
- Reconnect preserves discovery state (future)

**Success Criteria:**
- Client renders terrain from chunk messages
- No visual regressions
- Performance monitoring shows improvement

## Validation Criteria

### Performance Benchmarks

**Test Scenario: 200 players in hub**
- All players within 5-chunk radius
- Simulate 1 movement/second per player
- Run for 60 seconds

**Metrics (before vs after):**
- Discovery events/sec: 24,200 → **< 500** (48× improvement minimum)
- Network messages/sec: 24,200 → **< 500**
- Terrain generations/sec: 2,400 → **< 100** (cache hits dominate)
- Memory usage: ~0 MB → **< 200 MB** (for 200-player test)
- Avg discovery latency: measure baseline → **< 50ms**

**Load Test: 1,000 players distributed**
- 5 hubs with 150 players each
- 250 players spread across world
- Simulate 30 minutes of gameplay

**Metrics:**
- Total memory usage: **< 1.5 GB**
- Cache hit rate: **> 90%** overall, **> 99%** in hubs
- Network bandwidth per player: **< 100 KB/min** sustained
- No memory leaks (run for 8 hours)
- No cache thrashing (monitor eviction rate)

### Correctness Tests

- **Determinism:** Same seed produces identical chunks across runs
- **Cache consistency:** Cached chunks match fresh generations
- **No missing tiles:** Every tile in FOV is sent exactly once
- **No duplicate tiles:** Same tile never sent twice to same player
- **Chunk boundaries:** No visual seams at chunk edges
- **Reconnect handling:** Player reconnect doesn't resend known chunks (future)

## Future Enhancements (Out of Scope)

### Persistent World State
- Save chunks to disk (SQLite or similar)
- Load chunks on server startup
- Track player modifications separately from procedural generation

### Predictive Loading
- Pre-generate chunks in player's direction of movement
- Background thread for chunk generation
- Priority queue based on player proximity

### Chunk Streaming
- Load inactive chunks from disk on-demand
- Unload cold chunks more aggressively
- Implement "warm" tier (compressed in memory)

### Dynamic LOD
- Different chunk sizes based on distance
- Lower detail for distant chunks
- Smooth LOD transitions

## References

- **Current discovery implementation:** `src/server/systems/actor.rs:20-79`
- **Terrain generation:** `src/server/resources/terrain.rs`
- **Map resource:** `src/common/resources/map.rs`
- **Network protocol:** `src/common/messages.rs`

## Related ADRs

- (Future) ADR-002: Persistent World State
- (Future) ADR-003: Protocol Versioning and Migration Strategy

## Decision Makers

- ARCHITECT role evaluation
- Performance requirements: 1,000 players, 100km² world, hub clustering

## Date

2025-10-28
