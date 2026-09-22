/// Horizontal distance from camera to player pivot (world units): the
/// boom at full length.
pub const CAMERA_DISTANCE: f32 = 40.0;
/// Degrees the frame top stays below the horizontal at max FOV; with
/// `CAMERA_DISTANCE` this fixes the boom's pitch (`camera_height`).
pub const HORIZON_MARGIN_DEG: f32 = 5.0;
/// Maximum vertical FOV for normal gameplay (60°).
pub const MAX_GAMEPLAY_FOV: f32 = 60_f32.to_radians();
/// World-space height per z-level.
pub const RISE: f32 = 0.8;

/// World-space size of one hexagon — centre to vertex. Neighbouring tile
/// centres sit `HEX_RADIUS * sqrt(3)` apart, so together with [`RISE`] this
/// fixes the ratio between a z-level and a tile step, and with it the angle
/// any given gradient reads as on screen.
pub const HEX_RADIUS: f32 = 1.0;

/// Half-angle either side of the heading within which the server streams
/// far summaries: the widest the frame reaches to either side, plus the
/// allowance a turn gets before the far view waits on the wire. The
/// client's widest lens must fit inside it.
pub const STREAM_SECTOR_HALF_ANGLE: f32 = 65_f32.to_radians();

/// Camera height that keeps the frame top `HORIZON_MARGIN_DEG` below the
/// horizontal at the given max vertical FOV.
pub fn camera_height(max_fov: f32) -> f32 {
    let margin = HORIZON_MARGIN_DEG.to_radians();
    CAMERA_DISTANCE * (max_fov * 0.5 + margin).tan()
}
