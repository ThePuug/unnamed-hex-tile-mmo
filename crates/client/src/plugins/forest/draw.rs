//! The instanced draw: a region's trees of one variation as one draw call
//! from an instance buffer built once with the region, near as the
//! variation's model and far as its card. The buffer lives on the main
//! world's entity and is only cloned into the render world; the region's
//! transform is the draw's own uniform, written each frame, so the draw
//! never reads the mesh's uniform and needs no change to how the rest of
//! the scene is drawn. Trees and cards are opaque, cast no shadow and are
//! queued into the opaque phase by their own visibility class, one
//! bounding box per batch. What stands on the camera's sightline to the
//! player is seen through: the sightline rides in the same uniform and
//! the fragment shader dithers out what lies inside its tunnel, so the
//! trees stay opaque and unsorted. A card is a quad the shader turns to
//! the camera, with the model's baked picture on it: the card pipeline
//! binds the variation's card texture and frames beside the region.

use std::sync::Arc;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{self, VisibilityClass};
use bevy::core_pipeline::core_3d::{Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::{lifetimeless::*, SystemParamItem};
use bevy::mesh::{MeshVertexBufferLayoutRef, VertexBufferLayout};
use bevy::pbr::{MeshPipeline, MeshPipelineKey, RenderMeshInstances, SetMeshViewBindGroup, SetMeshViewBindingArrayBindGroup, ViewKeyCache};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::mesh::{allocator::MeshAllocator, RenderMesh, RenderMeshBufferInfo};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::{
    AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, PhaseItem, RenderCommand, RenderCommandResult, SetItemPipeline,
    TrackedRenderPass, ViewBinnedRenderPhases,
};
use bevy::render::render_resource::binding_types::{sampler, texture_2d_array, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, RenderVisibleEntities};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bytemuck::{Pod, Zeroable};

use crate::systems::camera::{Sightline, NEAR_FADE_RADIUS, SIGHTLINE_RADIUS};

const TREE_SHADER: &str = "shaders/trees.wgsl";
const CARD_SHADER: &str = "shaders/cards.wgsl";
/// What both import. A module is resolved only once its asset is loaded,
/// and a pipeline whose import is missing waits forever and says nothing,
/// so the handle is held here for the life of the app.
const SHARED_SHADER: &str = "shaders/forest_shared.wgsl";

/// One tree as the shader reads it: its place in the region's frame and
/// its scale, then the cosine and sine of its turn; for a card, its side
/// layer in the card texture, its mirror, one or minus one, and the
/// model's height in world units.
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

    /// A card: the side layer of its variation, mirrored by its turn,
    /// facing left or right of the front, and `height` tall as scaled,
    /// which the shader centres the picture on.
    pub fn card(translation: Vec3, yaw: f32, scale: f32, side_layer: u32, height: f32) -> Self {
        let mirror = if yaw.sin() < 0.0 { -1.0 } else { 1.0 };
        Instance {
            pos_scale: [translation.x, translation.y, translation.z, scale],
            turn: [side_layer as f32, mirror, height, 0.0],
        }
    }
}

/// A batch of instances the draw takes its buffer and count from.
pub trait Batch: Component {
    fn buffer(&self) -> &Buffer;
    fn len(&self) -> u32;
}

/// The instance buffer of `instances`, built now, with the bounding box
/// of trees up to `reach` tall and wide about them.
fn instance_buffer(render_device: &RenderDevice, instances: &[Instance], reach: f32) -> (Buffer, Aabb) {
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("trees"),
        contents: bytemuck::cast_slice(instances),
        usage: BufferUsages::VERTEX,
    });
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for i in instances {
        let p = Vec3::new(i.pos_scale[0], i.pos_scale[1], i.pos_scale[2]);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let margin = Vec3::new(reach, 0.0, reach);
    (buffer, Aabb::from_min_max(lo - margin, hi + margin + Vec3::Y * reach))
}

/// A region's trees of one variation: the instance buffer and its count.
/// The entity holds the variation's mesh, a bounding box over every
/// instance, and no material, so nothing else draws it.
#[derive(Component, Clone)]
#[require(VisibilityClass)]
#[component(on_add = visibility::add_visibility_class::<TreeBatch>)]
pub struct TreeBatch {
    pub buffer: Buffer,
    pub len: u32,
}

impl TreeBatch {
    pub fn new(render_device: &RenderDevice, instances: &[Instance], reach: f32) -> (TreeBatch, Aabb) {
        let (buffer, aabb) = instance_buffer(render_device, instances, reach);
        (TreeBatch { buffer, len: instances.len() as u32 }, aabb)
    }
}

impl Batch for TreeBatch {
    fn buffer(&self) -> &Buffer { &self.buffer }
    fn len(&self) -> u32 { self.len }
}

