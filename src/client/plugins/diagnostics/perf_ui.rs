use bevy::prelude::*;
use iyes_perf_ui::prelude::*;

use super::config::{DiagnosticsConfig, DiagnosticsState};

// ============================================================================
// Components
// ============================================================================

/// Marker component for the root performance UI entity
///
/// Used to identify the performance overlay for visibility toggling.
#[derive(Component)]
pub struct PerfUiRoot;

// ============================================================================
// Systems
// ============================================================================

/// Creates the performance UI overlay on startup
///
/// The UI displays default metrics (FPS, frame time, entity count, etc.)
/// and respects the initial visibility setting from DiagnosticsState.
pub fn setup_performance_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
) {
    commands.spawn((
        PerfUiRoot,
        PerfUiDefaultEntries::default(),
        if state.perf_ui_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    ));
}

/// Toggles performance UI visibility when the perf UI toggle key is pressed
///
/// Updates both the DiagnosticsState resource and the visibility component
/// of the performance UI entity.
pub fn toggle_performance_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
    mut query: Query<&mut Visibility, With<PerfUiRoot>>,
) {
    if keyboard.just_pressed(config.perf_ui_toggle_key) {
        state.perf_ui_visible = !state.perf_ui_visible;

        if let Ok(mut visibility) = query.single_mut() {
            *visibility = if state.perf_ui_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        info!(
            "Performance UI {}",
            if state.perf_ui_visible { "enabled" } else { "disabled" }
        );
    }
}
