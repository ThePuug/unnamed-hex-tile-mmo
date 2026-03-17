use bevy::{
    prelude::*,
    pbr::{ExtendedMaterial, MaterialExtension},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use common_bevy::chunk::ChunkId;
use common_bevy::qem::DecimatedMesh;

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

/// LoD level for a chunk mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LodLevel { Lod1, Lod2 }

/// Per-chunk dual-LoD mesh state.
pub struct ChunkLodState {
    pub lod1_task: Option<Task<DecimatedMesh>>,
    pub lod2_task: Option<Task<DecimatedMesh>>,
    pub lod1_mesh: Option<Handle<Mesh>>,
    pub lod2_mesh: Option<Handle<Mesh>>,
    pub lod1_tris: u32,
    pub lod2_tris: u32,
    pub active_lod: LodLevel,
    pub entity: Option<Entity>,
}

/// Tracks dual-LoD mesh generation for all chunks.
#[derive(Resource, Default)]
pub struct ChunkLodMeshes {
    pub states: HashMap<ChunkId, ChunkLodState>,
}

/// LRU observation window for QEM stats per chunk.
/// Observations survive chunk eviction — shows rolling QEM performance.
#[derive(Resource)]
pub struct LodTriangleStats {
    pub lod1: std::collections::VecDeque<QemObservation>,
    pub lod2: std::collections::VecDeque<QemObservation>,
}

pub struct QemObservation {
    pub raw_tris: u32,
    pub decimated_tris: u32,
    pub max_error: f32,
}

const TRI_STATS_WINDOW: usize = 200;

impl Default for LodTriangleStats {
    fn default() -> Self {
        Self {
            lod1: std::collections::VecDeque::with_capacity(TRI_STATS_WINDOW),
            lod2: std::collections::VecDeque::with_capacity(TRI_STATS_WINDOW),
        }
    }
}

impl LodTriangleStats {
    pub fn push_lod1(&mut self, raw: u32, decimated: u32, max_error: f32) {
        if self.lod1.len() >= TRI_STATS_WINDOW { self.lod1.pop_front(); }
        self.lod1.push_back(QemObservation { raw_tris: raw, decimated_tris: decimated, max_error });
    }
    pub fn push_lod2(&mut self, raw: u32, decimated: u32, max_error: f32) {
        if self.lod2.len() >= TRI_STATS_WINDOW { self.lod2.pop_front(); }
        self.lod2.push_back(QemObservation { raw_tris: raw, decimated_tris: decimated, max_error });
    }
    fn aggregate(window: &std::collections::VecDeque<QemObservation>) -> (f64, f64) {
        let (raw, dec) = window.iter().fold((0u64, 0u64), |(r, d), o| (r + o.raw_tris as u64, d + o.decimated_tris as u64));
        let ratio = if raw > 0 { dec as f64 / raw as f64 } else { 0.0 };
        let err_p95 = if window.is_empty() {
            0.0
        } else {
            let mut errs: Vec<f32> = window.iter().map(|o| o.max_error).collect();
            errs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let rank = (errs.len() as f64 * 0.95).ceil() as usize;
            errs[rank.saturating_sub(1)] as f64
        };
        (ratio, err_p95)
    }
    pub fn lod1_stats(&self) -> (f64, f64) { Self::aggregate(&self.lod1) }
    pub fn lod2_stats(&self) -> (f64, f64) { Self::aggregate(&self.lod2) }
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