/// One view of a kind's card, as the model declared it: the frame in
/// world units and the elevation it was baked from, in radians.
#[derive(Clone, Copy, Debug)]
pub struct CardView {
    pub width: f32,
    pub height: f32,
    pub elevation: f32,
}

/// A kind's cards: the texture array, a layer per variation and view,
/// side then top, and the two views' frames.
#[derive(Debug)]
pub struct Cards {
    pub texture: Handle<Image>,
    pub side: CardView,
    pub top: CardView,
}

/// A region's trees of one variation as cards: the instance buffer and
/// its count, and the kind's cards. The entity holds the shared quad.
#[derive(Component, Clone)]
#[require(VisibilityClass)]
#[component(on_add = visibility::add_visibility_class::<CardBatch>)]
pub struct CardBatch {
    pub buffer: Buffer,
    pub len: u32,
    pub cards: Arc<Cards>,
}

impl CardBatch {
    pub fn new(render_device: &RenderDevice, instances: &[Instance], reach: f32, cards: Arc<Cards>) -> (CardBatch, Aabb) {
        let (buffer, aabb) = instance_buffer(render_device, instances, reach);
        (CardBatch { buffer, len: instances.len() as u32, cards }, aabb)
    }
}

impl Batch for CardBatch {
    fn buffer(&self) -> &Buffer { &self.buffer }
    fn len(&self) -> u32 { self.len }
}

/// The batch's world transform, extracted with it each frame: the region's,
/// which the render origin moves.
#[derive(Component, Clone, Copy)]
pub struct TreeTransform(pub Mat4);

impl ExtractComponent for TreeBatch {
    type QueryData = (&'static TreeBatch, &'static GlobalTransform);
    type QueryFilter = ();
    type Out = (TreeBatch, TreeTransform);

    fn extract_component((batch, transform): QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        Some((batch.clone(), TreeTransform(transform.to_matrix())))
    }
}

impl ExtractComponent for CardBatch {
    type QueryData = (&'static CardBatch, &'static GlobalTransform);
    type QueryFilter = ();
    type Out = (CardBatch, TreeTransform);

    fn extract_component((batch, transform): QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        Some((batch.clone(), TreeTransform(transform.to_matrix())))
    }
}

pub struct TreeDrawPlugin;

