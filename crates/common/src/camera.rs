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

/// Maximum assumed screen aspect ratio (width/height) for horizon coverage.
/// Slightly generous over 16:9 to cover wide windowed clients.
pub const ASSUMED_MAX_ASPECT: f32 = 2.0;

/// Horizontal ground distance the camera frustum can see — measured for the
/// TOP-CORNER rays, which see farthest.
///
/// The top-center ray dips `HORIZON_MARGIN_DEG` below horizontal by
/// construction (`camera_height`), striking ground at h/tan(margin). A
/// top-corner ray is additionally rotated sideways by the half-horizontal
/// FOV φ, which flattens its depression to atan(tan(margin)·cos(φ)) — it
/// strikes ground 1/cos(φ) farther. Terrain coverage computed from the
/// top-center distance alone leaves dark wedges in the upper screen corners.
pub fn far_ground_wu(camera_total_height: f32, fov: f32) -> f32 {
    let margin = HORIZON_MARGIN_DEG.to_radians();
    let half_hfov = (ASSUMED_MAX_ASPECT * (fov * 0.5).tan()).atan();
    camera_total_height / margin.tan() / half_hfov.cos()
}
