use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::components::entity_type::*;

/// Hex chunk radius in tiles. Each chunk is a hex ball of this radius,
/// containing `CHUNK_TILES` tiles (all tiles within hex distance R of center).
pub const CHUNK_RADIUS: i32 = 9;

/// Tile count per hex chunk: 3R(R+1) + 1.
pub const CHUNK_TILES: usize = (3 * CHUNK_RADIUS * (CHUNK_RADIUS + 1) + 1) as usize; // 271

/// Hex distance between adjacent chunk centers in tile coordinates (2R+1).
pub const CHUNK_SPACING: i32 = 2 * CHUNK_RADIUS + 1; // 19

/// Minimum full-detail radius in chunks. `detail_boundary_radius()` never
/// returns less than this, guaranteeing a gameplay-ready area around the player.
pub const FOV_CHUNK_RADIUS: u8 = 5;

/// Legacy alias — no longer caps; kept for API compatibility.
/// Use `terrain_chunk_radius()` which now returns the uncapped frustum radius.
pub const MAX_TERRAIN_CHUNK_RADIUS: u8 = 255;

/// Minimum number of summary-ring chunks beyond the detail boundary.
/// Guarantees an outer LoD ring always exists, even at ground level.
pub const MIN_SUMMARY_RING: u8 = 3;

/// Tile angular-size threshold in pixels. When a single tile subtends
/// fewer than this many pixels on screen, it becomes indistinguishable
/// and we switch to summary LoD.
const TILE_PIXEL_THRESHOLD: f32 = 4.0;

/// Assumed screen height in pixels for angular-size calculations.
const SCREEN_HEIGHT_PX: f32 = 1080.0;

/// Default vertical field of view in radians (15°). Must match `camera.rs::DEFAULT_FOV`.
pub const DEFAULT_FOV: f32 = std::f32::consts::PI / 12.0;

/// Maximum (widest) FOV the player can zoom to (60°). Must match camera.rs MAX.
/// Server uses this to guarantee enough chunks are loaded at any zoom level.
pub const MAX_FOV: f32 = std::f32::consts::PI / 3.0;

// ── Lattice constants for hex-ball tiling ──
//
// Hex balls of radius R tile the plane on the lattice with basis:
//   v1 = (R+1, R),  v2 = (-R, 2R+1)
// The determinant equals CHUNK_TILES, guaranteeing exactly one tile per chunk.
pub const LATTICE_V1: (i32, i32) = (CHUNK_RADIUS + 1, CHUNK_RADIUS);        // (10, 9)
pub const LATTICE_V2: (i32, i32) = (-CHUNK_RADIUS, 2 * CHUNK_RADIUS + 1);   // (-9, 19)
const LATTICE_DET: i32 = CHUNK_TILES as i32;                              // 271

/// World-space Euclidean distance between adjacent chunk centers.
/// Equals √(3 × CHUNK_TILES) × tile_radius (for radius=1 tiles).
/// Used to convert ground distance → chunk count in visibility calculations.
pub const CHUNK_EXTENT_WU: f32 = 28.5; // √813 ≈ 28.513

/// Compute the chunk-loading radius for a player at `player_z` looking at
/// ground at `ground_z`, using the camera's perspective frustum.
///
/// Computes the farthest visible ground point from the camera's actual
/// world-space height (CAMERA_HEIGHT above the player), then converts to
/// chunk count. The player can orbit the camera 360°, so the visible
/// radius is the max ground distance in any direction.
///
/// Returns at least `FOV_CHUNK_RADIUS + MIN_SUMMARY_RING` to guarantee
/// a summary LoD ring always exists.
pub fn visibility_radius(player_z: i32, ground_z: i32, fov: f32) -> u8 {
    let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;

    // Camera geometry (matches camera.rs constants)
    const CAMERA_HEIGHT: f32 = 90.0;
    const CAMERA_DISTANCE: f32 = 120.0;
    const RISE: f32 = 0.8;

    // Camera altitude above the ground plane.
    // Even when ground >= player, camera is still CAMERA_HEIGHT above the player.
    let height_above_ground = CAMERA_HEIGHT + (player_z.max(ground_z) - ground_z) as f32 * RISE;

    // Camera pitch below horizontal: atan2(height, horizontal_distance)
    let pitch = (CAMERA_HEIGHT as f64 / CAMERA_DISTANCE as f64).atan() as f32;

    // Top ray of frustum (shallowest angle, sees farthest)
    let top_ray_angle = pitch - fov * 0.5;

    // If top ray is at or above horizontal, terrain extends to horizon — use a large value
    let max_ground_dist = if top_ray_angle <= 0.01 {
        // Near-horizontal ray: load a lot
        height_above_ground * 20.0
    } else {
        // Ground intercept distance from camera
        let cam_ground_dist = height_above_ground / top_ray_angle.tan();
        // Distance from player = camera ground distance - camera horizontal offset
        (cam_ground_dist - CAMERA_DISTANCE).max(0.0)
    };

    let needed = (max_ground_dist / CHUNK_EXTENT_WU)
        .ceil()
        .min(255.0) as u8;

    needed.max(floor)
}

