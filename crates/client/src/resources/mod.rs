use bevy::{
    prelude::*,
    pbr::{ExtendedMaterial, MaterialExtension},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use common_bevy::chunk::{ChunkId, ChunkSummary};

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
use bevy_camera::primitives::Aabb;

/// Shared material for all chunk meshes (elevation color computed in shader)
#[derive(Resource)]
pub struct TerrainMaterial {
    pub handle: Handle<ExtendedMaterial<StandardMaterial, TerrainExtension>>,
}

/// Tracks pending async mesh generation tasks per chunk
#[derive(Resource, Default)]
pub struct PendingChunkMeshes {
    pub tasks: HashMap<ChunkId, Task<(Mesh, Aabb)>>,
}

/// Tracks pending async summary mesh generation tasks per chunk
#[derive(Resource, Default)]
pub struct PendingSummaryMeshes {
    pub tasks: HashMap<ChunkId, Task<Mesh>>,
}

/// Chunks whose appearance should NOT trigger neighbor mesh regeneration.
/// When the admin flyover generates all chunks (including a buffer zone) at once,
/// the mesh pipeline already has correct neighbor data — no cascade needed.
#[derive(Debug, Default, Resource)]
pub struct SkipNeighborRegen {
    pub chunks: HashSet<ChunkId>,
}

/// Stores chunk summaries for outer-ring LoD rendering.
/// Separate from the tile Map to avoid collision with physics/pathfinding.
#[derive(Debug, Default, Resource)]
pub struct ChunkSummaries {
    pub summaries: HashMap<ChunkId, ChunkSummary>,
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
