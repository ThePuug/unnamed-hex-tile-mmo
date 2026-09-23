//! The instanced draw of the wood: every tree of one model, whichever
//! region it stands in, as one stand, drawn with one call per pass, near
//! as the model and far, past the ring, as its kind's card.
//!
//! The world arrives in regions and the draw does not follow it: a region
//! only says which trees it stands, as its [`RegionWood`], and each stand
//! gathers its model's trees from every region into one instance buffer,
//! a region's trees one run of it, rebuilt when a region comes, changes
//! or goes. Each instance carries its region's slot in the wood's table of
//! frames, so the trees stay in their regions' frames and the render
//! origin moves them all without an instance being rewritten.
//!
//! Every frame each view keeps the runs whose region's box it can see, as
//! the arguments of one indirect draw each, and a stand is one multi-draw
//! of them: the GPU draws what the regions' own culling leaves, and the
//! CPU records a call a stand rather than a call a region and model.
//! Trees and cards are opaque and cast no shadow, and go into the depth
//! prepass and then the opaque pass: the prepass writes every silhouette,
//! so the opaque pass shades each pixel of the wood once and never shades
//! the ground a canopy covers. A card is a quad the shader turns to the
//! camera, with the model's baked picture on it.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::UntypedAssetId;
use bevy::camera::primitives::{Aabb, Frustum};
use bevy::camera::visibility::{self, NoFrustumCulling, VisibilityClass};
use bevy::core_pipeline::core_3d::{Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey};
use bevy::core_pipeline::prepass::{Opaque3dPrepass, OpaqueNoLightmap3dBatchSetKey, OpaqueNoLightmap3dBinKey};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::{lifetimeless::*, SystemParamItem};
use bevy::math::{primitives::ViewFrustum, Affine3A};
use bevy::mesh::{MeshVertexBufferLayoutRef, VertexBufferLayout};
use bevy::pbr::{MeshPipeline, MeshPipelineKey, MeshPipelineSystems, RenderMeshInstances, SetMeshViewBindGroup, SetMeshViewBindingArrayBindGroup, ViewKeyCache, MATERIAL_BIND_GROUP_INDEX};
use bevy::prelude::*;
use bevy::shader::ShaderDefVal;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::mesh::{
    allocator::{MeshAllocator, MeshSlabs},
    RenderMesh, RenderMeshBufferInfo,
};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::{
    AddRenderCommand, BinnedPhaseItem, BinnedRenderPhaseType, DrawFunctionId, DrawFunctions, PhaseItem, RenderCommand,
    RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewBinnedRenderPhases,
};
use bevy::render::render_resource::binding_types::{sampler, storage_buffer_read_only, texture_2d_array, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_component::SyncComponent;
use bevy::render::sync_world::MainEntity;
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, RenderVisibleEntities, RetainedViewEntity};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};
use bytemuck::{Pod, Zeroable};
use common::Slot;

use crate::resources::CardBand;
use crate::systems::camera::NEAR_FADE_RADIUS;

const TREE_SHADER: &str = "shaders/trees.wgsl";
const CARD_SHADER: &str = "shaders/cards.wgsl";
/// What both import. A module is resolved only once its asset is loaded,
/// and a pipeline whose import is missing waits forever and says nothing,
/// so the handle is held here for the life of the app.
const SHARED_SHADER: &str = "shaders/forest_shared.wgsl";

/// The fragment stage each shader writes its silhouette into the depth
/// prepass with: the colour stage's discards and nothing else.
const DEPTH_ENTRY: &str = "prepass_fragment";

/// The fragment stage each shader shades with. Both stages live in one
/// module, so neither is the module's only one and each pipeline must
/// name the one it wants.
const COLOUR_ENTRY: &str = "fragment";

/// One tree as the shader reads it: its place in its region's frame and
/// its scale, then the cosine and sine of its turn; for a card, the first
/// of its layers in the card texture, its mirror, one or minus one, and
/// the model's height in world units. The last component of the second
/// is the slot of its region's frame, which the stand writes.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct Instance {
    pub pos_scale: [f32; 4],
    pub turn: [f32; 4],
}

impl Instance {
    pub fn new(translation: Vec3, yaw: f32, scale: f32) -> Self {
        Instance {
            pos_scale: [translation.x, translation.y, translation.z, scale],
            turn: [yaw.cos(), yaw.sin(), 0.0, 0.0],
        }
    }

    /// A card: the first layer of its variation's pictures, mirrored by
    /// its turn, facing left or right of the front, and `height` tall as
    /// scaled, which the shader centres the picture on.
    pub fn card(translation: Vec3, yaw: f32, scale: f32, layer: u32, height: f32) -> Self {
        let mirror = if yaw.sin() < 0.0 { -1.0 } else { 1.0 };
        Instance {
            pos_scale: [translation.x, translation.y, translation.z, scale],
            turn: [layer as f32, mirror, height, 0.0],
        }
    }
}

