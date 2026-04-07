use bevy::{
    prelude::*,
    pbr::{ExtendedMaterial, MaterialExtension},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use common_bevy::chunk::ChunkId;
use common_bevy::summary_mesh::MeshRegionKey;

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

/// Result from async mesh build task (legacy — used by diagnostics grid overlay).
#[allow(dead_code)]
pub struct MeshBuildResult {
    pub mesh: Mesh,
    pub tri_count: u32,
}

/// Per-chunk mesh state (legacy — used by diagnostics grid overlay).
#[allow(dead_code)]
pub struct ChunkLodState {
    /// In-flight async task producing a Mesh.
    pub task: Option<Task<MeshBuildResult>>,
    /// Entity displaying this chunk's mesh.
    pub entity: Option<Entity>,
    /// Handle to the active mesh asset (for grid overlay extraction).
    pub mesh_handle: Option<Handle<Mesh>>,
    /// Triangle count of the active mesh (for diagnostics).
    pub tri_count: u32,
    /// Chunk-local origin (world position of chunk center tile).
    /// Mesh vertex positions are relative to this; entity Transform repositions.
    pub chunk_origin: Vec3,
}

/// Tracks mesh state for all chunks.
#[derive(Resource, Default)]
pub struct ChunkLodMeshes {
    pub states: HashMap<ChunkId, ChunkLodState>,
}

/// Triangle statistics.
#[derive(Resource, Default)]
pub struct LodTriangleStats {
    /// Total triangles across all meshes.
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

/// Forced summary radius for flyover inspection.
///
/// `None` = auto (use r(d) formula; currently falls back to tile meshes).
/// `Some(0)` = individual tiles everywhere (existing pipeline, parity test).
/// `Some(r)` = all terrain at summary radius r.
#[derive(Resource)]
pub struct ForcedSummaryRadius(pub Option<u32>);

impl Default for ForcedSummaryRadius {
    fn default() -> Self { Self(None) }
}

/// Per-mesh-region state for summary rendering.
pub struct SummaryMeshState {
    pub task: Option<bevy::tasks::Task<SummaryMeshBuildResult>>,
    pub entity: Option<Entity>,
    pub mesh_handle: Option<Handle<Mesh>>,
    pub tri_count: u32,
    pub mesh_origin: Vec3,
    /// Base geometry from the async build (for cross-region skirt rebuilds).
    pub base_positions: Vec<[f32; 3]>,
    pub base_normals: Vec<[f32; 3]>,
    pub base_indices: Vec<u32>,
    pub base_tri_count: u32,
    /// Perimeter edges for cross-region exchange.
    pub perimeter_edges: Vec<common_bevy::summary_mesh::PerimeterEdge>,
    /// How many summaries were built. < 271 means incomplete; re-dispatch on new data.
    pub summaries_built: u32,
}

/// Result from an async summary mesh build task.
/// Carries raw geometry (Mesh constructed on main thread after cross-region stitching).
pub struct SummaryMeshBuildResult {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub tri_count: u32,
    pub mesh_origin: Vec3,
    pub perimeter_edges: Vec<common_bevy::summary_mesh::PerimeterEdge>,
    /// How many summaries were built. < 271 means incomplete data;
    /// region should be rebuilt when more tiles arrive.
    pub summaries_built: u32,
}

/// Tracks mesh state for all summary mesh regions.
#[derive(Resource, Default)]
pub struct SummaryMeshes {
    pub states: HashMap<MeshRegionKey, SummaryMeshState>,
}

/// Marker component for summary mesh entities.
#[derive(Component)]
#[allow(dead_code)]
pub struct SummaryMesh {
    pub region_key: MeshRegionKey,
}

/// Client-side system timers. Wraps `common::timers::SystemTimers`.
/// No transport — data accumulates locally. Can be drained for diagnostics.
#[derive(Resource)]
pub struct ClientTimers(pub common::timers::SystemTimers);

impl Default for ClientTimers {
    fn default() -> Self { Self(common::timers::SystemTimers::new()) }
}
