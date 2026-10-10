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

/// The canopy's rise a terrain vertex draws, as the weights the three
/// kinds' heights are read by (`summary_mesh::rise_weights`), read from
/// [`RISE_LEVEL`]'s canopy.
pub const ATTRIBUTE_RISE: MeshVertexAttribute =
    MeshVertexAttribute::new("Terrain_Rise", 0x7e88_a1c1, VertexFormat::Float32x3);

/// The level whose canopy the ground's rise is read from, by every level
/// that draws one: the third, whose band the rise falls away across. The
/// rise reaches no level past it, and the levels before it draw one
/// reading of it, so where two meet they stand at one height.
pub const RISE_LEVEL: u32 = common_bevy::summary::LOD_LEVELS[2];

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
    /// The density a canopy reads closed from (`summary_mesh::CANOPY_FULL`).
    pub full: f32,
}

/// How a level's shaders read its canopy's parts (`plugins::canopy`): the
/// finer step a part stands on, in tiles, or none where the level wears no
/// canopy; how far a region's layer reaches from its centre, in steps; the
/// steps a part's whole ground is counted in; a tile's radius; and whether
/// the parts are read at all, where otherwise the ground wears the one
/// canopy each summary's vertices carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct CanopyLattice {
    pub step: i32,
    pub half: i32,
    pub whole: f32,
    pub radius: f32,
    pub on: u32,
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
/// a texture array of three seeds of the tile (texgen's `VARIANTS`), each
/// with its mip chain; the shader blends the layers by world position so
/// the repeat never lines up, and carries that count itself, since it
/// cannot read it from the asset.
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
    /// The level's canopy parts, a layer to each region, the layer the
    /// region's `MeshTag`.
    #[texture(108, dimension = "2d_array", sample_type = "u_int")]
    pub parts: Handle<Image>,
    #[uniform(109)]
    pub lattice: CanopyLattice,
}

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
            ATTRIBUTE_RISE.at_shader_location(5),
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
    common_bevy::resources::map::Map::new(qrz::Map::new(common::grid::HEX_RADIUS, common::grid::RISE))
}

#[derive(Debug, Resource)]
pub struct Server {
    /// Server's game world time when Init event was received
    pub server_time_at_init: u128,
    /// Client's elapsed time when Init event was received
    pub client_time_at_init: u128,
    /// Last time we sent a ping (for periodic pings)
    pub last_ping_time: u128,
    /// The trip to the server, half the round trip the transport measures
    pub latency: u128,
    /// How far past `latency` the clock leads, in ms, held by how early
    /// the server says presses arrive ([`Server::arrived`])
    pub margin: f64,
    /// How early a press arrives, smoothed
    pub early: f64,
    /// How far an arrival strays from `early`, smoothed
    pub spread: f64,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            server_time_at_init: 0,
            client_time_at_init: 0,
            last_ping_time: 0,
            latency: 50,
            margin: 40.0,
            early: 40.0,
            spread: 10.0,
        }
    }
}

impl Server {
    /// Set the clock from `dt`, the server's game world time as it sent
    /// Init, received at `client_now`: the server has run on by the trip
    /// here since, and by nothing else, however long the client ran before.
    pub fn sync(&mut self, dt: u128, client_now: u128) {
        self.server_time_at_init = dt.saturating_add(self.latency);
        self.client_time_at_init = client_now;
    }

    /// The game clock as this client lives it, at `client_now`: the
    /// server's, ahead by [`Server::lead`]. What the client does at a moment
    /// reaches the server before the server's clock gets there, so the
    /// server judges a press at the moment it was made and never takes a
    /// client's word for a moment already past (`abilities::Press`); a
    /// threat, timed 2 seconds and more ahead, shows that much later.
    pub fn current_time(&self, client_now: u128) -> u128 {
        let time_since_init = client_now.saturating_sub(self.client_time_at_init);
        self.server_time_at_init.saturating_add(time_since_init).saturating_add(self.lead())
    }

    /// [`Server::current_time`] as a `Duration`, the form the queues and
    /// abilities are stamped in.
    pub fn now(&self, client_now: u128) -> std::time::Duration {
        std::time::Duration::from_millis(self.current_time(client_now).min(u64::MAX as u128) as u64)
    }