/// Which stand a tree is drawn in: one variation's model, or one kind's
/// cards, whose variations differ only by the layers their instances name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StandKey {
    Model(Slot, usize),
    Cards(AssetId<Image>),
}

/// One stand's share of a region: the mesh the stand draws, the kind's
/// cards where it is a stand of cards, and the region's trees in it.
pub struct WoodPart {
    pub key: StandKey,
    pub mesh: Handle<Mesh>,
    pub cards: Option<Arc<Cards>>,
    pub instances: Vec<Instance>,
}

/// The trees a region stands, by stand, and the box they stand in, in the
/// region's frame. The region's own entity carries it, so the region
/// going takes its trees out of their stands with it.
#[derive(Component)]
pub struct RegionWood {
    pub parts: Vec<WoodPart>,
    pub bounds: Aabb,
}

impl RegionWood {
    /// The wood of `parts`, boxed about every instance's foot and up to
    /// `reach` above and about it.
    pub fn new(parts: Vec<WoodPart>, reach: f32) -> Self {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for i in parts.iter().flat_map(|p| &p.instances) {
            let p = Vec3::new(i.pos_scale[0], i.pos_scale[1], i.pos_scale[2]);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let margin = Vec3::new(reach, 0.0, reach);
        RegionWood { parts, bounds: Aabb::from_min_max(lo - margin, hi + margin + Vec3::Y * reach) }
    }
}

/// One region's run of a stand's instance buffer.
#[derive(Clone, Copy, Debug)]
pub struct StandRange {
    pub slot: u32,
    pub start: u32,
    pub count: u32,
}

/// What a stand draws from: its instance buffer, none while it stands no
/// tree, and each region's run of it.
#[derive(Clone, Default)]
pub struct StandDraw {
    pub buffer: Option<Buffer>,
    pub ranges: Arc<Vec<StandRange>>,
}

/// A stand of models. The entity holds the model's mesh and no material,
/// so nothing else draws it, and is never culled whole: its regions are.
#[derive(Component, Clone, Default)]
#[require(VisibilityClass, NoFrustumCulling)]
#[component(on_add = visibility::add_visibility_class::<TreeStand>)]
pub struct TreeStand(pub StandDraw);

/// A stand of one kind's cards. The entity holds the shared quad.
#[derive(Component, Clone)]
#[require(VisibilityClass, NoFrustumCulling)]
#[component(on_add = visibility::add_visibility_class::<CardStand>)]
pub struct CardStand {
    pub draw: StandDraw,
    pub cards: Arc<Cards>,
}

/// A stand the draw takes its instances from.
pub trait Stand: Component {
    /// What the overlay calls the time spent queueing stands of this kind
    /// into the opaque pass, and into the depth prepass before it.
    const TIMER: &'static str;
    const DEPTH_TIMER: &'static str;

    fn draw(&self) -> &StandDraw;
}

impl Stand for TreeStand {
    const TIMER: &'static str = "tree_q";
    const DEPTH_TIMER: &'static str = "tree_z";

    fn draw(&self) -> &StandDraw { &self.0 }
}

impl Stand for CardStand {
    const TIMER: &'static str = "card_q";
    const DEPTH_TIMER: &'static str = "card_z";

    fn draw(&self) -> &StandDraw { &self.draw }
}

/// One view of a kind's card, as the model declared it: the frame in
/// world units and the elevation it was baked from, in radians.
#[derive(Clone, Copy, Debug)]
pub struct CardView {
    pub width: f32,
    pub height: f32,
    pub elevation: f32,
}

/// The layers of one variation's pictures in a card texture: each view's
/// unlit colour then its shape, side then top, as modelgen packs them.
pub const CARD_LAYERS: u32 = 4;

/// A kind's cards: the texture array, `CARD_LAYERS` per variation, and
/// the two views' frames.
#[derive(Debug)]
pub struct Cards {
    pub texture: Handle<Image>,
    pub side: CardView,
    pub top: CardView,
}

impl SyncComponent for TreeStand {
    type Target = TreeStand;
}

impl ExtractComponent for TreeStand {
    type QueryData = &'static TreeStand;
    type QueryFilter = ();
    type Out = TreeStand;

    fn extract_component(stand: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        Some(stand.clone())
    }
}

impl SyncComponent for CardStand {
    type Target = CardStand;
}

impl ExtractComponent for CardStand {
    type QueryData = &'static CardStand;
    type QueryFilter = ();
    type Out = CardStand;

