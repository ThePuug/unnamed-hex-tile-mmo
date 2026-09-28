//! Ink: dark lines where a surface stands in front of what is behind it,
//! drawn over the graded image from the depth prepass. Every opaque draw
//! writes that depth, so the terrain, the wood and the actors are lined by
//! one rule and none of them carries a line of its own. A camera takes the
//! lines by carrying [`Ink`] and a `DepthPrepass`; the lines' look — width,
//! threshold, fade and colour — is the shader's, `shaders/ink.wgsl`.

use bevy::{
    core_pipeline::{
        fullscreen_material::fullscreen_material_system, prepass::ViewPrepassTextures,
        tonemapping::tonemapping, Core3d, Core3dSystems, FullscreenShader,
    },
    prelude::*,
    render::{
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_resource::{
            binding_types::{
                sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled,
                uniform_buffer,
            },
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState, Operations,
            PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
            ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines,
            TextureFormat, TextureSampleType,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        sync_component::SyncComponent,
        view::{ExtractedView, Msaa, ViewTarget},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

use crate::plugins::vignette::VignetteSettings;

pub struct InkPlugin;

impl Plugin for InkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<Ink>::default(),
            UniformComponentPlugin::<InkUniform>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<SpecializedRenderPipelines<InkPipeline>>()
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(Render, prepare_pipelines.in_set(RenderSystems::Prepare))
            // After tone mapping, so the lines are the graded image's darkest
            // and the curve never lifts them; before the vignette, so its
            // edge darkens the lines with everything else.
            .add_systems(
                Core3d,
                ink.in_set(Core3dSystems::PostProcess)
                    .after(tonemapping)
                    .before(fullscreen_material_system::<VignetteSettings>),
            );
    }
}

/// Lines this camera's image. The camera must carry a `DepthPrepass`.
#[derive(Component, Clone, Copy, Default)]
pub struct Ink;

/// What the shader needs of the camera: its near plane, which turns the
/// reverse-z depth into a distance.
#[derive(Component, Clone, Copy, ShaderType)]
pub struct InkUniform {
    near: f32,
}

impl SyncComponent for Ink {
    type Target = (InkUniform, InkPipelineId);
}

impl ExtractComponent for Ink {
    type QueryData = &'static Projection;
    type QueryFilter = With<Ink>;
    type Out = InkUniform;

    fn extract_component(projection: &Projection) -> Option<InkUniform> {
        match projection {
            Projection::Perspective(p) => Some(InkUniform { near: p.near }),
            _ => None,
        }
    }
}

#[derive(Resource)]
struct InkPipeline {
    single: BindGroupLayoutDescriptor,
    multisampled: BindGroupLayoutDescriptor,
    sampler: Sampler,
    shader: Handle<Shader>,
    fullscreen: FullscreenShader,
}

impl InkPipeline {
    fn layout(&self, multisampled: bool) -> &BindGroupLayoutDescriptor {
        if multisampled { &self.multisampled } else { &self.single }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct InkPipelineKey {
    target_format: TextureFormat,
    multisampled: bool,
}

impl SpecializedRenderPipeline for InkPipeline {
    type Key = InkPipelineKey;

    fn specialize(&self, key: InkPipelineKey) -> RenderPipelineDescriptor {
        let mut shader_defs = Vec::new();
        if key.multisampled {
            shader_defs.push("MULTISAMPLED".into());
        }
        RenderPipelineDescriptor {
            label: Some("ink pipeline".into()),
            layout: vec![self.layout(key.multisampled).clone()],
            vertex: self.fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                targets: vec![Some(ColorTargetState {
                    format: key.target_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        }
    }
}

#[derive(Component)]
pub struct InkPipelineId {
    id: CachedRenderPipelineId,
    multisampled: bool,
}

fn init_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
) {
    let layout = |label: &'static str, multisampled: bool| {
        BindGroupLayoutDescriptor::new(
            label,
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    if multisampled { texture_depth_2d_multisampled() } else { texture_depth_2d() },
                    uniform_buffer::<InkUniform>(true),
                ),
            ),
        )
    };
    commands.insert_resource(InkPipeline {
        single: layout("ink bind group layout", false),
        multisampled: layout("ink bind group layout (multisampled)", true),
        sampler: render_device.create_sampler(&SamplerDescriptor::default()),
        shader: asset_server.load("shaders/ink.wgsl"),
        fullscreen: fullscreen.clone(),
    });
}

fn prepare_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    ink_pipeline: Res<InkPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<InkPipeline>>,
    views: Query<(Entity, &ExtractedView, &Msaa), (With<ExtractedCamera>, With<InkUniform>)>,
) {
    for (entity, view, msaa) in &views {
        let multisampled = *msaa != Msaa::Off;
        let key = InkPipelineKey { target_format: view.target_format, multisampled };
        let id = pipelines.specialize(&pipeline_cache, &ink_pipeline, key);
        commands.entity(entity).insert(InkPipelineId { id, multisampled });
    }
}

fn ink(
    view: ViewQuery<(
        &ViewTarget,
        &ViewPrepassTextures,
        &Msaa,
        &DynamicUniformIndex<InkUniform>,
        &InkPipelineId,
    )>,
    pipeline_cache: Res<PipelineCache>,
    ink_pipeline: Res<InkPipeline>,
    uniforms: Res<ComponentUniforms<InkUniform>>,
    mut ctx: RenderContext,
) {
    let (target, prepass, msaa, uniform_index, pipeline_id) = view.into_inner();
    // A frame whose sample count changed after the pipeline was chosen
    // would bind a depth texture of the other kind: skip it.
    if pipeline_id.multisampled != (*msaa != Msaa::Off) {
        return;
    }
    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.id) else {
        return;
    };
    let Some(depth) = prepass.depth_view() else {
        return;
    };
    let Some(uniform) = uniforms.uniforms().binding() else {
        return;
    };

    let post_process = target.post_process_write();
    let bind_group = ctx.render_device().create_bind_group(
        "ink bind group",
        &pipeline_cache.get_bind_group_layout(ink_pipeline.layout(pipeline_id.multisampled)),
        &BindGroupEntries::sequential((post_process.source, &ink_pipeline.sampler, depth, uniform)),
    );
    let pass = RenderPassDescriptor {
        label: Some("ink"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };

    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let time_span = diagnostics.time_span(ctx.command_encoder(), "ink");
    {
        let mut render_pass = ctx.command_encoder().begin_render_pass(&pass);
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &bind_group, &[uniform_index.index()]);
        render_pass.draw(0..3, 0..1);
    }
    time_span.end(ctx.command_encoder());
}
