use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::components::entity_type::*;

/// Chunk size in tiles (16x16 = 256 tiles per chunk)
pub const CHUNK_SIZE: i32 = 16;

/// Field of view distance in chunks (expanded to prevent visible chunk edges when zoomed out)
pub const FOV_CHUNK_RADIUS: u8 = 5;

/// Absolute cap on elevation-adaptive chunk radius
pub const MAX_TERRAIN_CHUNK_RADIUS: u8 = 12;

/// Compute the chunk distance at which terrain at `ground_z` is visible from
/// a player at `player_z`, using orthographic ray-ground intersection.
///
/// Uses worst-case camera geometry (21:9 aspect). When `ground_z >= player_z`,
/// returns `FOV_CHUNK_RADIUS` (nearby terrain always within base radius).
///
/// `half_viewport` is the orthographic half-height in world units
/// (e.g. 40.0 for normal gameplay at max zoom, 20.0 × scale for flyover).
pub fn visibility_radius(player_z: i32, ground_z: i32, half_viewport: f32) -> u8 {
    // Ground at or above player fills the viewport at base radius
    if ground_z >= player_z {
        return FOV_CHUNK_RADIUS;
    }

    // Camera geometry (matches camera.rs constants)
    let camera_d: f32 = 40.0; // CAMERA_DISTANCE
    let camera_h: f32 = 30.0; // CAMERA_HEIGHT
    let look_r: f32 = 1.0;    // look target offset
    let arm = ((camera_h - look_r).powi(2) + camera_d.powi(2)).sqrt(); // ≈49.41

    // Camera basis vectors (orbit angle 0, worst case)
    let up_y = camera_d / arm;              // 0.8098
    let up_z = (camera_h - look_r) / arm;   // 0.5871
    let fwd_y = up_z;                       // 0.5871
    let fwd_z = up_y;                       // 0.8098

    // World elevations (terrain z → world y via 0.8 hex rise)
    let player_y = player_z as f32 * 0.8;
    let ground_y = ground_z as f32 * 0.8;

    // Top-center viewport ray origin
    let origin_y = player_y + camera_h + half_viewport * up_y;

    // Time for ray to descend to ground plane
    let t = (origin_y - ground_y) / fwd_y;

    // Forward ground distance from player
    let forward = t * fwd_z - (camera_d - half_viewport * up_z);

    // Worst-case lateral extent (21:9 aspect ratio)
    let lateral = half_viewport * (21.0 / 9.0);

    let max_dist = (forward * forward + lateral * lateral).sqrt();
    let chunk_extent = CHUNK_SIZE as f32 * 1.5;
    let needed = (max_dist / chunk_extent).ceil().min(255.0) as u8;

    needed.max(FOV_CHUNK_RADIUS)
}

/// Raw (uncapped) chunk radius needed at a given terrain height.
///
/// Equivalent to `visibility_radius(player_z, 0, 40.0)` — worst case
/// (sea-level ground, max zoom-out, 21:9 aspect).
pub fn elevation_chunk_radius_raw(player_z: i32) -> u8 {
    visibility_radius(player_z, 0, 40.0)
}

/// Compute the chunk loading radius for normal gameplay at a given terrain height.
///
/// Capped at `MAX_TERRAIN_CHUNK_RADIUS` to bound server/client load.
pub fn terrain_chunk_radius(player_z: i32) -> u8 {
    elevation_chunk_radius_raw(player_z).min(MAX_TERRAIN_CHUNK_RADIUS)
}

/// Chunk identifier in chunk-coordinate space
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub i32, pub i32);

impl ChunkId {
    /// Get the center tile of this chunk (for spawning engagements)
    pub fn center(&self) -> Qrz {
        // Center is at offset (8, 8) in a 16x16 chunk
        chunk_to_tile(*self, 8, 8)
    }
}

/// Summary data for a chunk rendered at low detail (outer ring LoD).
///
/// Contains only the representative elevation and biome — no individual tiles.
/// Reduces network traffic from ~2.6KB to ~12 bytes per chunk.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ChunkSummary {
    pub chunk_id: ChunkId,
    pub elevation: i32,
    pub biome: EntityType,
}

/// A chunk of terrain containing up to 256 tiles (16x16)
#[derive(Clone, Debug)]
pub struct TerrainChunk {
    pub tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 256]>,
}