    fn extract_component(stand: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        Some(stand.clone())
    }
}

/// Every stand and every region standing trees: each region's slot in the
/// table of frames, its box, and the stands it has trees in; each stand's
/// entity and whether its buffer must be gathered again.
#[derive(Resource, Default)]
pub struct Stands {
    stands: HashMap<StandKey, StandEntry>,
    regions: HashMap<Entity, RegionEntry>,
    free: Vec<u32>,
    slots: u32,
}

struct StandEntry {
    entity: Entity,
    stale: bool,
}

struct RegionEntry {
    slot: u32,
    bounds: Aabb,
    keys: Vec<StandKey>,
}

impl Stands {
    /// Each region standing trees: its entity, its slot, and its box in
    /// its own frame.
    pub fn regions(&self) -> impl Iterator<Item = (Entity, u32, &Aabb)> {
        self.regions.iter().map(|(&e, r)| (e, r.slot, &r.bounds))
    }

    fn take_slot(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            self.slots += 1;
            self.slots - 1
        })
    }
}

/// Follow the regions' wood into the stands: a region that comes or
/// changes takes a slot and marks its stands, one that goes gives its slot
/// back and marks its stands, and each marked stand gathers its model's
/// trees from every region again. A stand is spawned the first time a
/// region stands its model and kept after, empty or not.
#[allow(clippy::too_many_arguments)]
pub fn update_stands(
    mut commands: Commands,
    stands: ResMut<Stands>,
    woods: Query<(Entity, Ref<RegionWood>)>,
    mut gone: RemovedComponents<RegionWood>,
    mut tree_stands: Query<&mut TreeStand>,
    mut card_stands: Query<&mut CardStand>,
    render_device: Res<RenderDevice>,
    timers: Res<crate::resources::ClientTimers>,
) {
    let _t = timers.0.scope("stands");
    let stands = stands.into_inner();
    for entity in gone.read() {
        if woods.contains(entity) {
            continue;
        }
        let Some(region) = stands.regions.remove(&entity) else { continue };
        stands.free.push(region.slot);
        for key in region.keys {
            if let Some(stand) = stands.stands.get_mut(&key) {
                stand.stale = true;
            }
        }
    }
    for (entity, wood) in &woods {
        if !wood.is_changed() {
            continue;
        }
        let slot = match stands.regions.get(&entity) {
            Some(region) => region.slot,
            None => stands.take_slot(),
        };
        let keys: Vec<StandKey> = wood.parts.iter().map(|p| p.key).collect();
        let old = stands.regions.insert(entity, RegionEntry { slot, bounds: wood.bounds, keys: keys.clone() });
        for key in old.into_iter().flat_map(|r| r.keys).chain(keys) {
            if let Some(stand) = stands.stands.get_mut(&key) {
                stand.stale = true;
            }
        }
        for part in &wood.parts {
            stands.stands.entry(part.key).or_insert_with(|| {
                let mut stand = commands.spawn((Mesh3d(part.mesh.clone()), Transform::IDENTITY));
                match &part.cards {
                    Some(cards) => stand.insert(CardStand { draw: StandDraw::default(), cards: cards.clone() }),
                    None => stand.insert(TreeStand::default()),
                };
                StandEntry { entity: stand.id(), stale: true }
            });
        }
    }

    let mut gathered: HashMap<StandKey, (Vec<Instance>, Vec<StandRange>)> =
        stands.stands.iter().filter(|(_, s)| s.stale).map(|(k, _)| (*k, Default::default())).collect();
    if gathered.is_empty() {
        return;
    }
    for (entity, wood) in &woods {
        let Some(region) = stands.regions.get(&entity) else { continue };
        for part in &wood.parts {
            let Some((instances, ranges)) = gathered.get_mut(&part.key) else { continue };
            ranges.push(StandRange { slot: region.slot, start: instances.len() as u32, count: part.instances.len() as u32 });
            instances.extend(part.instances.iter().map(|i| {
                let mut i = *i;
                i.turn[3] = region.slot as f32;
                i
            }));
        }
    }
    for (key, (instances, ranges)) in gathered {
        let Some(stand) = stands.stands.get_mut(&key) else { continue };
        stand.stale = false;
        let buffer = (!instances.is_empty()).then(|| {
            render_device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("stand"),
                contents: bytemuck::cast_slice(&instances),
                usage: BufferUsages::VERTEX,
            })
        });
        let draw = StandDraw { buffer, ranges: Arc::new(ranges) };
        if let Ok(mut s) = tree_stands.get_mut(stand.entity) {
            s.0 = draw;
        } else if let Ok(mut s) = card_stands.get_mut(stand.entity) {
            s.draw = draw;
        }
    }
}

pub struct TreeDrawPlugin;

