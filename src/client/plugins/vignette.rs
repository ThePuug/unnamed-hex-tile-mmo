//! Vignette post-processing plugin
//!
//! TODO: The render graph API changed significantly in Bevy 0.17.
//! The vignette effect is temporarily disabled until the post-processing
//! can be properly updated to use the new API.

use bevy::prelude::*;

use crate::common::components::{behaviour::PlayerControlled, resources::CombatState};

/// Plugin that adds vignette post-processing effect
/// NOTE: Currently a no-op stub - render graph API needs updating for Bevy 0.17
pub struct VignettePlugin;

impl Plugin for VignettePlugin {
    fn build(&self, app: &mut App) {
        // Just add the system to update vignette intensity
        // The actual rendering is disabled until API is updated
        app.add_systems(Update, update_vignette_intensity);
    }
}

/// Settings for vignette effect (attached to camera)
/// NOTE: Currently not rendered - component kept for API compatibility
#[derive(Component, Clone, Copy, Default)]
pub struct VignetteSettings {
    /// Intensity of the vignette effect (0.0 = none, 1.0 = full)
    pub intensity: f32,
    /// Time for pulsing effect
    pub time: f32,
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
