use bevy::prelude::*;

use crate::{
    client::components::Terrain,
    common::resources::map::Map,
};
use super::config::DiagnosticsState;

// ============================================================================
// Feature Toggle Systems
// ============================================================================

/// Toggles slope rendering mode and triggers mesh regeneration
///
/// NOTE: This function is deprecated and only used by legacy tests.
/// Slope rendering toggle is now handled by the developer console.
///
/// When slope rendering is disabled, all tiles are rendered flat at their base elevation.
/// When enabled, vertices are adjusted to create smooth transitions between height differences.
///
/// This affects:
/// - Terrain mesh rendering
/// - Debug grid visualization
/// - Target cursor positioning
///
/// Physics requires slope rendering to be enabled for correct collision detection.
pub fn toggle_slope_rendering(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DiagnosticsState>,
    mut terrain_query: Query<&mut Terrain>,
    mut map: ResMut<Map>,
) {
    // Hardcoded key for legacy test compatibility
    if keyboard.just_pressed(KeyCode::KeyH) {
        state.slope_rendering_enabled = !state.slope_rendering_enabled;

        info!(
            "Slope rendering {}",
            if state.slope_rendering_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );

        // Trigger terrain mesh regeneration with new slope setting
        if let Ok(mut terrain) = terrain_query.single_mut() {
            terrain.task_start_regenerate_mesh = true;
        }

        // Trigger map change detection to force grid regeneration
        map.set_changed();
    }
}

/// Toggles between fixed lighting (9 AM) and dynamic day/night cycle
///
/// NOTE: This function is deprecated and only used by legacy tests.
/// Fixed lighting toggle is now handled by the developer console.
///
/// Fixed lighting is useful for debugging and ensures consistent visibility.
/// Dynamic lighting provides realistic time-of-day variation.
///
/// This state is read by the lighting system in world.rs to determine
/// whether to advance the time of day or keep it locked.
pub fn toggle_fixed_lighting(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DiagnosticsState>,
) {
    // Hardcoded key for legacy test compatibility
    if keyboard.just_pressed(KeyCode::KeyG) {
        state.fixed_lighting_enabled = !state.fixed_lighting_enabled;

        info!(
            "Fixed lighting {}",
            if state.fixed_lighting_enabled {
                "enabled (9 AM)"
            } else {
                "disabled (dynamic)"
            }
        );
    }
}