impl Plugin for TreeDrawPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stands>();
        app.add_plugins((
            ExtractComponentPlugin::<TreeStand>::default(),
            ExtractComponentPlugin::<CardStand>::default(),
            ExtractResourcePlugin::<CardBand>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SpecializedMeshPipelines<TreePipeline>>()
            .init_resource::<SpecializedMeshPipelines<CardPipeline>>()
            .init_resource::<RegionTable>()
            .init_resource::<WoodBuffers>()
            .init_resource::<StandDraws>()
            .add_render_command::<Opaque3d, DrawTrees>()
            .add_render_command::<Opaque3d, DrawCards>()
            .add_render_command::<Opaque3dPrepass, DrawTrees>()
            .add_render_command::<Opaque3dPrepass, DrawCards>()
            // `MeshPipeline` is built in `RenderStartup` as well, and this
            // clones it.
            .add_systems(RenderStartup, init_pipelines.after(MeshPipelineSystems))
            .add_systems(ExtractSchedule, extract_region_table)
            .add_systems(
                Render,
                (
                    queue_stands::<TreeStand, TreePipeline, DrawTrees, Opaque3dPrepass>.in_set(RenderSystems::Queue),
                    queue_stands::<TreeStand, TreePipeline, DrawTrees, Opaque3d>.in_set(RenderSystems::Queue),
                    queue_stands::<CardStand, CardPipeline, DrawCards, Opaque3dPrepass>.in_set(RenderSystems::Queue),
                    queue_stands::<CardStand, CardPipeline, DrawCards, Opaque3d>.in_set(RenderSystems::Queue),
                    (prepare_wood_buffers, prepare_stand_draws).in_set(RenderSystems::PrepareResources),
                    prepare_wood_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

/// What every drawing of the wood reads alike: the ring where the models
/// hand over to the cards — its centre, radius and overlap — and the
/// radius about the camera inside which everything fades.
#[derive(Clone, Copy, Default, ShaderType)]
struct ForestUniform {
    band: Vec4,
    near_fade: f32,
}

/// A kind's two card frames as the shader's uniform: width, height and
/// elevation of the side, then the top.
#[derive(Clone, Copy, ShaderType)]
struct CardUniform {
    side: Vec4,
    top: Vec4,
}

impl CardUniform {
    fn of(cards: &Cards) -> Self {
        let v = |c: CardView| Vec4::new(c.width, c.height, c.elevation, 0.0);
        CardUniform { side: v(cards.side), top: v(cards.top) }
    }
}

/// Which pipeline a batch is drawn with: the view's own key, and whether
/// this is the depth prepass's drawing of it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BatchKey {
    view: MeshPipelineKey,
    depth_only: bool,
}

/// A pipeline over the mesh pipeline's descriptor: its shader, and the
/// wood's layout, which takes the mesh's place at group 2.
trait BatchPipeline: Resource + SpecializedMeshPipeline<Key = BatchKey> {
    fn parts(&self) -> (&Handle<Shader>, &MeshPipeline, &BindGroupLayoutDescriptor);
    fn two_sided(&self) -> bool { false }

    /// The mesh pipeline's descriptor for the key, its view layouts and
    /// shader defs kept, with the stand's shader, the instance buffer as a
    /// second vertex buffer, and the wood's layout in the mesh's place.
    /// The prepass's drawing differs in its fragment stage alone — the
    /// silhouette, written to depth and to no colour target — so the
    /// depth the opaque pass tests against is the depth this wrote.
    fn batch_descriptor(&self, key: BatchKey, layout: &MeshVertexBufferLayoutRef, label: &'static str) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let (shader, mesh_pipeline, group) = self.parts();
        let mut descriptor = mesh_pipeline.specialize(key.view, layout)?;
        descriptor.label = Some(if key.depth_only { format!("{label}_depth").into() } else { label.into() });
        descriptor.vertex.shader = shader.clone();
        // The standard lighting the wood is lit by declares the material's
        // group, which a stand never binds: unread, it needs only its index.
        let material_group = ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), MATERIAL_BIND_GROUP_INDEX as u32);
        descriptor.vertex.shader_defs.push(material_group.clone());
        descriptor.vertex.buffers.push(VertexBufferLayout {
            array_stride: size_of::<Instance>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute { format: VertexFormat::Float32x4, offset: 0, shader_location: 10 },
                VertexAttribute { format: VertexFormat::Float32x4, offset: VertexFormat::Float32x4.size(), shader_location: 11 },
            ],
        });
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = shader.clone();
            fragment.shader_defs.push(material_group);
            if key.depth_only {
                fragment.entry_point = Some(DEPTH_ENTRY.into());
                fragment.targets.clear();
            } else {
                fragment.entry_point = Some(COLOUR_ENTRY.into());
            }
        }
        // Where the prepass has already written this silhouette, the
        // colour pass only tests against it. Writing it again would make
        // the hardware hold the depth test back until the shader's
        // discards are known, and every tree behind a tree would be
        // shaded before being thrown away.
        if !key.depth_only && key.view.contains(MeshPipelineKey::DEPTH_PREPASS) {
            if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
                depth_stencil.depth_write_enabled = Some(false);
            }
        }
        if self.two_sided() {
            descriptor.primitive.cull_mode = None;
        }
        // The mesh pipeline binds the view at 0, its binding arrays at 1
        // and the mesh at 2; the wood's group takes the mesh's place.
        descriptor.layout.truncate(2);
        descriptor.layout.push(group.clone());
        Ok(descriptor)
    }
}

