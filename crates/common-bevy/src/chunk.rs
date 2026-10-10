use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::components::entity_type::*;
use crate::geometry::hex_ball_tiles;

/// Hex chunk radius in tiles. Each chunk is a hex ball of this radius,
/// containing `CHUNK_TILES` tiles (all tiles within hex distance R of center).
pub const CHUNK_RADIUS: i32 = 9;

/// Tile count per hex chunk: the hex ball of `CHUNK_RADIUS`.
pub const CHUNK_TILES: usize = hex_ball_tiles(CHUNK_RADIUS as u32) as usize;

/// Hex distance between adjacent chunk centers in tile coordinates (2R+1).
pub const CHUNK_SPACING: i32 = 2 * CHUNK_RADIUS + 1; // 19

/// Minimum full-detail radius in chunks: the gameplay-ready area around
/// the player that `visibility_radius` never goes below.
pub const FOV_CHUNK_RADIUS: u8 = 5;

/// Minimum number of summary-ring chunks beyond the detail boundary.
/// Guarantees an outer LoD ring always exists, even at ground level.
pub const MIN_SUMMARY_RING: u8 = 3;

/// Fixed full-detail streaming radius in chunks (hex distance).
/// All chunks within this radius receive full tile data from the server.
/// Beyond this, the server sends lightweight summary data instead.
/// `summary::BAND_QUALITY_K` anchors the band ladder to it, so the bands
/// the client's Map can build end exactly where the stream does.
pub const FIXED_STREAM_RADIUS: u8 = 21;

/// World-unit extent of the fixed streaming radius.

/// NOTE: this is the chunk hexball's CIRCUMRADIUS (corner-direction extent).
/// A hex-distance-21 chunk set is a hexagon: it reaches this far only along
/// the six corner directions; along edge directions it ends at the apothem
/// (×√3/2). Use `FIXED_STREAM_APOTHEM_WU` for "Map data is guaranteed to
/// exist within this circle" decisions.
pub const FIXED_STREAM_RADIUS_WU: f32 = FIXED_STREAM_RADIUS as f32 * CHUNK_EXTENT_WU;

/// Inscribed radius of the streamed-chunk hexagon — the largest circle
/// fully inside guaranteed chunk coverage. Ownership decisions (Map-built
/// vs producer-built summary regions) must use this, not the circumradius:
/// along the six edge directions coverage ends here, and a claim out to the
/// circumradius leaves a crescent no one builds in each of those lobes.
pub const FIXED_STREAM_APOTHEM_WU: f32 = FIXED_STREAM_RADIUS_WU * APOTHEM_FACTOR;

/// √3/2 — hexagon apothem / circumradius ratio.
pub const APOTHEM_FACTOR: f32 = 0.866_025_4;

/// Maximum (widest) FOV the player can zoom to (60°). Must match camera.rs MAX.
/// Server uses this to guarantee enough chunks are loaded at any zoom level.
pub const MAX_FOV: f32 = std::f32::consts::PI / 3.0;

// ── Lattice constants for hex-ball tiling ──

// Hex balls of radius R tile the plane on the lattice with basis:
//   v1 = (R+1, R),  v2 = (-R, 2R+1)
// The determinant equals CHUNK_TILES, guaranteeing exactly one tile per chunk.
const LATTICE_V1: (i32, i32) = (CHUNK_RADIUS + 1, CHUNK_RADIUS);        // (10, 9)
const LATTICE_V2: (i32, i32) = (-CHUNK_RADIUS, 2 * CHUNK_RADIUS + 1);   // (-9, 19)
const LATTICE_DET: i32 = CHUNK_TILES as i32;                              // 271

/// World-space Euclidean distance between adjacent chunk centers.
/// Equals √(3 × CHUNK_TILES) × tile_radius (for radius=1 tiles).
/// Used to convert ground distance → chunk count in visibility calculations.
pub const CHUNK_EXTENT_WU: f32 = 28.5; // √813 ≈ 28.513

/// Compute the chunk-loading radius for a player at `player_z` looking at
/// ground at `ground_z`, using the camera's perspective frustum.

/// Computes the farthest visible ground point from the camera's actual
/// world-space height (CAMERA_HEIGHT above the player), then converts to
/// chunk count. The player can orbit the camera 360°, so the visible
/// radius is the max ground distance in any direction.

