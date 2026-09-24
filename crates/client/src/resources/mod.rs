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
/// `outer_center`, and morph the vertices across
/// `[outer - fade, outer - settle]` onto the coarser level's surface.
/// `settle` is one of the level's own cells: every triangle that reaches
/// the edge then lies wholly on the coarser surface, the two levels meet
/// parallel, and no sightline passes between them where they part. Field
/// order is the uniform layout in `terrain_cut.wgsl`. No name in an
/// imported shader module may end in a digit or start with an underscore:
/// the composer rejects any identifier naga's namer would rewrite.
#[derive(ShaderType, Debug, Clone, Copy, PartialEq)]
pub struct TerrainCut {
    pub inner_center: Vec2,
    pub outer_center: Vec2,
    pub inner: f32,
    pub outer: f32,
    pub fade: f32,
    pub settle: f32,
}

impl Default for TerrainCut {
    /// No cut: everything shows.
    fn default() -> Self {
        Self { inner_center: Vec2::ZERO, outer_center: Vec2::ZERO, inner: 0.0, outer: f32::MAX, fade: 0.0, settle: 0.0 }
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

/// Where the tiles' models hand over to the summaries' cards, as the
/// shaders see it: the ring at the first summary level's edge, its
/// centre and radius, the width of the overlap inside it over which the
/// models dither out and the cards dither in, and the distance by which
/// the cards, sinking from the ring on, have gone wholly under the
/// ground that wears their colour. And the band the far cards stand in,
/// the crags a summary past the tiles stands: from the first summary
/// level's outer edge, where the tiles' own boulders end, to the next
/// level's, each as its centre, radius and overlap. Rendered coordinates,
/// as the regions' transforms are; an unset ring shows the models and
/// hides the cards, and an unset far band hides the far cards.
#[derive(Resource, Clone, Copy, Default, PartialEq, bevy::render::extract_resource::ExtractResource)]
pub struct CardBand {
    pub center: Vec2,
    pub inner: f32,
    pub overlap: f32,
    pub sink_to: f32,
    pub far_in: Vec4,
    pub far_out: Vec4,
}

/// The coarser level's surface at a terrain vertex: normal xyz, height w in
/// the mesh's frame. The vertex shaders morph position and normal onto it
/// across the transition strip.
pub const ATTRIBUTE_COARSE_SURFACE: MeshVertexAttribute =
    MeshVertexAttribute::new("Terrain_CoarseSurface", 0x7e88_a1c0, VertexFormat::Float32x4);

/// How a level wears its canopy: each kind's colour, the trees' own, and
/// how wide a grown crown of it stands; and the crowns' relief, full
/// where the level begins and gone where it ends, so the level past it
/// wears the canopy flat with no seam. A level declaring no relief at
/// all wears it flat throughout.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct CanopyLook {
    /// Pine, deciduous, scrub: rgb, and the crown's width in world units.
    pub kinds: [Vec4; 3],
    /// The same three kinds' grown height, which is how far the ground
    /// rises where their canopy is closed. The fourth is unused.
    pub rises: Vec4,
    /// The circle the canopy's ground rises across: the cards' own centre
    /// in xy, and in zw the ground distances from it where the rise
    /// begins and ends — nothing at the ring where the models hand over,
    /// their whole height where the last card is drawn. Carried rather
    /// than taken from the view, because the shadow pass's view is the
    /// sun's: read from there, every pass would lift a different surface
    /// and the ground would shadow itself from one it never draws.
    pub lift: Vec4,
    /// The circle the rise falls away across, as `lift` states its own:
    /// the third summary level's outer edge's centre in xy, and in zw its
    /// band, where the rise ends to where the next level begins, so the
    /// ground under the canopy is bare where those two meet and neither
    /// draws a rise the other does not.
    pub fall: Vec4,
    /// The ground distances the relief is full at and gone by.
    pub relief_full: f32,
    pub relief_gone: f32,
}

/// One kind's look as the tree kit hands it over: the mean colour of its
/// models, a grown crown's width, and a grown tree's height.
#[derive(Clone, Copy, Debug)]
pub struct KindLook {
    pub color: Vec3,
    pub width: f32,
    pub height: f32,
}

/// Terrain material extension: elevation colour in the fragment shader,
/// grass over the ramp's green band and scree over its mountain band on
/// the tops, stone on the faces, atmospheric fade from the view position,
/// the band cut, and the canopy where the level carries one, in the
/// trees' colours with the crowns' relief near and flat far. The shaders
/// are shared; the cut and the canopy's look are per material, one
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
    #[uniform(107)]
    pub canopy: CanopyLook,
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
    /// prepass and leave out the coarse surface. A level that colours its
    /// ground by the canopy carries it as the vertex colour, which Bevy
    /// already interpolates to the fragment.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let mut attributes = vec![
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ATTRIBUTE_COARSE_SURFACE.at_shader_location(3),
        ];
        if layout.0.contains(Mesh::ATTRIBUTE_COLOR) {
            attributes.push(Mesh::ATTRIBUTE_COLOR.at_shader_location(4));
        }
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        Ok(())
    }
}

pub type TerrainMaterialAsset = ExtendedMaterial<StandardMaterial, TerrainExtension>;

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct EntityMap(BiMap<Entity,Entity>);