#[derive(Resource)]
struct TreePipeline {
    shader: Handle<Shader>,
    /// Held, never read: the shared module's asset stays loaded.
    _shared: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
    layout: BindGroupLayoutDescriptor,
    bind_group_layout: BindGroupLayout,
}

#[derive(Resource)]
struct CardPipeline {
    shader: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
    layout: BindGroupLayoutDescriptor,
    bind_group_layout: BindGroupLayout,
}

fn init_pipelines(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mesh_pipeline: Res<MeshPipeline>,
    render_device: Res<RenderDevice>,
) {
    let wood = BindGroupLayoutEntries::sequential(
        ShaderStages::VERTEX_FRAGMENT,
        (uniform_buffer::<ForestUniform>(false), storage_buffer_read_only::<Mat4>(false)),
    );
    commands.insert_resource(TreePipeline {
        shader: asset_server.load(TREE_SHADER),
        _shared: asset_server.load(SHARED_SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        layout: BindGroupLayoutDescriptor::new("trees_wood", &wood),
        bind_group_layout: render_device.create_bind_group_layout("trees_wood", &wood),
    });
    let card = BindGroupLayoutEntries::sequential(
        ShaderStages::VERTEX_FRAGMENT,
        (
            uniform_buffer::<ForestUniform>(false),
            storage_buffer_read_only::<Mat4>(false),
            uniform_buffer::<CardUniform>(false),
            texture_2d_array(TextureSampleType::Float { filterable: true }),
            sampler(SamplerBindingType::Filtering),
        ),
    );
    commands.insert_resource(CardPipeline {
        shader: asset_server.load(CARD_SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        layout: BindGroupLayoutDescriptor::new("cards_wood", &card),
        bind_group_layout: render_device.create_bind_group_layout("cards_wood", &card),
    });
}

impl BatchPipeline for TreePipeline {
    fn parts(&self) -> (&Handle<Shader>, &MeshPipeline, &BindGroupLayoutDescriptor) {
        (&self.shader, &self.mesh_pipeline, &self.layout)
    }
}

impl SpecializedMeshPipeline for TreePipeline {
    type Key = BatchKey;

    fn specialize(&self, key: Self::Key, layout: &MeshVertexBufferLayoutRef) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        self.batch_descriptor(key, layout, "trees")
    }
}

impl BatchPipeline for CardPipeline {
    fn parts(&self) -> (&Handle<Shader>, &MeshPipeline, &BindGroupLayoutDescriptor) {
        (&self.shader, &self.mesh_pipeline, &self.layout)
    }

    /// A card is seen from either side as the camera swings round it.
    fn two_sided(&self) -> bool { true }
}

impl SpecializedMeshPipeline for CardPipeline {
    type Key = BatchKey;

    fn specialize(&self, key: Self::Key, layout: &MeshVertexBufferLayoutRef) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        self.batch_descriptor(key, layout, "cards")
    }
}

/// A phase a stand is drawn into. The depth prepass takes it for its
/// silhouette alone; the opaque pass after that shades what is left
/// showing. Both draw the same instances through the same vertex stage,
/// so a fragment's depth is the one already in the buffer, and of an
/// overlapping stand only the nearest is ever shaded.
trait BatchPhase: BinnedPhaseItem {
    /// Whether the phase takes the batch for its depth alone.
    const DEPTH_ONLY: bool;

    fn keys(
        draw_function: DrawFunctionId,
        pipeline: CachedRenderPipelineId,
        slabs: Option<MeshSlabs>,
        asset_id: UntypedAssetId,
    ) -> (Self::BatchSetKey, Self::BinKey);
}

impl BatchPhase for Opaque3dPrepass {
    const DEPTH_ONLY: bool = true;

    fn keys(
        draw_function: DrawFunctionId,
        pipeline: CachedRenderPipelineId,
        slabs: Option<MeshSlabs>,
        asset_id: UntypedAssetId,
    ) -> (Self::BatchSetKey, Self::BinKey) {
        (
            OpaqueNoLightmap3dBatchSetKey {
                draw_function,
                pipeline,
                material_bind_group_index: None,
                slabs: slabs.unwrap_or_default(),
            },
            OpaqueNoLightmap3dBinKey { asset_id },
        )
    }
}

impl BatchPhase for Opaque3d {
    const DEPTH_ONLY: bool = false;