/// Returns at least `FOV_CHUNK_RADIUS + MIN_SUMMARY_RING` to guarantee
/// a summary LoD ring always exists.
pub fn visibility_radius(player_z: i32, ground_z: i32, fov: f32) -> u8 {
    let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;

    let cam_h = common::camera::camera_height(fov);
    let height_above_ground = cam_h + (player_z.max(ground_z) - ground_z) as f32 * common::grid::RISE;

    // Camera pitch below horizontal: atan2(height, horizontal_distance)
    let pitch = (cam_h as f64 / common::camera::CAMERA_DISTANCE as f64).atan() as f32;

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
        (cam_ground_dist - common::camera::CAMERA_DISTANCE).max(0.0)
    };

    let needed = (max_ground_dist / CHUNK_EXTENT_WU)
        .ceil()
        .min(255.0) as u8;

    needed.max(floor)
}

/// Chunk loading radius for a player at `player_z` with the ground around
/// it at its own height, at the widest zoom (`MAX_FOV`): the same at every
/// height, since only the drop to the ground extends the frustum's reach.
pub fn terrain_chunk_radius(player_z: i32) -> u8 {
    visibility_radius(player_z, player_z, MAX_FOV)
}

/// Chunk identifier in chunk-coordinate space (lattice coordinates).

/// `ChunkId(n, m)` maps to a center tile at `n * v1 + m * v2` where
/// v1 and v2 are the hex-ball tiling lattice basis vectors.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub i32, pub i32);

impl ChunkId {
    /// Get the center tile of this chunk.
    pub fn center(&self) -> Qrz {
        let q = self.0 * LATTICE_V1.0 + self.1 * LATTICE_V2.0;
        let r = self.0 * LATTICE_V1.1 + self.1 * LATTICE_V2.1;
        Qrz { q, r, z: 0 }
    }
}

/// A chunk of terrain containing up to CHUNK_TILES tiles (hex ball of radius CHUNK_RADIUS).
/// Each tile is its position, its type, and the surface water stands at over
/// it, or None where it is dry.
#[derive(Clone, Debug)]
pub struct TerrainChunk {
    pub tiles: tinyvec::ArrayVec<[(Qrz, EntityType, Option<i32>); 272]>,
}

impl TerrainChunk {
    pub fn new(tiles: tinyvec::ArrayVec<[(Qrz, EntityType, Option<i32>); 272]>) -> Self {
        Self {
            tiles,
        }
    }
}

/// Per-player discovery state: the chunk its stream was last computed for.
#[derive(Component, Debug, Default)]
pub struct PlayerDiscoveryState {
    /// Last chunk position (for delta detection)
    pub last_chunk: Option<ChunkId>,
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

/// Hex distance between two chunks in axial coordinates
/// (`qrz::hex_distance`): a regular hexagonal region in world space, where
/// Chebyshev distance would give a skewed parallelogram.
pub fn chunk_hex_distance(a: ChunkId, b: ChunkId) -> i32 {
    qrz::hex_distance((a.0, a.1), (b.0, b.1))
}

/// Convert a Loc (Qrz) to its containing chunk ID.

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
            let dist = qrz::hex_distance((q, r), (cq, cr));
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

/// **Protocol-critical**: ChunkData omits (q, r) on the wire — the receiver
/// reconstructs coordinates by zipping with this iterator. Changing the
/// iteration order is a breaking network protocol change.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The camera's default vertical field of view, 15°, as `camera.rs`
    /// sets it.
    const DEFAULT_FOV: f32 = std::f32::consts::PI / 12.0;

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
    fn terrain_chunk_radius_sea_level_with_max_zoom() {
        let r = terrain_chunk_radius(0);
        let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;
        assert!(r > floor, "at max zoom the frustum, not the floor, sets the radius: got {r} against {floor}");
    }

    #[test]
    fn terrain_chunk_radius_negative_z_same_as_zero() {
        assert_eq!(terrain_chunk_radius(-10), terrain_chunk_radius(0));
        assert_eq!(terrain_chunk_radius(-100), terrain_chunk_radius(0));
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
    fn visibility_radius_grows_with_the_drop_to_the_ground() {
        let floor = FOV_CHUNK_RADIUS + MIN_SUMMARY_RING;
        let level = visibility_radius(0, 0, DEFAULT_FOV);
        let raised = visibility_radius(50, 0, DEFAULT_FOV);
        let high = visibility_radius(100, 0, DEFAULT_FOV);
        assert_eq!(level, floor, "on level ground the floor dominates");
        assert!(floor < raised && raised < high, "a shallow pitch sees further down to the sea: {raised} < {high}");
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
}
