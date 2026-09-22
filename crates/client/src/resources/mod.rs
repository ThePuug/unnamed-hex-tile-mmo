pub mod origin;
pub use origin::{OffWorld, RenderOrigin, rebase_origin};

use bevy::{
    prelude::*,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef, VertexFormat},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
};
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;

use common_bevy::chunk::ChunkId;
use common_bevy::summary_mesh::MeshRegionKey;

/// The band cut for one LoD level: two circles on the ground, the inner
/// one shared with the finer level and the outer with the coarser, each
/// with its own centre since an edge follows the player only as fast as
/// both its levels are on screen (`EdgeCenters`). The shaders drop
/// fragments inside `inner` of `inner_center` or beyond `outer` of
/// `outer_center`, and morph the vertices across `[outer - fade, outer]`
/// onto the coarser level's surface. Field order is the uniform layout in
/// `terrain_cut.wgsl`; the tail pads the struct to the uniform stride. No
/// name in an imported shader module may end in a digit or start with an
/// underscore: the composer rejects any identifier naga's namer would
/// rewrite.
#[derive(ShaderType, Debug, Clone, Copy)]
pub struct TerrainCut {
    pub inner_center: Vec2,
    pub outer_center: Vec2,
    pub inner: f32,
    pub outer: f32,
    pub fade: f32,
    pub pad: f32,
}

impl Default for TerrainCut {
    /// No cut: everything shows.
    fn default() -> Self {
        Self { inner_center: Vec2::ZERO, outer_center: Vec2::ZERO, inner: 0.0, outer: f32::MAX, fade: 0.0, pad: 0.0 }
    }
}

impl TerrainCut {
    /// The cut as the shaders see it: its centres taken off `origin`, the
    /// render origin's ground position.
    pub fn rendered(self, origin: Vec2) -> Self {
        Self { inner_center: self.inner_center - origin, outer_center: self.outer_center - origin, ..self }
    }
}

/// Where each band edge is drawn: the centre of the circle the finer
/// level's cut and the coarser level's arrival share, keyed by the finer
/// level. It follows the player only as fast as both levels are on screen
/// around where it would go, and eases when it moves, so a plate is never
/// cut before the plate replacing it is drawn and the seam never jumps.
/// The keep set holds both levels' regions around a lagging centre.
#[derive(Resource, Default)]
pub struct EdgeCenters(pub HashMap<u32, Vec2>);

/// The coarser level's surface at a terrain vertex: normal xyz, height w in
/// the mesh's frame. The vertex shaders morph position and normal onto it
/// across the transition strip.
pub const ATTRIBUTE_COARSE_SURFACE: MeshVertexAttribute =
    MeshVertexAttribute::new("Terrain_CoarseSurface", 0x7e88_a1c0, VertexFormat::Float32x4);

/// Terrain material extension: elevation colour in the fragment shader,
/// grass over the ramp's green band and scree over its mountain band on
/// the tops, stone on the faces, atmospheric fade from the view position,
/// and the band cut. The shaders are shared; the cut is per material, one
/// material per LoD level.
///
/// Every texture is sampled in world space, so its sampler must wrap: a
/// clamped sampler smears the edge texel across the terrain. Each asset is
/// a texture array of `TEXTURE_VARIANTS` seeds of the tile, each with its
/// mip chain; the shader blends the layers by world position so the
/// repeat never lines up.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct TerrainExtension {
    #[uniform(100)]
    pub cut: TerrainCut,
    /// `assets/textures/grass-plain.dds`, on tile tops over world XZ.
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub grass: Handle<Image>,
    /// `assets/textures/cliff-stone.dds`, on faces over the vertical
    /// planes, world up as the tile's up.
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub cliff: Handle<Image>,
    /// `assets/textures/mountain-scree.dds`, on tile tops over world XZ.
    #[texture(105, dimension = "2d_array")]
    #[sampler(106)]
    pub scree: Handle<Image>,
}

/// Layers in each texture asset, as texgen's `VARIANTS` writes them: the
/// count `terrain.wgsl` blends, which cannot read it from the asset.
#[allow(dead_code)]
const TEXTURE_VARIANTS: u32 = 3;

impl MaterialExtension for TerrainExtension {
    fn vertex_shader() -> ShaderRef {
        "shaders/terrain_vertex.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
    /// Depth-only passes (shadow maps) morph and cut too, so a plate the
    /// main pass drops casts no shadow onto the plate that replaces it and
    /// the depth the main pass tests against is the surface it draws.
    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/terrain_prepass_vertex.wgsl".into()
    }
    fn prepass_fragment_shader() -> ShaderRef {
        "shaders/terrain_prepass.wgsl".into()
    }
    /// One vertex layout for every pass, the locations both vertex shaders
    /// declare: Bevy's own layouts differ between the main pass and the
    /// prepass and leave out the coarse surface.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ATTRIBUTE_COARSE_SURFACE.at_shader_location(3),
        ])?];
        Ok(())
    }
}

pub type TerrainMaterialAsset = ExtendedMaterial<StandardMaterial, TerrainExtension>;

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

