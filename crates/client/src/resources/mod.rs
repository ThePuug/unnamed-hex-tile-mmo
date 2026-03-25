use bevy::{
    prelude::*,
    pbr::{ExtendedMaterial, MaterialExtension},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use common_bevy::chunk::ChunkId;

/// Custom terrain material extension that computes elevation color in the fragment shader.
/// Atmospheric fade is derived from the view's camera position (no custom uniforms needed).
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct TerrainExtension {}

impl MaterialExtension for TerrainExtension {
    fn vertex_shader() -> ShaderRef {
        "shaders/terrain_vertex.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
}

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct EntityMap(BiMap<Entity,Entity>);

#[derive(Debug, Resource)]
pub struct Server {
    /// Server's game world time when Init event was received
    pub server_time_at_init: u128,
    /// Client's elapsed time when Init event was received
    pub client_time_at_init: u128,
    /// Last time we sent a ping (for periodic pings)
    pub last_ping_time: u128,
    /// Smoothed network latency estimate (exponential moving average)
    pub smoothed_latency: u128,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            server_time_at_init: 0,
            client_time_at_init: 0,
            last_ping_time: 0,
            smoothed_latency: 50, // Initial estimate: 50ms
        }
    }
}

impl Server {
    /// Calculate the current game world time (used for both threats and day/night)
    /// Game world time = server_time_at_init + (client_now - client_at_init)
    pub fn current_time(&self, client_now: u128) -> u128 {
        let time_since_init = client_now.saturating_sub(self.client_time_at_init);
        self.server_time_at_init.saturating_add(time_since_init)
    }
}

use bevy::tasks::Task;

/// Shared material for all chunk meshes (elevation color computed in shader)
#[derive(Resource)]
pub struct TerrainMaterial {
    pub handle: Handle<ExtendedMaterial<StandardMaterial, TerrainExtension>>,
}

/// Chunks whose appearance should NOT trigger neighbor mesh regeneration.
/// When the admin flyover generates all chunks (including a buffer zone) at once,
/// the mesh pipeline already has correct neighbor data — no cascade needed.
#[derive(Debug, Default, Resource)]
pub struct SkipNeighborRegen {
    pub chunks: HashSet<ChunkId>,
}

/// Per-chunk hex-native decimation mesh state.
/// Result from async mesh build task: the mesh plus diagnostic counts.
pub struct MeshBuildResult {
    pub mesh: Mesh,
    /// Triangle count of the decimated mesh.
    pub tri_count: u32,
    /// Triangle count if rendered at full detail (no decimation, with skirts).
    pub full_detail_tris: u32,
    /// Perimeter edges for cross-chunk skirt stitching with neighbors.
    pub perimeter: common_bevy::hexball_geometry::ChunkPerimeterEdges,
}

pub struct ChunkLodState {
    /// In-flight async task producing a decimated Mesh.
    pub task: Option<Task<MeshBuildResult>>,
    /// Entity displaying this chunk's mesh.
    pub entity: Option<Entity>,
    /// Handle to the active mesh asset (for grid overlay extraction).
    pub mesh_handle: Option<Handle<Mesh>>,
    /// Perimeter edges for cross-chunk skirt stitching.
    pub perimeter: Option<common_bevy::hexball_geometry::ChunkPerimeterEdges>,
    /// Triangle count of the active mesh (for diagnostics).
    pub tri_count: u32,
    /// Triangle count if rendered at full detail (for compression ratio).
    pub full_detail_tris: u32,
    /// Decimation threshold the current mesh was built at.
    pub current_threshold: u32,
    /// Chunk-local origin (world position of chunk center tile).
    /// Mesh vertex positions are relative to this; entity Transform repositions.
    pub chunk_origin: Vec3,
}

/// Tracks hex-native decimation mesh state for all chunks.
#[derive(Resource, Default)]
pub struct ChunkLodMeshes {
    pub states: HashMap<ChunkId, ChunkLodState>,
}

/// Per-threshold-tier statistics.
#[derive(Clone, Copy, Default)]
pub struct TierStats {
    pub chunks: u32,
    pub tris: u64,
    /// What the triangle count would be at full detail (no decimation).
    pub full_detail_tris: u64,
}

/// LoD triangle statistics grouped by threshold tier.
#[derive(Resource, Default)]
pub struct LodTriangleStats {
    /// Stats per threshold tier. Index = threshold value.
    /// Tiers beyond the vec length have zero chunks.
    pub tiers: Vec<TierStats>,
    /// Total triangles across all tiers.
    pub total_tris: u64,
    /// Total chunks with active meshes.
    pub mesh_count: u32,
}

/// Tracks which chunks have been received on the client
#[derive(Debug, Default, Resource)]
pub struct LoadedChunks {
    pub chunks: HashSet<ChunkId>,
}

impl LoadedChunks {
    /// Mark a chunk as loaded
    pub fn insert(&mut self, chunk_id: ChunkId) {
        self.chunks.insert(chunk_id);
    }

    /// Remove evicted chunks from tracking
    pub fn evict(&mut self, chunk_ids: &[ChunkId]) {
        for chunk_id in chunk_ids {
            self.chunks.remove(chunk_id);
        }
    }
}

/// Client-side system timers. Wraps `common::timers::SystemTimers`.
/// No transport — data accumulates locally. Can be drained for diagnostics.
#[derive(Resource)]
pub struct ClientTimers(pub common::timers::SystemTimers);

impl Default for ClientTimers {
    fn default() -> Self { Self(common::timers::SystemTimers::new()) }
}
