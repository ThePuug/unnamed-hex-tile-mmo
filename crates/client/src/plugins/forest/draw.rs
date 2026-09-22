//! The instanced draw: a region's trees of one variation as one draw call
//! from an instance buffer built once with the region. The buffer lives on
//! the main world's entity and is only cloned into the render world; the
//! region's transform is the draw's own uniform, written each frame, so
//! the draw never reads the mesh's uniform and needs no change to how the
//! rest of the scene is drawn. Trees are opaque, cast no shadow and are
//! queued into the opaque phase by their own visibility class, one
//! bounding box per batch. What stands on the camera's sightline to the
//! player is seen through: the sightline rides in the same uniform and
//! the fragment shader dithers out what lies inside its tunnel, so the
//! trees stay opaque and unsorted.

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
use bevy::render::render_resource::binding_types::uniform_buffer;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::view::{ExtractedView, RenderVisibleEntities};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bytemuck::{Pod, Zeroable};

use crate::systems::camera::{Sightline, NEAR_FADE_RADIUS, SIGHTLINE_RADIUS};

const SHADER: &str = "shaders/trees.wgsl";

/// One tree as the shader reads it: its place in the region's frame and
/// its scale, then the cosine and sine of its turn.
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
    /// A batch of `instances`, its buffer built now, with the bounding box
    /// of trees up to `reach` tall and wide about them.
    pub fn new(render_device: &RenderDevice, instances: &[Instance], reach: f32) -> (TreeBatch, Aabb) {
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
        let aabb = Aabb::from_min_max(lo - margin, hi + margin + Vec3::Y * reach);
        (TreeBatch { buffer, len: instances.len() as u32 }, aabb)
    }
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

pub struct TreeDrawPlugin;

impl Plugin for TreeDrawPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ExtractComponentPlugin::<TreeBatch>::default(), ExtractResourcePlugin::<Sightline>::default()));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<SpecializedMeshPipelines<TreePipeline>>()
            .add_render_command::<Opaque3d, DrawTrees>()
            .add_systems(RenderStartup, init_tree_pipeline)
            .add_systems(
                Render,
                (
                    queue_trees.in_set(RenderSystems::Queue),
                    prepare_tree_bind_groups.in_set(RenderSystems::PrepareBindGroups),
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

#[derive(Resource)]
struct TreePipeline {
    shader: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
    region_layout: BindGroupLayoutDescriptor,
    region_bind_group_layout: BindGroupLayout,
}

fn init_tree_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mesh_pipeline: Res<MeshPipeline>,
    render_device: Res<RenderDevice>,
) {
    let entries = BindGroupLayoutEntries::single(ShaderStages::VERTEX_FRAGMENT, uniform_buffer::<RegionUniform>(false));
    commands.insert_resource(TreePipeline {
        shader: asset_server.load(SHADER),
        mesh_pipeline: mesh_pipeline.clone(),
        region_layout: BindGroupLayoutDescriptor::new("trees_region", &entries),
        region_bind_group_layout: render_device.create_bind_group_layout("trees_region", &entries),
    });
}

impl SpecializedMeshPipeline for TreePipeline {
    type Key = MeshPipelineKey;

    /// The mesh pipeline's descriptor for the key, its view layouts and
    /// shader defs kept, with the tree shader, the instance buffer as a
    /// second vertex buffer, and the region's uniform in the mesh's place.
    fn specialize(&self, key: Self::Key, layout: &MeshVertexBufferLayoutRef) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;
        descriptor.label = Some("trees".into());
        descriptor.vertex.shader = self.shader.clone();
        descriptor.vertex.buffers.push(VertexBufferLayout {
            array_stride: size_of::<Instance>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute { format: VertexFormat::Float32x4, offset: 0, shader_location: 10 },
                VertexAttribute { format: VertexFormat::Float32x4, offset: VertexFormat::Float32x4.size(), shader_location: 11 },
            ],
        });
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = self.shader.clone();
        }
        // The mesh pipeline binds the view at 0, its binding arrays at 1
        // and the mesh at 2; the region takes the mesh's place.
        descriptor.layout.truncate(2);
        descriptor.layout.push(self.region_layout.clone());
        Ok(descriptor)
    }
}

