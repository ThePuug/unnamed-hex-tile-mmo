use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::common::components::entity_type::*;

/// Chunk size in tiles (8x8 = 64 tiles per chunk, smaller for easier visual debugging)
pub const CHUNK_SIZE: i16 = 8;

/// Field of view distance in chunks (FOV distance 10 ≈ 2 chunk radius)
pub const FOV_CHUNK_RADIUS: u8 = 2;

/// Chunk identifier in chunk-coordinate space
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub i16, pub i16);

impl ChunkId {
    pub fn new(chunk_q: i16, chunk_r: i16) -> Self {
        ChunkId(chunk_q, chunk_r)
    }
}

/// A chunk of terrain containing up to 64 tiles (8x8)
#[derive(Clone, Debug)]
pub struct TerrainChunk {
    pub tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 64]>,
    pub generated_at: Instant,
}

impl TerrainChunk {
    pub fn new(tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 64]>) -> Self {
        Self {
            tiles,
            generated_at: Instant::now(),
        }
    }
}

/// Per-player discovery state tracking which chunks a player has seen
#[derive(Component, Debug)]
pub struct PlayerDiscoveryState {
    /// Chunks this player has been sent
    pub seen_chunks: HashSet<ChunkId>,

    /// Last chunk position (for delta detection)
    pub last_chunk: Option<ChunkId>,
}

impl Default for PlayerDiscoveryState {
    fn default() -> Self {
        Self {
            seen_chunks: HashSet::new(),
            last_chunk: None,
        }
    }
}

/// World-level cache of generated terrain chunks (shared across all players)
#[derive(Resource)]
pub struct WorldDiscoveryCache {
    /// Shared cache of generated chunks (Arc for cheap cloning)
    pub chunks: HashMap<ChunkId, Arc<TerrainChunk>>,

    /// LRU tracker for memory management
    pub access_order: LruCache<ChunkId, ()>,