impl TerrainChunk {
    pub fn new(tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 256]>) -> Self {
        Self {
            tiles,
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
    let tile_q = chunk_id.0 * CHUNK_SIZE + offset_q as i32;
    let tile_r = chunk_id.1 * CHUNK_SIZE + offset_r as i32;
    Qrz { q: tile_q, r: tile_r, z: 0 }
}

/// Calculate visible chunks based on FOV distance
/// For FOV distance 10, we use chunk radius 2
pub fn calculate_visible_chunks(center: ChunkId, radius: u8) -> Vec<ChunkId> {
    let mut visible = Vec::new();
    let r = radius as i32;

    // Generate a square of chunks around the center
    // This is a conservative approximation of the circular FOV
    for dq in -r..=r {
        for dr in -r..=r {
            visible.push(ChunkId(center.0 + dq, center.1 + dr));
        }
    }

    visible
}

/// Calculate visible chunks using per-chunk elevation-aware filtering.
///
/// Returns `(inner, outer)` where:
/// - `inner`: chunks within `FOV_CHUNK_RADIUS` (full 64-tile detail)
/// - `outer`: chunks beyond `FOV_CHUNK_RADIUS` that pass visibility (summary LoD)
///
/// Chunks within `base_radius` are always included (skip height query).
/// This is typically `terrain_chunk_radius(player_z)` — the old symmetric
/// radius — so flat terrain loads at least as many chunks as before.
/// Outer chunks beyond `base_radius` are included only if their ground
/// elevation puts them within the camera's visible range from `player_z`.
///
/// `height_fn(q, r)` returns terrain height at tile (q, r).
pub fn calculate_visible_chunks_adaptive(
    center: ChunkId,
    player_z: i32,
    base_radius: u8,
    max_radius: u8,
    half_viewport: f32,
    height_fn: impl Fn(i32, i32) -> i32,
) -> (Vec<ChunkId>, Vec<ChunkId>) {
    let mut inner = Vec::new();
    let mut outer = Vec::new();
    let r = max_radius as i32;
    let base = base_radius as i32;
    let fov = FOV_CHUNK_RADIUS as i32;

    for dq in -r..=r {
        for dr in -r..=r {
            let chebyshev = dq.abs().max(dr.abs());
            let chunk_id = ChunkId(center.0 + dq, center.1 + dr);

            // Determine if chunk is visible
            let included = if chebyshev <= base {
                true // Always include within base radius
            } else {
                let center_tile = chunk_id.center();
                let chunk_z = height_fn(center_tile.q, center_tile.r);
                let vis_radius = visibility_radius(player_z, chunk_z, half_viewport) as i32;
                chebyshev <= vis_radius
            };

            if included {
                if chebyshev <= fov {
                    inner.push(chunk_id);
                } else {
                    outer.push(chunk_id);
                }
            }
        }
    }

    (inner, outer)
}

/// Returns the maximum elevation across all tiles in a chunk.
///
/// Used to compute worst-case visibility: any position within this chunk
/// can see at most what `visibility_radius(max_z, ...)` would return.
pub fn chunk_max_z(chunk_id: ChunkId, height_fn: impl Fn(i32, i32) -> i32) -> i32 {
    let mut max_z = i32::MIN;
    for oq in 0..CHUNK_SIZE as u8 {
        for or_ in 0..CHUNK_SIZE as u8 {
            let tile = chunk_to_tile(chunk_id, oq, or_);
            let z = height_fn(tile.q, tile.r);
            if z > max_z {
                max_z = z;
            }
        }
    }
    max_z
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

        // Tile (15,15) is in chunk (0,0) - now 16x16 chunks
        assert_eq!(loc_to_chunk(Qrz { q: 15, r: 15, z: 0 }), ChunkId(0, 0));

        // Tile (16,16) is in chunk (1,1) - now 16x16 chunks
        assert_eq!(loc_to_chunk(Qrz { q: 16, r: 16, z: 0 }), ChunkId(1, 1));

        // Tile (31,0) is in chunk (1,0) - now 16x16 chunks
        assert_eq!(loc_to_chunk(Qrz { q: 31, r: 0, z: 0 }), ChunkId(1, 0));
    }

    #[test]
    fn test_loc_to_chunk_negative() {
        // Tile (-1,-1) is in chunk (-1,-1)
        assert_eq!(loc_to_chunk(Qrz { q: -1, r: -1, z: 0 }), ChunkId(-1, -1));

        // Tile (-16,-16) is in chunk (-1,-1) - now 16x16 chunks
        assert_eq!(loc_to_chunk(Qrz { q: -16, r: -16, z: 0 }), ChunkId(-1, -1));

        // Tile (-17,-17) is in chunk (-2,-2) - now 16x16 chunks
        assert_eq!(loc_to_chunk(Qrz { q: -17, r: -17, z: 0 }), ChunkId(-2, -2));
    }

    #[test]
    fn test_chunk_to_tile() {
        // Chunk (0,0) offset (0,0) = tile (0,0)
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 0, 0).q, 0);
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 0, 0).r, 0);

        // Chunk (0,0) offset (15,15) = tile (15,15) - now 16x16 chunks
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 15, 15).q, 15);
        assert_eq!(chunk_to_tile(ChunkId(0, 0), 15, 15).r, 15);

        // Chunk (1,1) offset (0,0) = tile (16,16) - now 16x16 chunks
        assert_eq!(chunk_to_tile(ChunkId(1, 1), 0, 0).q, 16);
        assert_eq!(chunk_to_tile(ChunkId(1, 1), 0, 0).r, 16);

        // Chunk (-1,-1) offset (0,0) = tile (-16,-16) - now 16x16 chunks
        assert_eq!(chunk_to_tile(ChunkId(-1, -1), 0, 0).q, -16);
        assert_eq!(chunk_to_tile(ChunkId(-1, -1), 0, 0).r, -16);
    }

    #[test]
    fn test_chunk_to_tile_round_trip() {
        // For any chunk and offset, converting back should give the same chunk
        let chunk = ChunkId(5, -3);
        for offset_q in 0..16 {  // Now 16x16 chunks
            for offset_r in 0..16 {  // Now 16x16 chunks
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
        tiles1.push((Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(crate::components::entity_type::decorator::Decorator { index: 0, is_solid: true })));
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

    #[test]
    fn terrain_chunk_radius_sea_level_returns_base() {
        assert_eq!(terrain_chunk_radius(0), FOV_CHUNK_RADIUS);
    }

    #[test]
    fn terrain_chunk_radius_negative_z_returns_base() {
        assert_eq!(terrain_chunk_radius(-10), FOV_CHUNK_RADIUS);
        assert_eq!(terrain_chunk_radius(-100), FOV_CHUNK_RADIUS);
    }

    #[test]
    fn terrain_chunk_radius_monotonically_increasing() {
        let mut prev = terrain_chunk_radius(0);
        for z in (10..=200).step_by(10) {
            let current = terrain_chunk_radius(z);
            assert!(current >= prev, "radius decreased at z={z}: {prev} -> {current}");
            prev = current;
        }
    }

    #[test]
    fn terrain_chunk_radius_caps_at_max() {
        assert_eq!(terrain_chunk_radius(500), MAX_TERRAIN_CHUNK_RADIUS);
        assert_eq!(terrain_chunk_radius(1000), MAX_TERRAIN_CHUNK_RADIUS);
    }

    #[test]
    fn terrain_chunk_radius_spot_check_known_values() {
        assert_eq!(terrain_chunk_radius(0), 5);
        assert_eq!(terrain_chunk_radius(50), 7);
        assert_eq!(terrain_chunk_radius(100), 9);
        assert_eq!(terrain_chunk_radius(200), MAX_TERRAIN_CHUNK_RADIUS);
    }

    // ── visibility_radius tests ──

    #[test]
    fn visibility_radius_same_elevation_returns_base() {
        for z in [0, 50, 100, 200, 500] {
            assert_eq!(
                visibility_radius(z, z, 40.0), FOV_CHUNK_RADIUS,
                "same elevation z={z} should give base radius"
            );
        }
    }

    #[test]
    fn visibility_radius_ground_zero_matches_old_formula() {
        for z in (0..=300).step_by(10) {
            assert_eq!(
                visibility_radius(z, 0, 40.0),
                elevation_chunk_radius_raw(z),
                "ground_z=0 should match elevation_chunk_radius_raw at z={z}"
            );
        }
    }

    #[test]
    fn visibility_radius_higher_ground_returns_base() {
        assert_eq!(visibility_radius(50, 100, 40.0), FOV_CHUNK_RADIUS);
        assert_eq!(visibility_radius(0, 50, 40.0), FOV_CHUNK_RADIUS);
        assert_eq!(visibility_radius(200, 300, 40.0), FOV_CHUNK_RADIUS);
    }

    #[test]
    fn visibility_radius_monotonic_with_player_z() {
        let mut prev = visibility_radius(0, 0, 40.0);
        for z in (10..=300).step_by(10) {
            let current = visibility_radius(z, 0, 40.0);
            assert!(current >= prev, "radius decreased at z={z}: {prev} -> {current}");
            prev = current;
        }
    }

    #[test]
    fn visibility_radius_spot_checks() {
        // Raw (uncapped) values — terrain_chunk_radius applies the cap
        assert_eq!(visibility_radius(0, 0, 40.0), 5);
        assert_eq!(visibility_radius(50, 0, 40.0), 7);
        assert_eq!(visibility_radius(100, 0, 40.0), 9);
        assert_eq!(visibility_radius(200, 0, 40.0), 13); // capped to 12 by terrain_chunk_radius
    }

    #[test]
    fn visibility_radius_deeper_valley_extends_further() {
        // Looking from z=200 at ground z=0 should see further than at ground z=100
        let deep = visibility_radius(200, 0, 40.0);
        let shallow = visibility_radius(200, 100, 40.0);
        assert!(deep >= shallow, "deeper valley should extend at least as far");
    }

    // ── calculate_visible_chunks_adaptive tests ──

    #[test]
    fn adaptive_flat_world_equals_base_square() {
        let center = ChunkId(5, 5);
        let player_z = 100;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, 40.0, |_, _| player_z,
        );
        // Flat world: all outer chunks have ground=player, visibility_radius=5 < chebyshev,
        // so only the base_radius square is included (split into inner ≤ FOV and outer > FOV)
        let base = calculate_visible_chunks(center, base_radius);
        let all_set: std::collections::HashSet<_> = inner.into_iter().chain(outer).collect();
        let base_set: std::collections::HashSet<_> = base.into_iter().collect();
        assert_eq!(all_set, base_set, "flat world should equal base radius set");
    }

    #[test]
    fn adaptive_sea_level_world_includes_all_visible() {
        let center = ChunkId(0, 0);
        let player_z = 200;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, 40.0, |_, _| 0,
        );
        // Sea-level world: all chunks visible (vis_radius >= max_radius for ground=0)
        let full = calculate_visible_chunks(center, max_radius);
        let all_set: std::collections::HashSet<_> = inner.into_iter().chain(outer).collect();
        let full_set: std::collections::HashSet<_> = full.into_iter().collect();
        assert_eq!(all_set, full_set, "sea-level world should include all chunks up to max_radius");
    }

    #[test]
    fn adaptive_asymmetry_more_chunks_on_low_side() {
        let center = ChunkId(0, 0);
        let player_z = 200;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        // Left side (negative dq) is high terrain, right side is low
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, 40.0,
            |q, _r| if q < center.0 * CHUNK_SIZE { player_z } else { 0 },
        );
        let chunks: Vec<_> = inner.into_iter().chain(outer).collect();

        let left: Vec<_> = chunks.iter().filter(|c| c.0 < center.0).collect();
        let right: Vec<_> = chunks.iter().filter(|c| c.0 > center.0).collect();
        assert!(right.len() > left.len(),
            "low side should have more chunks: left={} right={}", left.len(), right.len());
    }

    #[test]
    fn adaptive_always_superset_of_base() {
        let center = ChunkId(3, -2);
        let player_z = 150;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, 40.0, |_, _| 0,
        );
        let base = calculate_visible_chunks(center, base_radius);
        let all_set: std::collections::HashSet<_> = inner.into_iter().chain(outer).collect();
        for chunk in base {
            assert!(all_set.contains(&chunk), "base chunk {:?} missing from adaptive set", chunk);
        }
    }

    #[test]
    fn adaptive_always_subset_of_max() {
        let center = ChunkId(0, 0);
        let player_z = 200;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, 40.0, |_, _| 50,
        );
        let max_set: std::collections::HashSet<_> =
            calculate_visible_chunks(center, max_radius).into_iter().collect();
        for chunk in inner.into_iter().chain(outer) {
            assert!(max_set.contains(&chunk), "adaptive chunk {:?} outside max radius", chunk);
        }
    }

    #[test]
    fn adaptive_inner_within_fov_radius() {
        let center = ChunkId(0, 0);
        let player_z = 200;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, 40.0, |_, _| 0,
        );
        // All inner chunks must be within FOV_CHUNK_RADIUS
        for chunk in &inner {
            let chebyshev = (chunk.0 - center.0).abs().max((chunk.1 - center.1).abs());
            assert!(chebyshev <= FOV_CHUNK_RADIUS as i32,
                "inner chunk {:?} at distance {} exceeds FOV_CHUNK_RADIUS {}", chunk, chebyshev, FOV_CHUNK_RADIUS);
        }
        // All outer chunks must be beyond FOV_CHUNK_RADIUS
        for chunk in &outer {
            let chebyshev = (chunk.0 - center.0).abs().max((chunk.1 - center.1).abs());
            assert!(chebyshev > FOV_CHUNK_RADIUS as i32,
                "outer chunk {:?} at distance {} within FOV_CHUNK_RADIUS {}", chunk, chebyshev, FOV_CHUNK_RADIUS);
        }
    }

    // ── chunk_max_z tests ──

    #[test]
    fn chunk_max_z_returns_maximum() {
        let chunk = ChunkId(0, 0);
        let max = chunk_max_z(chunk, |q, r| q + r);
        // Max q+r in chunk (0,0) is at tile (15,15) = 30
        assert_eq!(max, 30);
    }

    #[test]
    fn chunk_max_z_constant_height() {
        let chunk = ChunkId(1, 1);
        assert_eq!(chunk_max_z(chunk, |_, _| 42), 42);
    }
}
