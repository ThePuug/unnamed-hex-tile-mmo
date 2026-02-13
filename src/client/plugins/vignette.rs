//! Vignette post-processing plugin using Bevy 0.18 FullscreenMaterial API

use bevy::{
    core_pipeline::{
        core_3d::graph::{Core3d, Node3d},
        fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin},
    },
    prelude::*,
    render::{
        extract_component::ExtractComponent,
        render_graph::{InternedRenderLabel, InternedRenderSubGraph, RenderLabel, RenderSubGraph},
        render_resource::ShaderType,
    },
    shader::ShaderRef,
};

use crate::common::components::{behaviour::PlayerControlled, resources::CombatState};

/// Plugin that adds vignette post-processing effect
pub struct VignettePlugin;

impl Plugin for VignettePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FullscreenMaterialPlugin::<VignetteSettings>::default());
        app.add_systems(Update, update_vignette_intensity);
    }
}

/// Settings for vignette effect (attached to camera)
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct VignetteSettings {
    /// Intensity of the vignette effect (0.0 = none, 1.0 = full)
    pub intensity: f32,
    /// Time for pulsing effect
    pub time: f32,
}

impl FullscreenMaterial for VignetteSettings {
    fn fragment_shader() -> ShaderRef {
        "shaders/vignette.wgsl".into()
    }

    fn node_edges() -> Vec<InternedRenderLabel> {
        vec![
            Node3d::Tonemapping.intern(),
            Self::node_label().intern(),
            Node3d::EndMainPassPostProcessing.intern(),
        ]
    }

    fn sub_graph() -> Option<InternedRenderSubGraph> {
        Some(Core3d.intern())
    }
}

/// Update vignette intensity based on player combat state
fn update_vignette_intensity(
    mut vignette_query: Query<&mut VignetteSettings>,
    player_query: Query<&CombatState, With<PlayerControlled>>,
    time: Res<Time>,
) {
    let Ok(combat_state) = player_query.single() else {
        return;
    };

    let target_intensity = if combat_state.in_combat {
        1.0 // Full vignette when in combat
    } else {
        0.0 // No vignette when out of combat
    };

    const FADE_SPEED: f32 = 3.0;

    for mut settings in &mut vignette_query {
        let current = settings.intensity;
        let new_intensity = current + (target_intensity - current) * (FADE_SPEED * time.delta_secs()).min(1.0);
        settings.intensity = new_intensity;
        settings.time = time.elapsed_secs();
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
