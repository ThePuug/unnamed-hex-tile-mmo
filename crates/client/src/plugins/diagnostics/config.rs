use bevy::prelude::*;

#[derive(Resource)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    pub fixed_lighting_enabled: bool,
    pub metrics_overlay_visible: bool,
    /// Every camera renders without MSAA.
    pub msaa_off: bool,
    /// The sun's shadows are filtered by the hardware's 2×2 tap instead of
    /// the Gaussian.
    pub hard_shadows: bool,
    /// The camera goes straight to its wanted pose, showing whatever the
    /// envelope would have hidden.
    pub camera_envelope_off: bool,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            fixed_lighting_enabled: true,
            metrics_overlay_visible: false,
            msaa_off: false,
            hard_shadows: false,
            camera_envelope_off: false,
        }
    }
}
