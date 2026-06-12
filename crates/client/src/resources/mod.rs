use bevy::{
    prelude::*,
    pbr::{ExtendedMaterial, MaterialExtension},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;

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

/// Triangle statistics.
#[derive(Resource, Default)]
pub struct LodTriangleStats {
    /// Total triangles across all meshes.
    pub total_tris: u64,
    /// Total chunks with active meshes.
    pub mesh_count: u32,
    /// Per-band breakdown: r → (tris, mesh_count).
    pub per_band: std::collections::BTreeMap<u32, (u64, u32)>,
    /// In-flight async task counts.
    pub async_cz: u32,
    pub async_mesh: u32,
    pub async_tile: u32,
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

/// Where a cached region's values came from. Values are identical across
/// producers (same 7-sample rule over the same elevation field) — provenance
/// only governs lifecycle: server data is durable for the whole session,
/// flyover data is discarded when flyover toggles.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RegionSource {
    Server,
    Flyover,
}

/// Per-region summary elevation cache.
///
/// Each entry holds all ~271 center_z values for one mesh region.
/// DashMap for per-region locking — async mesh build tasks get an Arc
/// clone (one brief shard lock) then read 271 values lock-free.
#[derive(Resource, Clone, Default)]
pub struct SummaryCache {
    regions: Arc<DashMap<MeshRegionKey, Arc<RegionData>>>,
    new_data: Arc<AtomicBool>,
}

/// Elevation data for one mesh region's summary cells.
pub struct RegionData {
    pub cells: HashMap<(i32, i32), i32>,
    pub source: RegionSource,
}

impl SummaryCache {
    /// Insert region data, merging into any existing entry. Merge keeps the
    /// union of cells (a partial batch can never erase previously received
    /// cells) and promotes provenance to Server if either side is Server.
    pub fn insert_region(&self, key: MeshRegionKey, data: RegionData) {
        match self.regions.get(&key).map(|r| r.value().clone()) {
            Some(existing) => {
                let mut cells = existing.cells.clone();
                cells.extend(data.cells);
                let source = if existing.source == RegionSource::Server
                    || data.source == RegionSource::Server
                {
                    RegionSource::Server
                } else {
                    RegionSource::Flyover
                };
                self.regions.insert(key, Arc::new(RegionData { cells, source }));
            }
            None => {
                self.regions.insert(key, Arc::new(data));
            }
        }
        self.new_data.store(true, Ordering::Relaxed);
    }

    /// Get a region's data. Returns Arc for lock-free reading.
    pub fn get_region(&self, key: &MeshRegionKey) -> Option<Arc<RegionData>> {
        self.regions.get(key).map(|r| r.value().clone())
    }

    /// Check if a region exists in the cache.
    pub fn contains_region(&self, key: &MeshRegionKey) -> bool {
        self.regions.contains_key(key)
    }

    /// Check and clear the new-data flag.
    pub fn take_new_data(&self) -> bool {
        self.new_data.swap(false, Ordering::Relaxed)
    }

    /// Drop flyover-sourced regions (flyover toggle). Server-sourced data
    /// is durable — the server tracks what it has sent per client and never
    /// resends, so discarding it would blank the horizon until the player
    /// walks regions out of and back into the server's visible set.
    pub fn clear_flyover(&self) {
        self.regions.retain(|_, v| v.source == RegionSource::Server);
        self.new_data.store(true, Ordering::Relaxed);
    }
}

/// Client-side system timers. Wraps `common::timers::SystemTimers`.
/// No transport — data accumulates locally. Can be drained for diagnostics.
#[derive(Resource)]
pub struct ClientTimers(pub common::timers::SystemTimers);

impl Default for ClientTimers {
    fn default() -> Self { Self(common::timers::SystemTimers::new()) }
}