/// Each batch visible from a view goes into the view's opaque phase with
/// the pipeline specialised for its mesh, unbatched: the draw batches
/// itself.
#[allow(clippy::too_many_arguments)]
fn queue_trees(
    pipeline_cache: Res<PipelineCache>,
    tree_pipeline: Res<TreePipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<TreePipeline>>,
    mut phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    draw_functions: Res<DrawFunctions<Opaque3d>>,
    views: Query<(&RenderVisibleEntities, &ExtractedView)>,
    view_keys: Res<ViewKeyCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    mesh_allocator: Res<MeshAllocator>,
    mut change_tick: Local<Tick>,
) {
    let draw_function = draw_functions.read().id::<DrawTrees>();
    for (visible, view) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else { continue };
        // The view's own key, as the mesh pipeline computed it: its
        // prepasses and samples pick the view layout the bind group holds.
        let Some(&view_key) = view_keys.get(&view.retained_view_entity) else { continue };
        for &(render_entity, main_entity) in visible.get::<TreeBatch>().iter() {
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(main_entity) else { continue };
            let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id) else { continue };
            let key = view_key | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());
            let pipeline = match pipelines.specialize(&pipeline_cache, &tree_pipeline, key, &mesh.layout) {
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

/// The region's uniform and bind group, made once per batch and written
/// every frame from the extracted transform and sightline.
#[derive(Component)]
struct TreeBindGroup {
    uniform: Buffer,
    bind_group: BindGroup,
}

fn prepare_tree_bind_groups(
    mut commands: Commands,
    pipeline: Res<TreePipeline>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    sightline: Option<Res<Sightline>>,
    batches: Query<(Entity, &TreeTransform, Option<&TreeBindGroup>), With<TreeBatch>>,
) {
    let sightline = match sightline.as_ref().and_then(|s| s.player) {
        Some(p) => p.extend(SIGHTLINE_RADIUS),
        None => Vec4::ZERO,
    };
    for (entity, transform, existing) in &batches {
        let uniform = RegionUniform { world_from_local: transform.0, sightline, near_fade: NEAR_FADE_RADIUS };
        let mut bytes = encase::UniformBuffer::new(Vec::new());
        bytes.write(&uniform).expect("a uniform of plain floats writes");
        match existing {
            Some(bg) => render_queue.write_buffer(&bg.uniform, 0, bytes.as_ref()),
            None => {
                let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("trees_region"),
                    contents: bytes.as_ref(),
                    usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                });
                let bind_group = render_device.create_bind_group(
                    "trees_region",
                    &pipeline.region_bind_group_layout,
                    &BindGroupEntries::single(buffer.as_entire_binding()),
                );
                commands.entity(entity).insert(TreeBindGroup { uniform: buffer, bind_group });
            }
        }
    }
}

type DrawTrees = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetRegionBindGroup<2>,
    DrawTreeBatch,
);

struct SetRegionBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetRegionBindGroup<I> {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<TreeBindGroup>;

    fn render<'w>(
        _item: &P,
        _view: (),
        region: Option<&'w TreeBindGroup>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(region) = region else { return RenderCommandResult::Skip };
        pass.set_bind_group(I, &region.bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct DrawTreeBatch;

impl<P: PhaseItem> RenderCommand<P> for DrawTreeBatch {
    type Param = (SRes<RenderAssets<RenderMesh>>, SRes<RenderMeshInstances>, SRes<MeshAllocator>);
    type ViewQuery = ();
    type ItemQuery = Read<TreeBatch>;

    fn render<'w>(
        item: &P,
        _view: (),
        batch: Option<&'w TreeBatch>,
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
        pass.set_vertex_buffer(1, batch.buffer.slice(..));
        match &gpu_mesh.buffer_info {
            RenderMeshBufferInfo::Indexed { index_format, count } => {
                let Some(index_slice) = mesh_allocator.mesh_index_slice(&mesh_instance.mesh_asset_id) else {
                    return RenderCommandResult::Skip;
                };
                pass.set_index_buffer(index_slice.buffer.slice(..), *index_format);
                pass.draw_indexed(
                    index_slice.range.start..(index_slice.range.start + count),
                    vertex_slice.range.start as i32,
                    0..batch.len,
                );
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vertex_slice.range, 0..batch.len);
            }
        }
        RenderCommandResult::Success
    }
}