impl Plugin for TreeDrawPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<TreeBatch>::default(),
            ExtractComponentPlugin::<CardBatch>::default(),
            ExtractResourcePlugin::<Sightline>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SpecializedMeshPipelines<TreePipeline>>()
            .init_resource::<SpecializedMeshPipelines<CardPipeline>>()
            .add_render_command::<Opaque3d, DrawTrees>()
            .add_render_command::<Opaque3d, DrawCards>()
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(
                Render,
                (
                    queue_batches::<TreeBatch, TreePipeline, DrawTrees>.in_set(RenderSystems::Queue),
                    queue_batches::<CardBatch, CardPipeline, DrawCards>.in_set(RenderSystems::Queue),
                    prepare_tree_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                    prepare_card_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

/// The region's transform as the shader's uniform, with the sightline:
/// the player's centre and the tunnel's radius about the line from the
/// camera to it, zero when nothing is seen through, and the radius about
/// the camera inside which everything fades.
#[derive(Clone, Copy, ShaderType)]
struct RegionUniform {
    world_from_local: Mat4,
    sightline: Vec4,
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

/// A pipeline over the mesh pipeline's descriptor: its shader, and the
/// layout that takes the mesh's place at group 2.
trait BatchPipeline: Resource + SpecializedMeshPipeline<Key = MeshPipelineKey> {
    fn parts(&self) -> (&Handle<Shader>, &MeshPipeline, &BindGroupLayoutDescriptor);
    fn two_sided(&self) -> bool { false }

    /// The mesh pipeline's descriptor for the key, its view layouts and
    /// shader defs kept, with the batch shader, the instance buffer as a
    /// second vertex buffer, and the batch's layout in the mesh's place.
    fn batch_descriptor(&self, key: MeshPipelineKey, layout: &MeshVertexBufferLayoutRef, label: &'static str) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let (shader, mesh_pipeline, group) = self.parts();
        let mut descriptor = mesh_pipeline.specialize(key, layout)?;
        descriptor.label = Some(label.into());
        descriptor.vertex.shader = shader.clone();
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
        }
        if self.two_sided() {
            descriptor.primitive.cull_mode = None;
        }
        // The mesh pipeline binds the view at 0, its binding arrays at 1
        // and the mesh at 2; the batch's group takes the mesh's place.
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
    let region = BindGroupLayoutEntries::single(ShaderStages::VERTEX_FRAGMENT, uniform_buffer::<RegionUniform>(false));
    commands.insert_resource(TreePipeline {
        shader: asset_server.load(TREE_SHADER),
        _shared: asset_server.load(SHARED_SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        layout: BindGroupLayoutDescriptor::new("trees_region", &region),
        bind_group_layout: render_device.create_bind_group_layout("trees_region", &region),
    });
    let card = BindGroupLayoutEntries::with_indices(
        ShaderStages::VERTEX_FRAGMENT,
        (
            (0, uniform_buffer::<RegionUniform>(false)),
            (1, uniform_buffer::<CardUniform>(false)),
            (2, texture_2d_array(TextureSampleType::Float { filterable: true })),
            (3, sampler(SamplerBindingType::Filtering)),
        ),
    );
    commands.insert_resource(CardPipeline {
        shader: asset_server.load(CARD_SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        layout: BindGroupLayoutDescriptor::new("cards_region", &card),
        bind_group_layout: render_device.create_bind_group_layout("cards_region", &card),
    });
}

impl BatchPipeline for TreePipeline {
    fn parts(&self) -> (&Handle<Shader>, &MeshPipeline, &BindGroupLayoutDescriptor) {
        (&self.shader, &self.mesh_pipeline, &self.layout)
    }
}

impl SpecializedMeshPipeline for TreePipeline {
    type Key = MeshPipelineKey;

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
    type Key = MeshPipelineKey;

    fn specialize(&self, key: Self::Key, layout: &MeshVertexBufferLayoutRef) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        self.batch_descriptor(key, layout, "cards")
    }
}

/// Each batch of `B` visible from a view goes into the view's opaque
/// phase with `P` specialised for its mesh and drawn by `D`, unbatched:
/// the draw batches itself.
#[allow(clippy::too_many_arguments)]
fn queue_batches<B: Batch, P: BatchPipeline, D: 'static>(
    pipeline_cache: Res<PipelineCache>,
    batch_pipeline: Res<P>,
    mut pipelines: ResMut<SpecializedMeshPipelines<P>>,
    mut phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    draw_functions: Res<DrawFunctions<Opaque3d>>,
    views: Query<(&RenderVisibleEntities, &ExtractedView)>,
    view_keys: Res<ViewKeyCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    mesh_allocator: Res<MeshAllocator>,
    mut change_tick: Local<Tick>,
) {
    let draw_function = draw_functions.read().id::<D>();
    for (visible, view) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        // The view's own key, as the mesh pipeline computed it: its
        // prepasses and samples pick the view layout the bind group holds.
        let Some(&view_key) = view_keys.get(&view.retained_view_entity) else { continue };
        for &(render_entity, main_entity) in visible.get::<B>().iter() {
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(main_entity) else { continue };
            let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id) else { continue };
            let key = view_key | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());
            let pipeline = match pipelines.specialize(&pipeline_cache, &batch_pipeline, key, &mesh.layout) {
                Ok(pipeline) => pipeline,
                Err(e) => {
                    warn_once!("trees: no pipeline for this mesh and view: {e:?}");
                    continue;
                }
            };
            let (vertex_slab, index_slab) = mesh_allocator.mesh_slabs(&mesh_instance.mesh_asset_id);
            let next = change_tick.get() + 1;
            change_tick.set(next);
            phase.add(
                Opaque3dBatchSetKey {
                    draw_function,
                    pipeline,
                    material_bind_group_index: None,
                    vertex_slab: vertex_slab.unwrap_or_default(),
                    index_slab,
                    lightmap_slab: None,
                },
                Opaque3dBinKey { asset_id: mesh_instance.mesh_asset_id.into() },
                (render_entity, main_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::UnbatchableMesh,
                *change_tick,
            );
        }
    }
}

/// A batch's group 2: the region's uniform, made once and written every
/// frame from the extracted transform and sightline, and the bind group
/// holding it with whatever else the batch's pipeline binds.
#[derive(Component)]
struct BatchBindGroup {
    region: Buffer,
    bind_group: BindGroup,
}

/// The region uniform's bytes for this frame.
fn region_bytes(transform: &TreeTransform, sightline: Option<&Sightline>) -> Vec<u8> {
    let sightline = match sightline.and_then(|s| s.player) {
        Some(p) => p.extend(SIGHTLINE_RADIUS),
        None => Vec4::ZERO,
    };
    let uniform = RegionUniform { world_from_local: transform.0, sightline, near_fade: NEAR_FADE_RADIUS };
    let mut bytes = encase::UniformBuffer::new(Vec::new());
    bytes.write(&uniform).expect("a uniform of plain floats writes");
    bytes.into_inner()
}

fn region_buffer(render_device: &RenderDevice, bytes: &[u8]) -> Buffer {
    render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("trees_region"),
        contents: bytes,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    })
}