    fn keys(
        draw_function: DrawFunctionId,
        pipeline: CachedRenderPipelineId,
        slabs: Option<MeshSlabs>,
        asset_id: UntypedAssetId,
    ) -> (Self::BatchSetKey, Self::BinKey) {
        (
            Opaque3dBatchSetKey {
                draw_function,
                pipeline,
                material_bind_group_index: None,
                slabs: slabs.unwrap_or_default(),
                lightmap_slab: None,
            },
            Opaque3dBinKey { asset_id },
        )
    }
}

/// Each stand of `S` goes into each view's `I` phase with `P` specialised
/// for its mesh and drawn by `D`, unbatched: the stand batches itself. A
/// view without a phase of that kind — one drawn with no depth prepass —
/// is passed over.
#[allow(clippy::too_many_arguments)]
fn queue_stands<S: Stand, P: BatchPipeline, D: 'static, I: BatchPhase>(
    pipeline_cache: Res<PipelineCache>,
    batch_pipeline: Res<P>,
    mut pipelines: ResMut<SpecializedMeshPipelines<P>>,
    mut phases: ResMut<ViewBinnedRenderPhases<I>>,
    draw_functions: Res<DrawFunctions<I>>,
    views: Query<(&RenderVisibleEntities, &ExtractedView)>,
    view_keys: Res<ViewKeyCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    mesh_allocator: Res<MeshAllocator>,
    timers: Res<crate::resources::ClientTimers>,
) {
    let _t = timers.0.scope(if I::DEPTH_ONLY { S::DEPTH_TIMER } else { S::TIMER });
    let draw_function = draw_functions.read().id::<D>();
    for (visible, view) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        // The view's own key, as the mesh pipeline computed it: its
        // prepasses and samples pick the view layout the bind group holds.
        let Some(&view_key) = view_keys.get(&view.retained_view_entity) else { continue };
        let Some(class) = visible.get::<S>() else { continue };
        for &(render_entity, main_entity) in class.entities_cpu_culling.iter() {
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(main_entity) else { continue };
            let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id()) else { continue };
            let key = BatchKey {
                view: view_key | MeshPipelineKey::from_primitive_topology_and_strip_index(mesh.primitive_topology(), mesh.index_format()),
                depth_only: I::DEPTH_ONLY,
            };
            let pipeline = match pipelines.specialize(&pipeline_cache, &batch_pipeline, key, &mesh.layout) {
                Ok(pipeline) => pipeline,
                Err(e) => {
                    warn_once!("trees: no pipeline for this mesh and view: {e:?}");
                    continue;
                }
            };
            let slabs = mesh_allocator.mesh_slabs(&mesh_instance.mesh_asset_id());
            let (batch_set_key, bin_key) = I::keys(draw_function, pipeline, slabs, mesh_instance.mesh_asset_id().into());
            // The phase remembers which bin an entity was in and `add`
            // does not move it: when the key changes — a different sample
            // count, a new pipeline — the stale bin keeps the stand and
            // draws it against a pass it no longer matches. Evicting first
            // costs a lookup and leaves exactly one bin holding each.
            phase.remove(main_entity);
            phase.add(
                batch_set_key,
                bin_key,
                (render_entity, main_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::UnbatchableMesh,
            );
        }
    }
}

/// Each region's frame and box by slot, as of this frame: read from the
/// regions themselves as the render world takes the frame, so the render
/// origin's moves arrive with it. A slot no region holds has no box.
#[derive(Resource, Default)]
struct RegionTable {
    frames: Vec<Mat4>,
    bounds: Vec<Option<(Affine3A, Aabb)>>,
}

fn extract_region_table(mut table: ResMut<RegionTable>, stands: Extract<Res<Stands>>, transforms: Extract<Query<&GlobalTransform>>) {
    let slots = stands.slots as usize;
    table.frames.clear();
    // A binding is never empty, so the table holds one frame at least.
    table.frames.resize(slots.max(1), Mat4::IDENTITY);
    table.bounds.clear();
    table.bounds.resize(slots, None);
    for (entity, slot, bounds) in stands.regions() {
        let Ok(transform) = transforms.get(entity) else { continue };
        table.frames[slot as usize] = transform.to_matrix();
        table.bounds[slot as usize] = Some((transform.affine(), *bounds));
    }
}

/// The wood's uniform and table of frames, written once a frame, and the
/// groups that bind them: one for the trees, and one for each card
/// texture standing. A bind group holds the buffers it was made against,
/// so all of them go when either is re-allocated.
#[derive(Resource, Default)]
struct WoodBuffers {
    forest: UniformBuffer<ForestUniform>,
    frames: StorageBuffer<Vec<Mat4>>,
    held: Option<(BufferId, BufferId)>,
    trees: Option<BindGroup>,
    /// One per card texture: a card's group binds its model's pictures
    /// and their frames beside the wood's.
    cards: HashMap<AssetId<Image>, BindGroup>,
}

