use bevy::prelude::*;

/// Configuration for diagnostics features
#[derive(Resource)]
pub struct DiagnosticsConfig {
    /// Key to toggle debug grid (default: J)
    pub grid_toggle_key: KeyCode,
    /// Key to toggle slope rendering (default: H)
    pub slope_toggle_key: KeyCode,
    /// Key to toggle fixed lighting (default: G)
    pub lighting_toggle_key: KeyCode,
    /// Key to toggle performance UI (default: F3)
    pub perf_ui_toggle_key: KeyCode,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            grid_toggle_key: KeyCode::KeyJ,
            slope_toggle_key: KeyCode::KeyH,
            lighting_toggle_key: KeyCode::KeyG,
            perf_ui_toggle_key: KeyCode::F3,
        }
    }
}

/// Consolidated state for all diagnostic features
#[derive(Resource)]
pub struct DiagnosticsState {
    /// Debug grid visibility (toggled with 'J' key)
    pub grid_visible: bool,
    /// Slope rendering enabled (toggled with 'H' key, required for proper physics)
    pub slope_rendering_enabled: bool,
    /// Fixed lighting at 9 AM (toggled with 'G' key, false = dynamic day/night cycle)
    pub fixed_lighting_enabled: bool,
    /// Performance UI visibility (toggled with 'F3' key)
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
