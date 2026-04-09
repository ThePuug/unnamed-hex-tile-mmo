/// Horizontal distance from camera to player pivot (world units).
pub const CAMERA_DISTANCE: f32 = 120.0;
/// Degrees the frustum top stays below the horizon at max FOV.
pub const HORIZON_MARGIN_DEG: f32 = 5.0;
/// Maximum vertical FOV for normal gameplay (60°).
pub const MAX_GAMEPLAY_FOV: f32 = 60_f32.to_radians();
/// World-space height per z-level.
pub const RISE: f32 = 0.8;

/// Camera height that keeps the horizon `HORIZON_MARGIN_DEG` below the
/// frustum top at the given max vertical FOV.
pub fn camera_height(max_fov: f32) -> f32 {
    let margin = HORIZON_MARGIN_DEG.to_radians();
    CAMERA_DISTANCE * (max_fov * 0.5 + margin).tan()
}