fn prepare_wood_buffers(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    band: Option<Res<CardBand>>,
    table: Res<RegionTable>,
    buffers: ResMut<WoodBuffers>,
    timers: Res<crate::resources::ClientTimers>,
) {
    let _t = timers.0.scope("regions");
    let buffers = buffers.into_inner();
    let band = band.map_or(Vec4::ZERO, |b| Vec4::new(b.center.x, b.center.y, b.inner, b.overlap));
    buffers.forest.set(ForestUniform { band, near_fade: NEAR_FADE_RADIUS });
    buffers.forest.write_buffer(&render_device, &render_queue);
    buffers.frames.set(table.frames.clone());
    buffers.frames.write_buffer(&render_device, &render_queue);
    let held = buffers.forest.buffer().map(Buffer::id).zip(buffers.frames.buffer().map(Buffer::id));
    if buffers.held != held {
        buffers.held = held;
        buffers.trees = None;
        buffers.cards.clear();
    }
}

/// The groups that read the wood's buffers: one for the trees, and one
/// for each card texture standing. A card stand whose texture is not on
/// the GPU yet has no group, and the draw skips it.
fn prepare_wood_bind_groups(
    tree_pipeline: Res<TreePipeline>,
    card_pipeline: Res<CardPipeline>,
    render_device: Res<RenderDevice>,
    images: Res<RenderAssets<GpuImage>>,
    buffers: ResMut<WoodBuffers>,
    cards: Query<&CardStand>,
    timers: Res<crate::resources::ClientTimers>,
) {
    let _t = timers.0.scope("bgroups");
    let WoodBuffers { forest, frames, trees, cards: groups, .. } = buffers.into_inner();
    let (Some(forest), Some(frames)) = (forest.binding(), frames.binding()) else { return };
    if trees.is_none() {
        *trees = Some(render_device.create_bind_group(
            "trees_wood",
            &tree_pipeline.bind_group_layout,
            &BindGroupEntries::sequential((forest.clone(), frames.clone())),
        ));
    }
    for stand in &cards {
        let id = stand.cards.texture.id();
        if groups.contains_key(&id) {
            continue;
        }
        let Some(image) = images.get(&stand.cards.texture) else { continue };
        let mut uniform = encase::UniformBuffer::new(Vec::new());
        uniform.write(&CardUniform::of(&stand.cards)).expect("a uniform of plain floats writes");
        let uniform = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("cards_frames"),
            contents: uniform.as_ref(),
            usage: BufferUsages::UNIFORM,
        });
        let group = render_device.create_bind_group(
            "cards_wood",
            &card_pipeline.bind_group_layout,
            &BindGroupEntries::sequential((
                forest.clone(),
                frames.clone(),
                uniform.as_entire_binding(),
                &image.texture_view,
                &image.sampler,
            )),
        );
        groups.insert(id, group);
    }
}

/// One indirect indexed draw, as the GPU reads it.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct IndirectArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

/// Every view's draws of every stand, in one indirect buffer written once
/// a frame: for a view and a stand, where its draws start and how many.
#[derive(Resource)]
struct StandDraws {
    args: RawBufferVec<IndirectArgs>,
    at: HashMap<(RetainedViewEntity, Entity), (u32, u32)>,
}

impl Default for StandDraws {
    fn default() -> Self {
        StandDraws { args: RawBufferVec::new(BufferUsages::INDIRECT), at: HashMap::new() }
    }
}