    /// How far the clock leads the server's: a trip there, and the margin
    /// past it that arrivals hold ([`Server::arrived`]).
    pub fn lead(&self) -> u128 {
        (self.latency as f64 + self.margin).max(0.0) as u128
    }

    /// Take an arrival: a press reached the server `early` ms before its
    /// moment, negative when it came after. The margin moves an eighth of
    /// the way toward where arrivals stand four times their spread early,
    /// what a retransmission timer allows a trip (RFC 6298), so a press
    /// made on time is late only in a spike. The clock moves with it.
    pub fn arrived(&mut self, early: i64) {
        let early = early as f64;
        self.spread = self.spread * 0.75 + (early - self.early).abs() * 0.25;
        self.early = self.early * 0.875 + early * 0.125;
        self.margin += (4.0 * self.spread - early) / 8.0;
    }
}

/// Terrain materials by LoD level, created on first use. Every level shares the textures,
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
        let full = common_bevy::summary_mesh::CANOPY_FULL;
        let Some(kinds) = &self.kinds else { return CanopyLook { full, ..default() } };
        let (relief_full, relief_gone) = if Self::canopied(r) {
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
            full,
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

    /// Whether level `r` wears a canopy: every level above the tiles but
    /// the last.
    pub fn canopied(r: u32) -> bool {
        use common_bevy::summary::LOD_LEVELS;
        r != LOD_LEVELS[0] && r != *LOD_LEVELS.last().expect("a ladder")
    }

    /// Level `r`'s material, made on first asking, binding `parts`, the
    /// level's canopy parts.
    pub fn for_level(
        &mut self,
        r: u32,
        materials: &mut Assets<TerrainMaterialAsset>,
        parts: Handle<Image>,
    ) -> Handle<TerrainMaterialAsset> {
        let canopy = self.canopy_for(r);
        let scale = common::summary::scale(r);
        let lattice = CanopyLattice {
            step: if Self::canopied(r) && scale % 3 == 0 { scale / 3 } else { 0 },
            half: common_bevy::summary_mesh::PARTS_LAYER_HALF,
            whole: common::cover::CANOPY_WHOLE as f32,
            radius: common::grid::HEX_RADIUS,
            on: 1,
        };
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
                        parts,
                        lattice,
                        ..default()
                    },
                })
            })
            .clone()
    }
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
    /// Summary mesh tasks in flight.
    pub async_mesh: u32,
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