/// The map before any of the world has arrived.
pub fn world_map() -> common_bevy::resources::map::Map {
    common_bevy::resources::map::Map::new(qrz::Map::<common_bevy::components::entity_type::EntityType>::new(
        common::camera::HEX_RADIUS,
        common::camera::RISE,
        qrz::HexOrientation::FlatTop,
    ))
}

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
    /// The kinds' looks once the tree kit has handed them over: pine,
    /// deciduous, scrub.
    kinds: Option<[KindLook; 3]>,
}

impl FromWorld for TerrainMaterial {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        let repeating = |path: &'static str| {
            assets
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    // The asset declares its layers and mip chain; the sampler
                    // reads the chain, trilinear, so a tile far off is its own
                    // mean and not the texels the pixel happens to land on.
                    settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        ..ImageSamplerDescriptor::linear()
                    });
                })
                .load(path)
        };
        Self {
            by_level: HashMap::new(),
            grass: repeating("textures/grass-plain.dds"),
            cliff: repeating("textures/cliff-stone.dds"),
            scree: repeating("textures/mountain-scree.dds"),
            kinds: None,
        }
    }
}

impl TerrainMaterial {
    /// How level `r` wears its canopy: the kinds' looks, once the kit has
    /// given them, with the canopy's relief on every level that wears the
    /// colour, over the same two distances whichever level it is, so no
    /// seam opens where one hands to the next. It runs full to the end of
    /// the third level's band and fades over the fourth's, which is as
    /// far as the colour itself reaches; the tiles wear neither.
    fn canopy_for(&self, r: u32) -> CanopyLook {
        use common_bevy::summary::{threshold_horiz, LOD_LEVELS};
        let Some(kinds) = &self.kinds else { return CanopyLook::default() };
        let (relief_full, relief_gone) = if LOD_LEVELS[1..LOD_LEVELS.len() - 1].contains(&r) {
            (threshold_horiz(LOD_LEVELS[2]), threshold_horiz(LOD_LEVELS[3]))
        } else {
            (0.0, 0.0)
        };
        CanopyLook {
            kinds: kinds.map(|k| k.color.extend(k.width)),
            rises: Vec4::new(kinds[0].height, kinds[1].height, kinds[2].height, 0.0),
            // The span is the band cut's, handed over each frame by
            // `update_terrain_cut`; until it has run the ground is flat.
            lift: Vec4::ZERO,
            fall: Vec4::ZERO,
            relief_full,
            relief_gone,
        }
    }

    /// Hand every level the kinds' looks, from the tree kit once it has
    /// loaded.
    pub fn set_kinds(&mut self, kinds: [KindLook; 3], materials: &mut Assets<TerrainMaterialAsset>) {
        self.kinds = Some(kinds);
        for (&r, handle) in &self.by_level {
            if let Some(mut material) = materials.get_mut(handle) {
                material.extension.canopy = self.canopy_for(r);
            }
        }
    }

    pub fn for_level(
        &mut self,
        r: u32,
        materials: &mut Assets<TerrainMaterialAsset>,
    ) -> Handle<TerrainMaterialAsset> {
        let canopy = self.canopy_for(r);
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
                        canopy,
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
    pub base_canopy: Vec<[f32; 4]>,
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
    /// The same for its trees as cards, past the trees' keep.
    pub cards_spawned: bool,
    /// The last build yielded nothing — a cell or ring cell had no data
    /// yet. Retried once data has arrived since `epoch`, not on every run:
    /// retrying every frame would take the build slots from regions that
    /// can be built.
    pub waiting: bool,
    /// Whether every chunk under the region had been streamed when its
    /// last build was dispatched. A region's ground can be built from the
    /// summaries the server sends, which reach past the tiles, while the
    /// covers its trees come from have not arrived; such a region is
    /// built once more when they all have.
    pub tiles_loaded: bool,
    /// Whether a tile under the region changed since its last build, so
    /// its trees stand as they were: built once more.
    pub stale: bool,
    /// `SummaryMeshes::epoch` when the last build was dispatched. Data that
    /// lands while a build is in flight, or on a run whose task budget was
    /// spent before this region's turn, is data the region has not built
    /// from, and that run's own change flag is gone by the time it could.
    pub epoch: u64,
}

impl SummaryMeshState {
    /// Whether a build may be dispatched at data `epoch` with the region's
    /// tiles `tiles_loaded`: none in flight, and either nothing built and
    /// not still waiting on the data it last built from, or built without
    /// the tiles its trees come from and holding them now.
    pub fn wants_build(&self, epoch: u64, tiles_loaded: bool) -> bool {
        if self.task.is_some() {
            return false;
        }
        if self.entity.is_none() {
            return !(self.waiting && self.epoch == epoch);
        }
        self.stale || tiles_loaded && !self.tiles_loaded
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
    /// The canopy per vertex where the level colours its ground by it,
    /// else empty.
    pub canopy: Vec<[f32; 4]>,
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

/// One mesh region's summary cells.
pub struct RegionData {
    pub cells: HashMap<(i32, i32), common_bevy::summary::SummaryCell>,
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
/// Shared, because a frame is main-thread work and render-thread work and
/// the render app holds a clone of the same accumulator.
#[derive(Resource, Clone)]
pub struct ClientTimers(pub Arc<common::timers::SystemTimers>);

impl Default for ClientTimers {
    fn default() -> Self { Self(Arc::new(common::timers::SystemTimers::new())) }
}
