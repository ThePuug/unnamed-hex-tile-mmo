use bevy::prelude::*;

/// Consolidated state for all diagnostic features
///
/// All features are toggled via the developer console (NumpadDivide to open).
/// Direct keybindings have been removed to eliminate input conflicts.
#[derive(Resource)]
pub struct DiagnosticsState {
    /// Debug grid visibility
    pub grid_visible: bool,
    /// Slope rendering enabled (required for proper physics)
    pub slope_rendering_enabled: bool,
    /// Fixed lighting at 9 AM (false = dynamic day/night cycle)
    pub fixed_lighting_enabled: bool,
    /// Performance UI visibility
    pub perf_ui_visible: bool,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            slope_rendering_enabled: true, // Required for proper physics
            fixed_lighting_enabled: true,  // Start in debug mode
            perf_ui_visible: true,         // Start visible
        }
    }
}