/// Per-mesh-region state for summary rendering.
pub struct SummaryMeshState {
    pub task: Option<bevy::tasks::Task<SummaryMeshBuildResult>>,
    pub entity: Option<Entity>,
    pub mesh_handle: Option<Handle<Mesh>>,
    pub tri_count: u32,
    pub mesh_origin: Vec3,
    /// The region's cover, placed with the ground and spawned as children
    /// of its entity once the cover kit has loaded.
    pub cover: Vec<crate::plugins::cover::CoverInstance>,
    /// Whether the entity now standing carries its cover as models: set as
    /// they are spawned within the models' reach, cleared as they are taken
    /// down beyond it and whenever the entity or its children go.
    pub models_spawned: bool,
    /// The same for its cover as cards, past the models' keep.
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
    pub rise: Vec<[f32; 3]>,
    /// The canopy per vertex where the level colours its ground by it,
    /// else empty.
    pub canopy: Vec<[f32; 4]>,
    /// The region's canopy parts as its layer holds them
    /// (`summary_mesh::parts_layer`) where the level wears a canopy, else
    /// empty.
    pub parts: Vec<u16>,
    pub indices: Vec<u32>,
    pub tri_count: u32,
    pub mesh_origin: Vec3,
    pub water: WaterGeometry,
    pub cover: Vec<crate::plugins::cover::CoverInstance>,
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
pub struct SummaryMesh;

/// Marker component for the water mesh under a summary mesh entity.
#[derive(Component)]
pub struct WaterMesh;

/// Per-region summary cache: each entry holds a mesh region's 271 cells.
/// DashMap for per-region locking — async mesh build tasks get an Arc
/// clone (one brief shard lock) then read the cells lock-free.
#[derive(Resource, Clone, Default)]
pub struct SummaryCache {
    regions: Arc<DashMap<MeshRegionKey, Arc<RegionData>>>,
    new_data: Arc<AtomicBool>,
    /// Built regions a revised summary is drawn in: its own and those of
    /// the cells around it, whose rims read it.
    revised: Arc<std::sync::Mutex<HashSet<MeshRegionKey>>>,
}

/// One mesh region's summary cells.
pub struct RegionData {
    pub cells: HashMap<(i32, i32), common_bevy::summary::SummaryCell>,
}

/// A cell and the six around it, as lattice offsets.
const RING: [(i32, i32); 7] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// The regions of the levels finer than [`RISE_LEVEL`] whose vertices
/// carry the rise read from its cell `cell`: a part of it stands at the fan
/// corners of the cells around it, and a finer vertex reads the fan it
/// lies in. The tiles draw no rise.
fn finer_regions_reading_rise(cell: (i32, i32)) -> HashSet<MeshRegionKey> {
    use common_bevy::summary::{part_offsets, summary_lattice, LOD_LEVELS};
    let scale = summary_lattice(RISE_LEVEL).scale;
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let mut regions = HashSet::new();
    for &finer in LOD_LEVELS.iter().filter(|&&l| l > 0 && l < RISE_LEVEL) {
        let fine = summary_lattice(finer).scale;
        for (dq, dr) in RING {
            let (cq, cr) = ((cell.0 + dq) * scale, (cell.1 + dr) * scale);
            for (oq, or) in part_offsets(RISE_LEVEL) {
                // A part stands on a centre of the next finer level, and
                // so on every finer level's lattice.
                let (q, r) = (cq + oq, cr + or);
                let (fq, fr) = (q.div_euclid(fine), r.div_euclid(fine));
                for (eq, er) in RING {
                    let (mn, mm) = region_lat.cell_id(fq + eq, fr + er);
                    regions.insert(MeshRegionKey { r: finer, mn, mm });
                }
            }
        }
    }
    regions
}

impl SummaryCache {
    /// Insert region data, merging into any existing entry. Merge keeps the
    /// union of cells (a partial batch can never erase previously received
    /// cells).
    /// A cell that arrives with a value other than the one held is a
    /// revision: the regions drawing it are noted for [`Self::take_revised`].
    pub fn insert_region(&self, key: MeshRegionKey, data: RegionData) {
        match self.regions.get(&key).map(|r| r.value().clone()) {
            Some(existing) => {
                let region_lat = common_bevy::summary::mesh_region_lattice();
                let mut revised = self.revised.lock().expect("the lock is never poisoned");
                for (&(sq, sr), cell) in &data.cells {
                    let Some(held) = existing.cells.get(&(sq, sr)).filter(|held| *held != cell) else { continue };
                    for (dq, dr) in RING {
                        let (mn, mm) = region_lat.cell_id(sq + dq, sr + dr);
                        revised.insert(MeshRegionKey { r: key.r, mn, mm });
                    }
                    if key.r == RISE_LEVEL && (held.canopy != cell.canopy || held.wet != cell.wet) {
                        revised.extend(finer_regions_reading_rise((sq, sr)));
                    }
                }
                drop(revised);
                let mut cells = existing.cells.clone();
                cells.extend(data.cells);
                self.regions.insert(key, Arc::new(RegionData { cells }));
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

    /// The regions noted as drawing a revised summary since the last call.
    pub fn take_revised(&self) -> HashSet<MeshRegionKey> {
        std::mem::take(&mut *self.revised.lock().expect("the lock is never poisoned"))
    }
}

/// Client-side system timers. Wraps `common::timers::SystemTimers`, which
/// keeps every sample until drained; only admin builds publish the metrics
/// that drain it, so elsewhere a timer records nothing. Shared, because a
/// frame is main-thread work and render-thread work and the render app
/// holds a clone of the same accumulator.
#[derive(Resource, Clone)]
pub struct ClientTimers {
    #[cfg(feature = "admin")]
    timers: Arc<common::timers::SystemTimers>,
}

impl Default for ClientTimers {
    fn default() -> Self {
        Self {
            #[cfg(feature = "admin")]
            timers: Arc::new(common::timers::SystemTimers::new()),
        }
    }
}

impl ClientTimers {
    /// Times `name` until the guard drops.
    pub fn scope(&self, name: &'static str) -> Option<common::timers::ScopeTimer<'_>> {
        #[cfg(feature = "admin")]
        return Some(self.timers.scope(name));
        #[cfg(not(feature = "admin"))]
        {
            let _ = name;
            None
        }
    }

    /// Records `ms` against `name`.
    #[cfg(feature = "admin")]
    pub fn record(&self, name: &'static str, ms: f32) {
        self.timers.record(name, ms);
    }

    /// Each timer's p95 and count since the last drain.
    #[cfg(feature = "admin")]
    pub fn drain(&self) -> Vec<(&'static str, f32, f32)> {
        self.timers.drain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_does_not_run_ahead_by_the_clients_uptime() {
        let at = |uptime: u128| {
            let mut server = Server::default();
            server.sync(10_000, uptime);
            server.current_time(uptime + 1_500)
        };
        assert_eq!(at(0), at(90_000), "time on the character screen shifts nothing");
    }

    #[test]
    fn the_clock_starts_a_trip_past_what_the_server_sent_and_leads_it() {
        let mut server = Server::default();
        server.sync(10_000, 4_000);
        assert_eq!(server.current_time(4_000), 10_000 + server.latency + server.lead());
    }

    /// Presses over a link whose trips run through `trips` in turn, the
    /// transport's measure of it `latency`: each arrives as early as the
    /// lead outruns its trip. Returns the server and the last round's
    /// arrivals.
    fn presses(latency: u128, trips: &[i64]) -> (Server, Vec<i64>) {
        let mut server = Server { latency, ..Server::default() };
        let mut last = Vec::new();
        for round in 0..100 {
            last.clear();
            for &trip in trips {
                let early = server.lead() as i64 - trip;
                server.arrived(early);
                if round == 99 {
                    last.push(early);
                }
            }
        }
        (server, last)
    }

    #[test]
    fn the_lead_holds_presses_early_by_their_spread() {
        let (steady, _) = presses(50, &[60, 62, 58, 60]);
        let (jittery, arrivals) = presses(50, &[20, 100, 40, 80]);
        assert!(jittery.lead() > steady.lead(), "a link that strays leads further");
        assert!(arrivals.iter().all(|&early| early > 0), "and its presses arrive ahead of their moments: {arrivals:?}");

        let (short, _) = presses(10, &[60, 62, 58, 60]);
        assert!(short.lead().abs_diff(steady.lead()) <= 2, "a trip the transport misjudges is made up: {} {}", short.lead(), steady.lead());
    }

    #[test]
    fn a_late_press_pushes_the_lead_out() {
        let mut server = Server::default();
        let before = server.lead();
        server.arrived(-30);
        assert!(server.lead() > before);
    }

    #[test]
    fn a_pong_takes_back_the_time_the_server_lost() {
        let mut server = Server::default();
        server.sync(10_000, 0);
        // The server stalls and its clock falls 1_750 behind the client's
        server.sync(10_000 + 20_000 - 1_750, 20_000);
        assert_eq!(server.current_time(20_000), 10_000 + 20_000 - 1_750 + server.latency + server.lead());
    }
    /// A revised canopy at the rise's level rebuilds every finer region
    /// whose vertices carry its rise: built over the summaries with and
    /// without it, a region whose rise differs is one the revision names.
    #[test]
    fn a_revised_canopy_names_every_finer_region_carrying_its_rise() {
        use common::cover::{Canopy, Content, Cover};
        use common_bevy::summary::{SummaryCell, PARTS};
        use common_bevy::summary_mesh::build_summary_mesh_region;
        let pines = Canopy::of(Cover::NONE.with(0, Content::Pine).with(1, Content::Pine));
        let revised = (1, -1);
        let bare = |_: i32, _: i32| Some(SummaryCell::default());
        let wooded = |q: i32, r: i32| Some(SummaryCell { canopy: if (q, r) == revised { [pines; PARTS] } else { [Canopy::NONE; PARTS] }, ..SummaryCell::default() });
        let flat = |_: i32, _: i32| Some(5);
        let named = finer_regions_reading_rise(revised);
        let mut reading = 0;
        for mn in -3..=3 {
            for mm in -3..=3 {
                let key = MeshRegionKey { r: 1, mn, mm };
                let build = |cells: &dyn Fn(i32, i32) -> Option<SummaryCell>| {
                    build_summary_mesh_region(1, key, &flat, Some(&flat), None, Some((RISE_LEVEL, cells))).expect("every cell is there").rise
                };
                if build(&bare) != build(&wooded) {
                    reading += 1;
                    assert!(named.contains(&key), "{key:?} carries the revised rise");
                }
            }
        }
        assert!(reading > 0, "some region reads it");
    }
}