/// Terrain materials by LoD level, created on first use so a forced debug
/// radius gets one like any ladder level. Every level shares the textures,
/// loaded once here.
#[derive(Resource)]
pub struct TerrainMaterial {
    pub by_level: HashMap<u32, Handle<TerrainMaterialAsset>>,
    grass: Handle<Image>,
    cliff: Handle<Image>,
    scree: Handle<Image>,
}

impl FromWorld for TerrainMaterial {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        let repeating = |path: &'static str| {
            assets.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
                // The asset declares its layers and mip chain; the sampler
                // reads the chain, trilinear, so a tile far off is its own
                // mean and not the texels the pixel happens to land on.
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
            })
        };
        Self {
            by_level: HashMap::new(),
            grass: repeating("textures/grass-plain.dds"),
            cliff: repeating("textures/cliff-stone.dds"),
            scree: repeating("textures/mountain-scree.dds"),
        }
    }
}

impl TerrainMaterial {
    pub fn for_level(
        &mut self,
        r: u32,
        materials: &mut Assets<TerrainMaterialAsset>,
    ) -> Handle<TerrainMaterialAsset> {
        self.by_level
            .entry(r)
            .or_insert_with(|| {
                materials.add(ExtendedMaterial {
                    base: StandardMaterial {
                        perceptual_roughness: 1.,
                        double_sided: true,
                        cull_mode: None,
                        // Mask is what gives the shadow pipeline a fragment
                        // stage; base colour alpha is 1, so the cutoff itself
                        // never discards.
                        alpha_mode: AlphaMode::Mask(0.5),
                        ..default()
                    },
                    extension: TerrainExtension {
                        grass: self.grass.clone(),
                        cliff: self.cliff.clone(),
                        scree: self.scree.clone(),
                        ..default()
                    },
                })
            })
            .clone()
    }
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
    /// Geometry from the async build, kept so the entity can be respawned
    /// without a rebuild (flyover stash/restore).
    pub base_positions: Vec<[f32; 3]>,
    pub base_normals: Vec<[f32; 3]>,
    pub base_coarse: Vec<[f32; 4]>,
    pub base_indices: Vec<u32>,
    pub base_tri_count: u32,
    /// The water standing over the region, built with the ground and drawn
    /// as a child of its entity, so it lives and dies with the ground.
    pub base_water: WaterGeometry,
    /// The region's trees, placed with the ground and spawned as children
    /// of its entity once the tree kit has loaded.
    pub base_trees: Vec<crate::plugins::forest::TreeInstance>,
    /// Whether the entity now standing carries its trees: set as they are
    /// spawned within the trees' reach, cleared as they are taken down
    /// beyond it and whenever the entity or its children go.
    pub trees_spawned: bool,
    /// The last build yielded nothing — a cell or ring cell had no data
    /// yet. Retried once data has arrived since `epoch`, not on every run:
    /// retrying every frame would take the build slots from regions that
    /// can be built.
    pub waiting: bool,
    /// `SummaryMeshes::epoch` when the last build was dispatched. Data that
    /// lands while a build is in flight, or on a run whose task budget was
    /// spent before this region's turn, is data the region has not built
    /// from, and that run's own change flag is gone by the time it could.
    pub epoch: u64,
}

impl SummaryMeshState {
    /// Whether a build may be dispatched at data `epoch`: none in flight,
    /// nothing built, and not still waiting on the data it last built from.
    pub fn wants_build(&self, epoch: u64) -> bool {
        self.task.is_none() && self.entity.is_none() && !(self.waiting && self.epoch == epoch)
    }
}

/// Raw geometry of the water over a mesh region; empty where none stands.
#[derive(Clone, Default)]
pub struct WaterGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Result from an async summary mesh build task: raw geometry, turned into
/// a Mesh on the main thread.
pub struct SummaryMeshBuildResult {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub coarse: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub tri_count: u32,
    pub mesh_origin: Vec3,
    pub water: WaterGeometry,
    pub trees: Vec<crate::plugins::forest::TreeInstance>,
}

/// Tracks mesh state for all summary mesh regions.
#[derive(Resource, Default)]
pub struct SummaryMeshes {
    pub states: HashMap<MeshRegionKey, SummaryMeshState>,
    /// Counts the dispatch runs that found new map or summary data. A
    /// waiting region is retried while its build's epoch is behind it.
    pub epoch: u64,
}

/// Marker component for summary mesh entities.
#[derive(Component)]
#[allow(dead_code)]
pub struct SummaryMesh {
    pub region_key: MeshRegionKey,
}

/// Marker component for the water mesh under a summary mesh entity.
#[derive(Component)]
pub struct WaterMesh;

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

/// Each entry holds all ~271 center_z values for one mesh region.
/// DashMap for per-region locking — async mesh build tasks get an Arc
/// clone (one brief shard lock) then read 271 values lock-free.
#[derive(Resource, Clone, Default)]
pub struct SummaryCache {
    regions: Arc<DashMap<MeshRegionKey, Arc<RegionData>>>,
    new_data: Arc<AtomicBool>,
}

/// One mesh region's summary cells: each cell's height and the water
/// surface over it, or None where it is dry.
pub struct RegionData {
    pub cells: HashMap<(i32, i32), (i32, Option<i32>)>,
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