/// Compute the LoD boundary: the chunk radius at which individual tiles
/// become indistinguishable on screen (subtend fewer than `TILE_PIXEL_THRESHOLD`
/// pixels). Beyond this radius, summary meshes are used instead of full detail.
///
/// Same frustum geometry as `visibility_radius`, but solves for the distance
/// where a single tile's angular size drops below the pixel threshold.
/// Higher elevation pushes the boundary further (camera sees further).
/// Narrower FOV (zoomed in) pushes it further (more pixels per tile).
///
/// Returns at least `FOV_CHUNK_RADIUS` — there's always a guaranteed
/// gameplay-ready area of full detail around the player.
pub fn detail_boundary_radius(player_z: i32, fov: f32) -> u8 {
    const CAMERA_HEIGHT: f32 = 90.0;
    const RISE: f32 = 0.8;
    /// World-space extent of a single hex tile
    const TILE_EXTENT_WU: f32 = 1.5;

    // Camera altitude above the ground plane at player height
    let h = CAMERA_HEIGHT + player_z.max(0) as f32 * RISE;

    // Angular size of one pixel (radians)
    let pixel_angle = fov / SCREEN_HEIGHT_PX;

    let d_sq_max = TILE_EXTENT_WU * h / (pixel_angle * TILE_PIXEL_THRESHOLD);
    let r_sq = d_sq_max - h * h;

    let ground_dist = if r_sq > 0.0 { r_sq.sqrt() } else { 0.0 };

    let chunks = (ground_dist / CHUNK_EXTENT_WU)
        .ceil()
        .min(255.0) as u8;

    chunks.max(FOV_CHUNK_RADIUS)
}

/// Chunk radius needed at a given terrain height at max zoom-out.
///
/// Uses `MAX_FOV` (60°, widest zoom) and `ground_z = 0` (sea level) for
/// worst-case visibility — ensures enough chunks to fill the screen
/// regardless of player's current zoom level.
pub fn elevation_chunk_radius_raw(player_z: i32) -> u8 {
    visibility_radius(player_z, 0, MAX_FOV)
}

/// Base symmetric chunk loading radius (assumes nearby terrain is at player height).
///
/// Used as the always-loaded inner radius in adaptive chunk loading.
/// Returns the floor (`FOV_CHUNK_RADIUS + MIN_SUMMARY_RING`), since when
/// ground is at the player's elevation, the frustum sees nearby terrain only.
/// The extended radius toward valleys comes from `elevation_chunk_radius_raw`.
pub fn terrain_chunk_radius(player_z: i32) -> u8 {
    visibility_radius(player_z, player_z, MAX_FOV)
}

/// Chunk identifier in chunk-coordinate space (lattice coordinates).
///
/// `ChunkId(n, m)` maps to a center tile at `n * v1 + m * v2` where
/// v1 and v2 are the hex-ball tiling lattice basis vectors.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub i32, pub i32);

impl ChunkId {
    /// Get the center tile of this chunk.
    pub fn center(&self) -> Qrz {
        let q = self.0 * LATTICE_V1.0 + self.1 * LATTICE_V2.0;
        let r = self.0 * LATTICE_V1.1 + self.1 * LATTICE_V2.1;
        Qrz { q, r, z: 0 }
    }
}

/// Summary data for a chunk rendered at low detail (outer ring LoD).
///
/// Contains only the representative elevation and biome — no individual tiles.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ChunkSummary {
    pub chunk_id: ChunkId,
    pub elevation: i32,
    pub biome: EntityType,
}

/// A chunk of terrain containing up to CHUNK_TILES tiles (hex ball of radius CHUNK_RADIUS).
#[derive(Clone, Debug)]
pub struct TerrainChunk {
    pub tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 272]>,
}

