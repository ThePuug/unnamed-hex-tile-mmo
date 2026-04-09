use bevy::{
    prelude::*,
    pbr::{ExtendedMaterial, MaterialExtension},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use std::sync::Arc;
use parking_lot::RwLock;

use common_bevy::chunk::ChunkId;
use common_bevy::message::{SummaryData, SummaryKey};
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

/// Unified cache of summary center_z values for all r >= 1 bands.
///
/// Producers:
/// - **r=1–2 local:** computed from Map tile data via `select_center_z()`
/// - **r=3+ server:** received via `SummaryBatch` from server
/// - **r=3+ flyover:** computed from `AdminComposite.elevation_at()`
///
/// Consumer: `build_summary_mesh_region_from_summaries()` reads center_z
/// for all r >= 1 mesh regions. r=0 still reads tiles from the Map directly.
///
/// Arc-wrapped so async mesh build tasks can read it.
#[derive(Resource, Clone)]
pub struct SummaryCache {
    inner: Arc<RwLock<SummaryCacheInner>>,
}

struct SummaryCacheInner {
    entries: HashMap<SummaryKey, i32>,
    changed: bool,
    /// Mesh region keys that have new/updated data since last take.
    dirty_regions: HashSet<common_bevy::summary_mesh::MeshRegionKey>,
}

impl Default for SummaryCache {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SummaryCacheInner {
                entries: HashMap::new(),
                changed: false,
                dirty_regions: HashSet::new(),
            })),
        }
    }
}

impl SummaryCache {
    /// Insert a single summary (used by async tasks computing local r=1–2).
    /// Does NOT set the changed flag — async-produced entries are consumed
    /// by the task that produced them, not by a subsequent dispatch cycle.
    pub fn insert(&self, key: SummaryKey, center_z: i32) {
        self.inner.write().entries.insert(key, center_z);
    }

    /// Apply a batch of additions and removals (server SummaryBatch or flyover).
    /// Tracks affected mesh regions so dispatch can target just those.
    pub fn apply_batch(&self, additions: &[SummaryData], _removals: &[SummaryKey]) {
        let mut inner = self.inner.write();
        let region_lat = common_bevy::summary::mesh_region_lattice();
        for add in additions {
            let key = SummaryKey { r: add.r, sq: add.sq, sr: add.sr };
            inner.entries.insert(key, add.center_z);
            let (mn, mm) = region_lat.cell_id(add.sq, add.sr);
            inner.dirty_regions.insert(common_bevy::summary_mesh::MeshRegionKey { r: add.r, mn, mm });
        }
        // Removals are no-ops: cache stays warm, mesh eviction is position-based.
        if !additions.is_empty() {
            inner.changed = true;
        }
    }

    /// Check and clear the changed flag.
    pub fn take_changed(&self) -> bool {
        let mut inner = self.inner.write();
        let changed = inner.changed;
        inner.changed = false;
        changed
    }

    /// Clear all entries (used when switching between flyover and normal mode).
    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.entries.clear();
        inner.dirty_regions.clear();
        inner.changed = true;
    }

    /// Look up a summary's center_z by key.
    pub fn get(&self, key: &SummaryKey) -> Option<i32> {
        self.inner.read().entries.get(key).copied()
    }

    /// Look up by summary-lattice coordinates.
    pub fn get_by_lattice(&self, r: u32, sq: i32, sr: i32) -> Option<i32> {
        self.get(&SummaryKey { r, sq, sr })
    }

    /// Drain the set of mesh regions with new/updated data.
    pub fn take_dirty_regions(&self) -> HashSet<common_bevy::summary_mesh::MeshRegionKey> {
        let mut inner = self.inner.write();
        std::mem::take(&mut inner.dirty_regions)
    }

}

/// Client-side system timers. Wraps `common::timers::SystemTimers`.
/// No transport — data accumulates locally. Can be drained for diagnostics.
#[derive(Resource)]
pub struct ClientTimers(pub common::timers::SystemTimers);

impl Default for ClientTimers {
    fn default() -> Self { Self(common::timers::SystemTimers::new()) }
}