    /// Memory budget: 100,000 chunks = ~1.2 GB
    pub max_chunks: usize,
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

/// Convert a Loc (Qrz) to its containing chunk ID
pub fn loc_to_chunk(loc: Qrz) -> ChunkId {
    // Use floor division to get chunk coordinates
    let chunk_q = loc.q.div_euclid(CHUNK_SIZE);
    let chunk_r = loc.r.div_euclid(CHUNK_SIZE);
    ChunkId(chunk_q, chunk_r)
}

/// Convert a chunk ID and offset within the chunk to a tile Qrz
/// offset_q and offset_r must be in range [0, CHUNK_SIZE)
pub fn chunk_to_tile(chunk_id: ChunkId, offset_q: u8, offset_r: u8) -> Qrz {
    let tile_q = chunk_id.0 * CHUNK_SIZE + offset_q as i16;
    let tile_r = chunk_id.1 * CHUNK_SIZE + offset_r as i16;
    Qrz { q: tile_q, r: tile_r, z: 0 }
}

/// Calculate visible chunks based on FOV distance
/// For FOV distance 10, we use chunk radius 2
pub fn calculate_visible_chunks(center: ChunkId, radius: u8) -> Vec<ChunkId> {
    let mut visible = Vec::new();
    let r = radius as i16;

    // Generate a square of chunks around the center
    // This is a conservative approximation of the circular FOV
    for dq in -r..=r {
        for dr in -r..=r {
            visible.push(ChunkId(center.0 + dq, center.1 + dr));
        }
    }

    visible
}

/// Check if a location (Qrz) is within a specific chunk
pub fn is_loc_in_chunk(loc: Qrz, chunk_id: ChunkId) -> bool {
    loc_to_chunk(loc) == chunk_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loc_to_chunk_positive() {
        // Tile (0,0) is in chunk (0,0)
        assert_eq!(loc_to_chunk(Qrz { q: 0, r: 0, z: 0 }), ChunkId(0, 0));

        // Tile (7,7) is in chunk (0,0) - now 8x8 chunks
        assert_eq!(loc_to_chunk(Qrz { q: 7, r: 7, z: 0 }), ChunkId(0, 0));

        // Tile (8,8) is in chunk (1,1) - now 8x8 chunks
        assert_eq!(loc_to_chunk(Qrz { q: 8, r: 8, z: 0 }), ChunkId(1, 1));

        // Tile (15,0) is in chunk (1,0) - now 8x8 chunks
        assert_eq!(loc_to_chunk(Qrz { q: 15, r: 0, z: 0 }), ChunkId(1, 0));
    }

    #[test]
    fn test_loc_to_chunk_negative() {
        // Tile (-1,-1) is in chunk (-1,-1)
        assert_eq!(loc_to_chunk(Qrz { q: -1, r: -1, z: 0 }), ChunkId(-1, -1));

        // Tile (-8,-8) is in chunk (-1,-1) - now 8x8 chunks
        assert_eq!(loc_to_chunk(Qrz { q: -8, r: -8, z: 0 }), ChunkId(-1, -1));

        // Tile (-9,-9) is in chunk (-2,-2) - now 8x8 chunks
        assert_eq!(loc_to_chunk(Qrz { q: -9, r: -9, z: 0 }), ChunkId(-2, -2));
    }

    #[test]
    fn test_chunk_to_tile() {
        // Chunk (0,0) offset (0,0) = tile (0,0)
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 0, 0).q, 0);
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 0, 0).r, 0);

        // Chunk (0,0) offset (7,7) = tile (7,7) - now 8x8 chunks
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 7, 7).q, 7);
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 7, 7).r, 7);

        // Chunk (1,1) offset (0,0) = tile (8,8) - now 8x8 chunks
        assert_eq!(chunk_to_tile(ChunkId(1, 1), 0, 0).q, 8);
        assert_eq!(chunk_to_tile(ChunkId(1, 1), 0, 0).r, 8);

        // Chunk (-1,-1) offset (0,0) = tile (-8,-8) - now 8x8 chunks
        assert_eq!(chunk_to_tile(ChunkId(-1, -1), 0, 0).q, -8);
        assert_eq!(chunk_to_tile(ChunkId(-1, -1), 0, 0).r, -8);
    }

    #[test]
    fn test_chunk_to_tile_round_trip() {
        // For any chunk and offset, converting back should give the same chunk
        let chunk = ChunkId(5, -3);
        for offset_q in 0..8 {  // Now 8x8 chunks
            for offset_r in 0..8 {  // Now 8x8 chunks
                let tile = chunk_to_tile(chunk, offset_q, offset_r);
                let recovered_chunk = loc_to_chunk(tile);
                assert_eq!(recovered_chunk, chunk,
                    "Round trip failed for chunk {:?} offset ({},{}) -> tile {:?} -> chunk {:?}",
                    chunk, offset_q, offset_r, tile, recovered_chunk);
            }
        }
    }

    #[test]
    fn test_calculate_visible_chunks_radius_0() {
        let center = ChunkId(0, 0);
        let visible = calculate_visible_chunks(center, 0);

        // Radius 0 should only include the center chunk
        assert_eq!(visible.len(), 1);
        assert!(visible.contains(&ChunkId(0, 0)));
    }

    #[test]
    fn test_calculate_visible_chunks_radius_1() {
        let center = ChunkId(0, 0);
        let visible = calculate_visible_chunks(center, 1);

        // Radius 1 should include 3x3 = 9 chunks
        assert_eq!(visible.len(), 9);
        assert!(visible.contains(&ChunkId(0, 0)));
        assert!(visible.contains(&ChunkId(-1, -1)));
        assert!(visible.contains(&ChunkId(1, 1)));
        assert!(visible.contains(&ChunkId(-1, 0)));
        assert!(visible.contains(&ChunkId(0, 1)));
    }

    #[test]
    fn test_calculate_visible_chunks_radius_2() {
        let center = ChunkId(10, -5);
        let visible = calculate_visible_chunks(center, 2);

        // Radius 2 should include 5x5 = 25 chunks
        assert_eq!(visible.len(), 25);
        assert!(visible.contains(&ChunkId(10, -5)));  // center
        assert!(visible.contains(&ChunkId(8, -7)));   // corner
        assert!(visible.contains(&ChunkId(12, -3)));  // corner
        assert!(visible.contains(&ChunkId(10, -7)));  // edge
        assert!(visible.contains(&ChunkId(12, -5)));  // edge
    }

    #[test]
    fn test_world_discovery_cache_default() {
        let cache = WorldDiscoveryCache::default();

        assert_eq!(cache.chunks.len(), 0);
        assert_eq!(cache.max_chunks, 100_000);
    }

    #[test]
    fn test_player_discovery_state_default() {
        let state = PlayerDiscoveryState::default();

        assert_eq!(state.seen_chunks.len(), 0);
        assert_eq!(state.last_chunk, None);
    }

    #[test]
    fn test_lru_eviction_behavior() {
        let mut cache = WorldDiscoveryCache::default();
        cache.max_chunks = 3;  // Small cache for testing

        // Create some test chunks
        let chunk1 = ChunkId(0, 0);
        let chunk2 = ChunkId(1, 1);
        let chunk3 = ChunkId(2, 2);
        let chunk4 = ChunkId(3, 3);

        let mut tiles1 = tinyvec::ArrayVec::new();
        tiles1.push((Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(crate::common::components::entity_type::decorator::Decorator { index: 0, is_solid: true })));
        let tiles2 = tiles1.clone();
        let tiles3 = tiles1.clone();
        let tiles4 = tiles1.clone();

        // Insert 3 chunks (fills cache)
        cache.chunks.insert(chunk1, Arc::new(TerrainChunk::new(tiles1)));
        cache.access_order.put(chunk1, ());

        cache.chunks.insert(chunk2, Arc::new(TerrainChunk::new(tiles2)));
        cache.access_order.put(chunk2, ());

        cache.chunks.insert(chunk3, Arc::new(TerrainChunk::new(tiles3)));
        cache.access_order.put(chunk3, ());

        assert_eq!(cache.chunks.len(), 3);

        // Insert 4th chunk (should trigger eviction)
        if cache.chunks.len() >= cache.max_chunks {
            if let Some((evicted_id, _)) = cache.access_order.pop_lru() {
                cache.chunks.remove(&evicted_id);
            }
        }

        cache.chunks.insert(chunk4, Arc::new(TerrainChunk::new(tiles4)));
        cache.access_order.put(chunk4, ());

        // Should still have 3 chunks, but chunk1 (LRU) should be evicted
        assert_eq!(cache.chunks.len(), 3);
        assert!(!cache.chunks.contains_key(&chunk1));
        assert!(cache.chunks.contains_key(&chunk2));
        assert!(cache.chunks.contains_key(&chunk3));
        assert!(cache.chunks.contains_key(&chunk4));
    }
}
