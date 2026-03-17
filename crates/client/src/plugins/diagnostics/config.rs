use bevy::prelude::*;

#[derive(Resource)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    pub fixed_lighting_enabled: bool,
    pub metrics_overlay_visible: bool,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            fixed_lighting_enabled: true,
            metrics_overlay_visible: false,
        }
    }
}
