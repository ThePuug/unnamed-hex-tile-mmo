use bevy::{
    core_pipeline::{
        core_3d::graph::{Core3d, Node3d},
        fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    },
    ecs::query::QueryItem,
    prelude::*,
    render::{
        extract_component::{ComponentUniforms, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin},
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
        RenderApp,
    },
};

use crate::common::components::{behaviour::PlayerControlled, resources::CombatState};

/// Plugin that adds vignette post-processing effect
pub struct VignettePlugin;

impl Plugin for VignettePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<VignetteSettings>::default(),
            UniformComponentPlugin::<VignetteSettings>::default(),
        ));

        app.add_systems(Update, update_vignette_intensity);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_render_graph_node::<ViewNodeRunner<VignetteNode>>(Core3d, VignetteLabel)
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::Tonemapping,
                    VignetteLabel,
                    Node3d::EndMainPassPostProcessing,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<VignettePipeline>();
    }
}

/// Settings for vignette effect (attached to camera)
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType)]
pub struct VignetteSettings {
    /// Intensity of the vignette effect (0.0 = none, 1.0 = full)
    pub intensity: f32,
    /// Time for pulsing effect
    pub time: f32,

    #[cfg(target_arch = "wasm32")]
    pub _webgl2_padding: Vec2,
}

impl Default for VignetteSettings {
    fn default() -> Self {
        Self {
            intensity: 0.0,
            time: 0.0,
            #[cfg(target_arch = "wasm32")]
            _webgl2_padding: Vec2::ZERO,
        }
    }
}

/// Update vignette intensity based on player combat state
fn update_vignette_intensity(
    mut vignette_query: Query<&mut VignetteSettings>,
    player_query: Query<&CombatState, With<PlayerControlled>>,
    time: Res<Time>,
) {
    let Ok(combat_state) = player_query.get_single() else {
        return;
    };

    let target_intensity = if combat_state.in_combat {
        1.0 // Full vignette when in combat
    } else {
        0.0 // No vignette when out of combat
    };

    const FADE_SPEED: f32 = 3.0; // Match previous fade speed

    for mut settings in &mut vignette_query {
        // Smooth lerp to target intensity
        let current = settings.intensity;
        let new_intensity = current + (target_intensity - current) * (FADE_SPEED * time.delta_secs()).min(1.0);
        settings.intensity = new_intensity;

        // Update time for pulsing effect
        settings.time = time.elapsed_secs();
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct VignetteLabel;

#[derive(Default)]
struct VignetteNode;

impl ViewNode for VignetteNode {
    type ViewQuery = &'static ViewTarget;

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        view_target: QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<VignettePipeline>();

        let Some(pipeline_id) = pipeline_cache.get_render_pipeline(pipeline.pipeline_id) else {
            return Ok(());
        };

        let settings_uniforms = world.resource::<ComponentUniforms<VignetteSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        let post_process = view_target.post_process_write();

        let bind_group = render_context.render_device().create_bind_group(
            "vignette_bind_group",
            &pipeline.layout,
            &BindGroupEntries::sequential((
                post_process.source,
                &pipeline.sampler,
                settings_binding.clone(),
            )),
        );

        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("vignette_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_render_pipeline(pipeline_id);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct VignettePipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for VignettePipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        let layout = render_device.create_bind_group_layout(
            "vignette_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    uniform_buffer::<VignetteSettings>(false),
                ),
            ),
        );

        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        let shader = world.load_asset("shaders/vignette.wgsl");

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("vignette_pipeline".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    entry_point: "fragment".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::bevy_default(),
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: false,
            });

        Self {
            layout,
            sampler,
            pipeline_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vignette_settings_default() {
        let settings = VignetteSettings::default();
        assert_eq!(settings.intensity, 0.0, "Default vignette intensity should be 0.0 (disabled)");
    }
}