impl TerrainChunk {
    pub fn new(tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 272]>) -> Self {
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

    /// Memory budget: 100,000 chunks ≈ 2.7 GB (271 tiles × ~100 bytes each)
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

/// Hex distance between two chunks in axial coordinates.
///
/// Same formula as `Qrz::flat_distance`: max(|dq|, |dr|, |dq + dr|).
/// This produces a regular hexagonal region in world space, unlike
/// Chebyshev distance which produces a skewed parallelogram.
pub fn chunk_hex_distance(a: ChunkId, b: ChunkId) -> i32 {
    let dq = a.0 - b.0;
    let dr = a.1 - b.1;
    (dq.abs()).max(dr.abs()).max((dq + dr).abs())
}

/// Convert a Loc (Qrz) to its containing chunk ID.
///
/// Uses the hex-ball tiling lattice: computes fractional lattice coordinates
/// via the inverse basis matrix, then checks the 4 nearest lattice points
/// to find the one whose center is closest in hex distance.
pub fn loc_to_chunk(loc: Qrz) -> ChunkId {
    let q = loc.q;
    let r = loc.r;
    let det = LATTICE_DET as f64;

    // Inverse lattice transform: (n, m) = M^{-1} · (q, r)
    // M = [[R+1, -R], [R, 2R+1]], M^{-1} = (1/det) * [[2R+1, R], [-R, R+1]]
    let nf = ((2 * CHUNK_RADIUS + 1) as f64 * q as f64 + CHUNK_RADIUS as f64 * r as f64) / det;
    let mf = (-(CHUNK_RADIUS as f64) * q as f64 + (CHUNK_RADIUS + 1) as f64 * r as f64) / det;

    // Check 4 nearest lattice points, pick closest center in hex distance
    let n0 = nf.floor() as i32;
    let m0 = mf.floor() as i32;
    let mut best_n = n0;
    let mut best_m = m0;
    let mut best_dist = i32::MAX;

    for dn in 0..=1 {
        for dm in 0..=1 {
            let n = n0 + dn;
            let m = m0 + dm;
            let cq = n * LATTICE_V1.0 + m * LATTICE_V2.0;
            let cr = n * LATTICE_V1.1 + m * LATTICE_V2.1;
            let dq = q - cq;
            let dr = r - cr;
            let dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
            if dist < best_dist || (dist == best_dist && (n, m) < (best_n, best_m)) {
                best_dist = dist;
                best_n = n;
                best_m = m;
            }
        }
    }

    ChunkId(best_n, best_m)
}

/// Iterate all tiles in a hex chunk (hex ball of radius CHUNK_RADIUS around center).
/// Yields exactly CHUNK_TILES `(q, r)` pairs.
pub fn chunk_tiles(chunk_id: ChunkId) -> impl Iterator<Item = (i32, i32)> {
    let center = chunk_id.center();
    let cq = center.q;
    let cr = center.r;
    let r = CHUNK_RADIUS;
    (-r..=r).flat_map(move |dq| {
        let dr_min = (-r).max(-dq - r);
        let dr_max = r.min(-dq + r);
        (dr_min..=dr_max).map(move |dr| (cq + dq, cr + dr))
    })
}

/// Check if a tile (q, r) is within a specific chunk's hex ball.
pub fn tile_in_chunk(q: i32, r: i32, chunk_id: ChunkId) -> bool {
    let center = chunk_id.center();
    let dq = q - center.q;
    let dr = r - center.r;
    dq.abs().max(dr.abs()).max((dq + dr).abs()) <= CHUNK_RADIUS
}

/// Calculate visible chunks based on FOV distance
pub fn calculate_visible_chunks(center: ChunkId, radius: u8) -> Vec<ChunkId> {
    let mut visible = Vec::new();
    let r = radius as i32;

    // Generate a hex-shaped region of chunks around the center.
    // Hex range in axial: dq in [-r, r], dr in [max(-r, -dq-r), min(r, -dq+r)]
    for dq in -r..=r {
        let dr_min = (-r).max(-dq - r);
        let dr_max = r.min(-dq + r);
        for dr in dr_min..=dr_max {
            visible.push(ChunkId(center.0 + dq, center.1 + dr));
        }
    }

    visible
}

/// Calculate visible chunks using per-chunk elevation-aware filtering.
///
/// Returns `(inner, outer)` where:
/// - `inner`: chunks within `detail_radius` (full tile detail)
/// - `outer`: chunks beyond `detail_radius` that pass visibility (summary LoD)
///
/// `detail_radius` is the LoD boundary — typically from `detail_boundary_radius()`.
/// Chunks within `base_radius` are always included (skip height query).
/// Outer chunks beyond `base_radius` are included only if their ground
/// elevation puts them within the camera's visible range from `player_z`.
///
/// `height_fn(q, r)` returns terrain height at tile (q, r).
pub fn calculate_visible_chunks_adaptive(
    center: ChunkId,
    player_z: i32,
    base_radius: u8,
    max_radius: u8,
    detail_radius: u8,
    fov: f32,
    height_fn: impl Fn(i32, i32) -> i32,
) -> (Vec<ChunkId>, Vec<ChunkId>) {
    let mut inner = Vec::new();
    let mut outer = Vec::new();
    let r = max_radius as i32;
    let base = base_radius as i32;
    let detail = detail_radius as i32;

    for dq in -r..=r {
        let dr_min = (-r).max(-dq - r);
        let dr_max = r.min(-dq + r);
        for dr in dr_min..=dr_max {
            let hex_dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
            let chunk_id = ChunkId(center.0 + dq, center.1 + dr);

            // Determine if chunk is visible
            let included = if hex_dist <= base {
                true // Always include within base radius
            } else {
                let center_tile = chunk_id.center();
                let chunk_z = height_fn(center_tile.q, center_tile.r);
                let vis_radius = visibility_radius(player_z, chunk_z, fov) as i32;
                hex_dist <= vis_radius
            };

            if included {
                if hex_dist <= detail + 1 {
                    inner.push(chunk_id);
                }
                if hex_dist >= detail {
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
    chunk_tiles(chunk_id)
        .map(|(q, r)| height_fn(q, r))
        .max()
        .unwrap_or(i32::MIN)
}

/// Check if a location (Qrz) is within a specific chunk
pub fn is_loc_in_chunk(loc: Qrz, chunk_id: ChunkId) -> bool {
    loc_to_chunk(loc) == chunk_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_tiles_correct_count() {
        let count = chunk_tiles(ChunkId(0, 0)).count();
        assert_eq!(count, CHUNK_TILES, "hex ball of radius {CHUNK_RADIUS} should have {CHUNK_TILES} tiles");
    }

    #[test]
    fn chunk_center_is_in_chunk() {
        let chunk = ChunkId(3, -2);
        let center = chunk.center();
        assert_eq!(loc_to_chunk(center), chunk);
    }

    #[test]
    fn roundtrip_all_tiles_in_chunk() {
        // Every tile yielded by chunk_tiles should map back to the same chunk
        for &chunk in &[ChunkId(0, 0), ChunkId(1, 0), ChunkId(0, 1), ChunkId(-1, 2), ChunkId(5, -3)] {
            for (q, r) in chunk_tiles(chunk) {
                let recovered = loc_to_chunk(Qrz { q, r, z: 0 });
                assert_eq!(recovered, chunk,
                    "tile ({q},{r}) in chunk {chunk:?} mapped to {recovered:?}");
            }
        }
    }

    #[test]
    fn no_tile_in_two_chunks() {
        // Adjacent chunks should not share any tiles
        let c0 = ChunkId(0, 0);
        let tiles_0: HashSet<_> = chunk_tiles(c0).collect();

        // Check all 6 hex neighbors of c0
        for &(dn, dm) in &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let c1 = ChunkId(dn, dm);
            for (q, r) in chunk_tiles(c1) {
                assert!(!tiles_0.contains(&(q, r)),
                    "tile ({q},{r}) is in both chunk {c0:?} and {c1:?}");
            }
        }
    }

    #[test]
    fn tile_in_chunk_matches_iteration() {
        let chunk = ChunkId(2, -1);
        let tiles: HashSet<_> = chunk_tiles(chunk).collect();
        let center = chunk.center();
        // Check a region around the center
        for dq in -12..=12 {
            for dr in -12..=12 {
                let q = center.q + dq;
                let r = center.r + dr;
                assert_eq!(tile_in_chunk(q, r, chunk), tiles.contains(&(q, r)),
                    "tile_in_chunk disagrees with chunk_tiles for ({q},{r}) in {chunk:?}");
            }
        }
    }

    #[test]
    fn lattice_determinant_equals_chunk_tiles() {
        let det = LATTICE_V1.0 * LATTICE_V2.1 - LATTICE_V1.1 * LATTICE_V2.0;
        assert_eq!(det, CHUNK_TILES as i32);
    }

    #[test]
    fn adjacent_chunk_centers_at_correct_distance() {
        let c0 = ChunkId(0, 0).center();
        // All 6 chunk neighbors should have centers at hex distance CHUNK_SPACING
        for &(dn, dm) in &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
            let c1 = ChunkId(dn, dm).center();
            let dist = c0.flat_distance(&c1);
            assert_eq!(dist, CHUNK_SPACING,
                "chunk ({dn},{dm}) center at hex distance {dist}, expected {CHUNK_SPACING}");
        }
    }

    #[test]
    fn test_calculate_visible_chunks_radius_0() {
        let center = ChunkId(0, 0);
        let visible = calculate_visible_chunks(center, 0);
        assert_eq!(visible.len(), 1);
        assert!(visible.contains(&ChunkId(0, 0)));
    }

    #[test]
    fn test_calculate_visible_chunks_radius_1() {
        let center = ChunkId(0, 0);
        let visible = calculate_visible_chunks(center, 1);

        // Hex radius 1 = 1 center + 6 neighbors = 7 chunks
        assert_eq!(visible.len(), 7);
        assert!(visible.contains(&ChunkId(0, 0)));
        assert!(visible.contains(&ChunkId(1, 0)));
        assert!(visible.contains(&ChunkId(-1, 0)));
        assert!(visible.contains(&ChunkId(0, 1)));
        assert!(visible.contains(&ChunkId(0, -1)));
        assert!(visible.contains(&ChunkId(1, -1)));
        assert!(visible.contains(&ChunkId(-1, 1)));
        assert!(!visible.contains(&ChunkId(-1, -1)));
        assert!(!visible.contains(&ChunkId(1, 1)));
    }

    #[test]
    fn test_calculate_visible_chunks_radius_2() {
        let center = ChunkId(10, -5);
        let visible = calculate_visible_chunks(center, 2);

        // Hex radius 2 = 1 + 6 + 12 = 19 chunks
        assert_eq!(visible.len(), 19);
        assert!(visible.contains(&ChunkId(10, -5)));  // center
        assert!(visible.contains(&ChunkId(10, -7)));  // edge (dr=-2)
        assert!(visible.contains(&ChunkId(12, -5)));  // edge (dq=+2)
        assert!(visible.contains(&ChunkId(12, -7)));  // hex dist 2: max(2,2,0)=2
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
        cache.max_chunks = 3;

        let chunk1 = ChunkId(0, 0);
        let chunk2 = ChunkId(1, 1);
        let chunk3 = ChunkId(2, 2);
        let chunk4 = ChunkId(3, 3);

        let mut tiles1 = tinyvec::ArrayVec::new();
        tiles1.push((Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(crate::components::entity_type::decorator::Decorator { index: 0, is_solid: true })));
        let tiles2 = tiles1.clone();
        let tiles3 = tiles1.clone();
        let tiles4 = tiles1.clone();

        cache.chunks.insert(chunk1, Arc::new(TerrainChunk::new(tiles1)));
        cache.access_order.put(chunk1, ());
        cache.chunks.insert(chunk2, Arc::new(TerrainChunk::new(tiles2)));
        cache.access_order.put(chunk2, ());
        cache.chunks.insert(chunk3, Arc::new(TerrainChunk::new(tiles3)));
        cache.access_order.put(chunk3, ());

        assert_eq!(cache.chunks.len(), 3);

        if cache.chunks.len() >= cache.max_chunks {
            if let Some((evicted_id, _)) = cache.access_order.pop_lru() {
                cache.chunks.remove(&evicted_id);
            }
        }

        cache.chunks.insert(chunk4, Arc::new(TerrainChunk::new(tiles4)));
        cache.access_order.put(chunk4, ());

        assert_eq!(cache.chunks.len(), 3);
        assert!(!cache.chunks.contains_key(&chunk1));
        assert!(cache.chunks.contains_key(&chunk2));
        assert!(cache.chunks.contains_key(&chunk3));
        assert!(cache.chunks.contains_key(&chunk4));
    }

    #[test]
    fn terrain_chunk_radius_sea_level_with_max_zoom() {
        let r = terrain_chunk_radius(0);
        assert!(r >= 15, "ground level at max zoom should need many chunks, got {r}");
    }

    #[test]
    fn terrain_chunk_radius_negative_z_same_as_zero() {
        assert_eq!(terrain_chunk_radius(-10), terrain_chunk_radius(0));
        assert_eq!(terrain_chunk_radius(-100), terrain_chunk_radius(0));
    }

    #[test]
    fn elevation_chunk_radius_raw_monotonically_increasing() {
        let mut prev = elevation_chunk_radius_raw(0);
        for z in (10..=200).step_by(10) {
            let current = elevation_chunk_radius_raw(z);
            assert!(current >= prev, "radius decreased at z={z}: {prev} -> {current}");
            prev = current;
        }
    }

    #[test]
    fn elevation_chunk_radius_raw_grows_with_elevation() {
        assert!(elevation_chunk_radius_raw(500) > elevation_chunk_radius_raw(200));
        assert!(elevation_chunk_radius_raw(1000) > elevation_chunk_radius_raw(500));
    }

    #[test]
    fn terrain_chunk_radius_constant_for_same_elevation() {
        let base = terrain_chunk_radius(0);
        for z in [50, 100, 200, 500] {
            assert_eq!(terrain_chunk_radius(z), base,
                "terrain_chunk_radius({z}) should equal base {base}");
        }
    }

    // ── visibility_radius tests ──

    #[test]
    fn visibility_radius_same_elevation_returns_floor() {
        let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;
        for z in [0, 50, 100, 200, 500] {
            assert_eq!(
                visibility_radius(z, z, DEFAULT_FOV), floor,
                "same elevation z={z} should give floor radius"
            );
        }
    }

    #[test]
    fn visibility_radius_ground_zero_max_fov_matches_raw() {
        for z in (0..=300).step_by(10) {
            assert_eq!(
                visibility_radius(z, 0, MAX_FOV),
                elevation_chunk_radius_raw(z),
                "ground_z=0 with MAX_FOV should match elevation_chunk_radius_raw at z={z}"
            );
        }
    }

    #[test]
    fn visibility_radius_higher_ground_returns_floor() {
        let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;
        assert_eq!(visibility_radius(50, 100, DEFAULT_FOV), floor);
        assert_eq!(visibility_radius(0, 50, DEFAULT_FOV), floor);
        assert_eq!(visibility_radius(200, 300, DEFAULT_FOV), floor);
    }

    #[test]
    fn visibility_radius_monotonic_with_player_z() {
        let mut prev = visibility_radius(0, 0, DEFAULT_FOV);
        for z in (10..=300).step_by(10) {
            let current = visibility_radius(z, 0, DEFAULT_FOV);
            assert!(current >= prev, "radius decreased at z={z}: {prev} -> {current}");
            prev = current;
        }
    }

    #[test]
    fn visibility_radius_spot_checks() {
        assert_eq!(visibility_radius(0, 0, DEFAULT_FOV), 8);
        assert_eq!(visibility_radius(50, 0, DEFAULT_FOV), 8);
        assert_eq!(visibility_radius(100, 0, DEFAULT_FOV), 8);
    }

    #[test]
    fn visibility_radius_wider_fov_loads_more() {
        let normal = visibility_radius(200, 0, DEFAULT_FOV);
        let wide = visibility_radius(200, 0, DEFAULT_FOV * 2.0);
        assert!(wide > normal, "wider FOV should need more chunks: normal={normal} wide={wide}");
    }

    #[test]
    fn visibility_radius_never_below_floor() {
        let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;
        for fov_mult in [0.5_f32, 1.0, 2.0, 4.0] {
            let r = visibility_radius(0, 0, DEFAULT_FOV * fov_mult);
            assert!(r >= floor, "fov_mult={fov_mult} gave radius {r} below floor {floor}");
        }
    }

    #[test]
    fn visibility_radius_deeper_valley_extends_further() {
        let deep = visibility_radius(200, 0, DEFAULT_FOV);
        let shallow = visibility_radius(200, 100, DEFAULT_FOV);
        assert!(deep >= shallow, "deeper valley should extend at least as far");
    }

    // ── detail_boundary_radius tests ──

    #[test]
    fn detail_boundary_never_below_fov_chunk_radius() {
        for z in [0, 50, 100, 200, 500] {
            for fov_deg in [6.0_f32, 15.0, 30.0, 60.0] {
                let fov = fov_deg.to_radians();
                let r = detail_boundary_radius(z, fov);
                assert!(r >= FOV_CHUNK_RADIUS,
                    "detail_boundary_radius({z}, {fov_deg}°) = {r} below floor {FOV_CHUNK_RADIUS}");
            }
        }
    }

    #[test]
    fn detail_boundary_narrower_fov_extends_further() {
        let narrow = detail_boundary_radius(0, 6_f32.to_radians());
        let wide = detail_boundary_radius(0, 60_f32.to_radians());
        assert!(narrow >= wide,
            "narrower FOV should extend detail at least as far: narrow={narrow} wide={wide}");
    }

    #[test]
    fn detail_boundary_higher_elevation_extends_further() {
        let low = detail_boundary_radius(0, DEFAULT_FOV);
        let high = detail_boundary_radius(200, DEFAULT_FOV);
        assert!(high >= low,
            "higher elevation should extend detail at least as far: low={low} high={high}");
    }

    #[test]
    fn detail_boundary_monotonic_with_elevation() {
        let fov = DEFAULT_FOV;
        let mut prev = detail_boundary_radius(0, fov);
        for z in (10..=300).step_by(10) {
            let current = detail_boundary_radius(z, fov);
            assert!(current >= prev,
                "detail boundary decreased at z={z}: {prev} -> {current}");
            prev = current;
        }
    }

    // ── calculate_visible_chunks_adaptive tests ──

    #[test]
    fn adaptive_flat_world_equals_base_hex() {
        let center = ChunkId(5, 5);
        let player_z = 100;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let detail_radius = detail_boundary_radius(player_z, MAX_FOV);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, detail_radius, MAX_FOV, |_, _| player_z,
        );
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
        let detail_radius = detail_boundary_radius(player_z, MAX_FOV);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, detail_radius, MAX_FOV, |_, _| 0,
        );
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
        let detail_radius = detail_boundary_radius(player_z, MAX_FOV);
        let center_tile = center.center();
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, detail_radius, MAX_FOV,
            |q, _r| if q < center_tile.q { player_z } else { 0 },
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
        let detail_radius = detail_boundary_radius(player_z, MAX_FOV);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, detail_radius, MAX_FOV, |_, _| 0,
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
        let detail_radius = detail_boundary_radius(player_z, MAX_FOV);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, detail_radius, MAX_FOV, |_, _| 50,
        );
        let max_set: std::collections::HashSet<_> =
            calculate_visible_chunks(center, max_radius).into_iter().collect();
        for chunk in inner.into_iter().chain(outer) {
            assert!(max_set.contains(&chunk), "adaptive chunk {:?} outside max radius", chunk);
        }
    }

    #[test]
    fn adaptive_inner_within_detail_radius() {
        let center = ChunkId(0, 0);
        let player_z = 200;
        let base_radius = terrain_chunk_radius(player_z);
        let max_radius = elevation_chunk_radius_raw(player_z);
        let detail_radius = detail_boundary_radius(player_z, MAX_FOV);
        let (inner, outer) = calculate_visible_chunks_adaptive(
            center, player_z, base_radius, max_radius, detail_radius, MAX_FOV, |_, _| 0,
        );
        for chunk in &inner {
            let dist = chunk_hex_distance(*chunk, center);
            assert!(dist <= detail_radius as i32,
                "inner chunk {:?} at hex distance {} exceeds detail_radius {}", chunk, dist, detail_radius);
        }
        for chunk in &outer {
            let dist = chunk_hex_distance(*chunk, center);
            assert!(dist > detail_radius as i32,
                "outer chunk {:?} at hex distance {} within detail_radius {}", chunk, dist, detail_radius);
        }
    }

    // ── chunk_max_z tests ──

    #[test]
    fn chunk_max_z_returns_maximum() {
        let chunk = ChunkId(0, 0);
        let max = chunk_max_z(chunk, |q, r| q + r);
        // Max q+r in hex ball around center (0,0) at radius 9: q=9, r=0 → 9
        // or q=0, r=9 → 9, or any combo where q+r is maximized within the ball
        // Max of q+r subject to max(|q|,|r|,|q+r|) ≤ 9 is q+r = 9
        assert_eq!(max, 9);
    }

    #[test]
    fn chunk_max_z_constant_height() {
        let chunk = ChunkId(1, 1);
        assert_eq!(chunk_max_z(chunk, |_, _| 42), 42);
    }
}