fn prepare_tree_bind_groups(
    mut commands: Commands,
    pipeline: Res<TreePipeline>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    sightline: Option<Res<Sightline>>,
    batches: Query<(Entity, &TreeTransform, Option<&BatchBindGroup>), With<TreeBatch>>,
) {
    for (entity, transform, existing) in &batches {
        let bytes = region_bytes(transform, sightline.as_deref());
        match existing {
            Some(bg) => render_queue.write_buffer(&bg.region, 0, &bytes),
            None => {
                let region = region_buffer(&render_device, &bytes);
                let bind_group = render_device.create_bind_group(
                    "trees_region",
                    &pipeline.bind_group_layout,
                    &BindGroupEntries::single(region.as_entire_binding()),
                );
                commands.entity(entity).insert(BatchBindGroup { region, bind_group });
            }
        }
    }
}

/// A card batch's group 2 waits for its texture: until the image is on
/// the GPU the batch has no bind group and the draw skips it.
fn prepare_card_bind_groups(
    mut commands: Commands,
    pipeline: Res<CardPipeline>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    sightline: Option<Res<Sightline>>,
    images: Res<RenderAssets<GpuImage>>,
    batches: Query<(Entity, &TreeTransform, &CardBatch, Option<&BatchBindGroup>)>,
) {
    for (entity, transform, batch, existing) in &batches {
        let bytes = region_bytes(transform, sightline.as_deref());
        match existing {
            Some(bg) => render_queue.write_buffer(&bg.region, 0, &bytes),
            None => {
                let Some(image) = images.get(&batch.cards.texture) else { continue };
                let region = region_buffer(&render_device, &bytes);
                let mut card_bytes = encase::UniformBuffer::new(Vec::new());
                card_bytes.write(&CardUniform::of(&batch.cards)).expect("a uniform of plain floats writes");
                let card = render_device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("cards_frames"),
                    contents: card_bytes.as_ref(),
                    usage: BufferUsages::UNIFORM,
                });
                let bind_group = render_device.create_bind_group(
                    "cards_region",
                    &pipeline.bind_group_layout,
                    &BindGroupEntries::with_indices((
                        (0, region.as_entire_binding()),
                        (1, card.as_entire_binding()),
                        (2, &image.texture_view),
                        (3, &image.sampler),
                    )),
                );
                commands.entity(entity).insert(BatchBindGroup { region, bind_group });
            }
        }
    }
}

type DrawTrees = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetBatchBindGroup<2>,
    DrawBatch<TreeBatch>,
);

type DrawCards = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetBatchBindGroup<2>,
    DrawBatch<CardBatch>,
);

struct SetBatchBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetBatchBindGroup<I> {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<BatchBindGroup>;

    fn render<'w>(
        _item: &P,
        _view: (),
        group: Option<&'w BatchBindGroup>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(group) = group else { return RenderCommandResult::Skip };
        pass.set_bind_group(I, &group.bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct DrawBatch<B: Batch>(std::marker::PhantomData<B>);

impl<P: PhaseItem, B: Batch> RenderCommand<P> for DrawBatch<B> {
    type Param = (SRes<RenderAssets<RenderMesh>>, SRes<RenderMeshInstances>, SRes<MeshAllocator>);
    type ViewQuery = ();
    type ItemQuery = Read<B>;

    fn render<'w>(
        item: &P,
        _view: (),
        batch: Option<&'w B>,
        (meshes, render_mesh_instances, mesh_allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let mesh_allocator = mesh_allocator.into_inner();
        let Some(batch) = batch else { return RenderCommandResult::Skip };
        let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(item.main_entity()) else {
            return RenderCommandResult::Skip;
        };
        let Some(gpu_mesh) = meshes.into_inner().get(mesh_instance.mesh_asset_id) else { return RenderCommandResult::Skip };
        let Some(vertex_slice) = mesh_allocator.mesh_vertex_slice(&mesh_instance.mesh_asset_id) else {
            return RenderCommandResult::Skip;
        };
        pass.set_vertex_buffer(0, vertex_slice.buffer.slice(..));
        pass.set_vertex_buffer(1, batch.buffer().slice(..));
        match &gpu_mesh.buffer_info {
            RenderMeshBufferInfo::Indexed { index_format, count } => {
                let Some(index_slice) = mesh_allocator.mesh_index_slice(&mesh_instance.mesh_asset_id) else {
                    return RenderCommandResult::Skip;
                };
                pass.set_index_buffer(index_slice.buffer.slice(..), *index_format);
                pass.draw_indexed(
                    index_slice.range.start..(index_slice.range.start + count),
                    vertex_slice.range.start as i32,
                    0..batch.len(),
                );
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vertex_slice.range, 0..batch.len());
            }
        }
        RenderCommandResult::Success
    }
}