/// For each view drawing the wood and each stand, the runs whose region's
/// box the view can see, as one indirect draw each of the stand's mesh.
#[allow(clippy::too_many_arguments)]
fn prepare_stand_draws(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    draws: ResMut<StandDraws>,
    table: Res<RegionTable>,
    views: Query<&ExtractedView>,
    phases: Res<ViewBinnedRenderPhases<Opaque3d>>,
    tree_stands: Query<(Entity, &MainEntity, &TreeStand)>,
    card_stands: Query<(Entity, &MainEntity, &CardStand)>,
    meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    mesh_allocator: Res<MeshAllocator>,
    timers: Res<crate::resources::ClientTimers>,
) {
    let _t = timers.0.scope("stand_draws");
    let draws = draws.into_inner();
    draws.args.clear();
    draws.at.clear();
    // Each stand's mesh as an indirect draw names it: its index count and
    // where its indices and vertices sit in their slabs.
    let mut stand_meshes = Vec::new();
    let stands = tree_stands
        .iter()
        .map(|(e, m, s)| (e, *m, &s.0))
        .chain(card_stands.iter().map(|(e, m, s)| (e, *m, &s.draw)));
    for (entity, main_entity, stand) in stands {
        if stand.buffer.is_none() {
            continue;
        }
        let Some(instance) = render_mesh_instances.render_mesh_queue_data(main_entity) else { continue };
        let id = instance.mesh_asset_id();
        let Some(mesh) = meshes.get(id) else { continue };
        let RenderMeshBufferInfo::Indexed { count, .. } = mesh.buffer_info else { continue };
        let (Some(vertices), Some(indices)) = (mesh_allocator.mesh_vertex_slice(&id), mesh_allocator.mesh_index_slice(&id)) else {
            continue;
        };
        stand_meshes.push((entity, stand.ranges.clone(), count, indices.range.start, vertices.range.start as i32));
    }
    for view in &views {
        if !phases.contains_key(&view.retained_view_entity) {
            continue;
        }
        let clip_from_world = view
            .clip_from_world
            .unwrap_or_else(|| view.clip_from_view * view.world_from_view.to_matrix().inverse());
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let seen: Vec<bool> = table
            .bounds
            .iter()
            .map(|b| b.as_ref().is_some_and(|(frame, bounds)| frustum.intersects_obb(bounds, frame, true, true)))
            .collect();
        for (entity, ranges, index_count, first_index, base_vertex) in &stand_meshes {
            let start = draws.args.len() as u32;
            for range in ranges.iter() {
                if !seen.get(range.slot as usize).copied().unwrap_or(false) {
                    continue;
                }
                draws.args.push(IndirectArgs {
                    index_count: *index_count,
                    instance_count: range.count,
                    first_index: *first_index,
                    base_vertex: *base_vertex,
                    first_instance: range.start,
                });
            }
            let count = draws.args.len() as u32 - start;
            if count > 0 {
                draws.at.insert((view.retained_view_entity, *entity), (start, count));
            }
        }
    }
    draws.args.write_buffer(&render_device, &render_queue);
}

type DrawTrees = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetTreeWood<2>,
    DrawStand<TreeStand>,
);

type DrawCards = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetCardWood<2>,
    DrawStand<CardStand>,
);

/// The wood's group for the trees.
struct SetTreeWood<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTreeWood<I> {
    type Param = SRes<WoodBuffers>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(group) = buffers.into_inner().trees.as_ref() else { return RenderCommandResult::Skip };
        pass.set_bind_group(I, group, &[]);
        RenderCommandResult::Success
    }
}

/// The wood's group for this stand's card texture.
struct SetCardWood<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetCardWood<I> {
    type Param = SRes<WoodBuffers>;
    type ViewQuery = ();
    type ItemQuery = Read<CardStand>;

    fn render<'w>(
        _item: &P,
        _view: (),
        stand: Option<&'w CardStand>,
        buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(stand) = stand else { return RenderCommandResult::Skip };
        let Some(group) = buffers.into_inner().cards.get(&stand.cards.texture.id()) else { return RenderCommandResult::Skip };
        pass.set_bind_group(I, group, &[]);
        RenderCommandResult::Success
    }
}

/// A stand's draws for this view in one call: the stand's mesh, its
/// instance buffer, and the view's indirect draws of it.
struct DrawStand<S: Stand>(std::marker::PhantomData<S>);

impl<P: PhaseItem, S: Stand> RenderCommand<P> for DrawStand<S> {
    type Param = (SRes<StandDraws>, SRes<RenderMeshInstances>, SRes<MeshAllocator>, SRes<RenderAssets<RenderMesh>>);
    type ViewQuery = Read<ExtractedView>;
    type ItemQuery = Read<S>;

    fn render<'w>(
        item: &P,
        view: &'w ExtractedView,
        stand: Option<&'w S>,
        (draws, render_mesh_instances, mesh_allocator, meshes): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let draws = draws.into_inner();
        let mesh_allocator = mesh_allocator.into_inner();
        let Some(stand) = stand else { return RenderCommandResult::Skip };
        let Some(instances) = stand.draw().buffer.as_ref() else { return RenderCommandResult::Skip };
        let Some(&(start, count)) = draws.at.get(&(view.retained_view_entity, item.entity())) else {
            return RenderCommandResult::Skip;
        };
        let Some(args) = draws.args.buffer() else { return RenderCommandResult::Skip };
        let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(item.main_entity()) else {
            return RenderCommandResult::Skip;
        };
        let id = mesh_instance.mesh_asset_id();
        let Some(gpu_mesh) = meshes.into_inner().get(id) else { return RenderCommandResult::Skip };
        let RenderMeshBufferInfo::Indexed { index_format, .. } = gpu_mesh.buffer_info else { return RenderCommandResult::Skip };
        let (Some(vertices), Some(indices)) = (mesh_allocator.mesh_vertex_slice(&id), mesh_allocator.mesh_index_slice(&id)) else {
            return RenderCommandResult::Skip;
        };
        pass.set_vertex_buffer(0, vertices.buffer.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.set_index_buffer(indices.buffer.slice(..), index_format);
        pass.multi_draw_indexed_indirect(args, start as u64 * size_of::<IndirectArgs>() as u64, count);
        RenderCommandResult::Success
    }
}
