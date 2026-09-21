//! The gameplay camera: a pose behind the player that follows the heading,
//! opens toward the ceiling over open ground, and never frames ground that
//! is not drawn. No key moves it. Flyover drives the same camera entity by
//! hand through `CameraOrbit` and its own update.

use bevy::{core_pipeline::prepass::DepthPrepass, pbr::{DistanceFog, FogFalloff}, prelude::*};
use crate::systems::closeup::CloseupCamera;
use qrz::{Convert, Qrz};
use std::f32::consts::PI;

use crate::plugins::{diagnostics::DiagnosticsState, vignette::VignetteSettings};
use crate::resources::{EdgeCenters, SummaryMeshes};
use crate::systems::world::DrawnGround;
use common_bevy::{
    components::{heading::{Heading, HEADING_SLOTS}, *},
    resources::map::Map,
    systems::movement::{standing_y, surface_y},
};

/// Orbit stops: one per heading, so the camera can stand behind any.
pub const ORBIT_STOPS: usize = HEADING_SLOTS as usize;
/// Angular separation between orbit stops.
const ORBIT_STEP: f32 = 2.0 * PI / ORBIT_STOPS as f32;
/// Seconds between steps while a flyover turn key is held.
const ORBIT_REPEAT_SECS: f32 = 0.08;
/// Exponential decay constant of the yaw's easing: slow enough that the
/// heading's steps blend into one turn, quick enough to lag it by a stop
/// or two at most.
const YAW_EASE: f32 = 8.0;
/// Exponential decay constants of the boom and lens: toward a wanted pose
/// that is safe the frame moves briskly; toward one that is not it moves
/// slowly, waiting on ground to land, so regions arriving one by one do
/// not jog it; tightening to the envelope settles in a third of a second.
const TIGHTEN_EASE: f32 = 10.0;
const OPEN_EASE: f32 = 4.0;
const WAIT_EASE: f32 = 1.5;
/// Exponential decay constant of the pull's smoothing, so the ground
/// sampled ahead cannot flick the pose as the player walks.
const PULL_EASE: f32 = 2.0;
/// Exponential decay constants of the boom's clearance: shortening away
/// from an obstruction is quick, lengthening back out is slow.
const SHORTEN_EASE: f32 = 12.0;
const LENGTHEN_EASE: f32 = 2.0;
/// Threshold below which the yaw's easing snaps to its target.
const SNAP_THRESHOLD: f32 = 0.005;

// Re-export canonical camera constants from common.
pub use common::camera::{CAMERA_DISTANCE, MAX_GAMEPLAY_FOV, camera_height};

/// Vertical field of view at rest: a narrow telephoto.
const REST_FOV: f32 = 15_f32.to_radians();
/// The ceiling's lens and the elevation of its boom: low and moderate, so
/// that with the player in the lower third the frame looks out across the
/// ground to the haze, with the horizon in its upper part and sky above.
const CEILING_FOV: f32 = 35_f32.to_radians();
const CEILING_ELEVATION: f32 = 12_f32.to_radians();
/// The floor's lens and elevation: the narrowest, from the highest.
const FLOOR_FOV: f32 = 6_f32.to_radians();
const FLOOR_ELEVATION: f32 = 55_f32.to_radians();
/// How far below the frame's centre the player stands, as a fraction of
/// the half-height: the look point is that much ahead on the ground.
const PLAYER_DROP: f32 = 1.0 / 3.0;
/// The widest the lens opens to hold rising ground in the frame.
const HILL_FOV: f32 = 60_f32.to_radians();
/// The plane the camera sweeps on is the ground's under the player: the
/// steepest it is allowed to tilt, short of the angle of repose so a brink
/// does not flip it; the decay constant of its smoothing; and the least
/// the camera stands above the player's feet whatever the plane does
/// behind them.
const TILT_MAX: f32 = 30_f32.to_radians();
const TILT_EASE: f32 = 3.0;
const MIN_RISE: f32 = 5_f32.to_radians();
/// Ground distances around the player at which the plane is fitted, in
/// world units, in each of six directions.
const TILT_RINGS_WU: [f32; 2] = [20.0, 45.0];
/// Maximum FOV for flyover mode (admin).
pub const MAX_FLYOVER_FOV: f32 = 90_f32.to_radians();

/// Camera height for normal gameplay (convenience alias).
pub fn gameplay_camera_height() -> f32 {
    camera_height(MAX_GAMEPLAY_FOV)
}

/// The haze at noon: one colour that distance fades everything toward, and
/// the sky above the horizon, so the frontier at the reach never shows.
/// `world::update` lights both by the sun and moon through the day, the
/// sky in the sun's tint and the haze in a share of it.
pub const HAZE_COLOR: Color = Color::linear_rgb(0.72, 0.78, 0.85);
/// Where the haze completes, as a fraction of the reach: inside it, so the
/// frontier stands behind full haze.
const HAZE_END_FRAC: f32 = 0.92;

/// Ground distance at which the haze is complete: the footprint ends here.
pub fn haze_limit_wu() -> f32 {
    common_bevy::summary::reach_wu() * HAZE_END_FRAC
}

/// Distance fog to the haze, for the camera: deepening as the square of
/// the distance, so it is flat at the camera and has no onset to see,
/// slight over the near ground and complete at the haze limit.
fn haze() -> DistanceFog {
    DistanceFog {
        color: HAZE_COLOR,
        falloff: FogFalloff::from_visibility_squared(haze_limit_wu()),
        ..default()
    }
}

/// A camera pose. The camera stands `boom` behind the player along the
/// yaw, at the height of the ground's plane under the player there plus
/// the boom's `elevation`, and looks along the yaw through a lens of `fov`
/// with the player a third of the way down the frame. The sweep is an
/// ellipse on the plane: the boom's reach on the ground holds and the
/// height follows the slope. The frame's top ray dips `pitch - fov / 2`
/// below the horizontal, which is what decides how far the footprint
/// reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Radians counter-clockwise from behind a player facing north.
    pub yaw: f32,
    pub elevation: f32,
    pub fov: f32,
    pub boom: f32,
}

/// The ground's plane under the player as a gradient: its rise per unit
/// of run in world x and z. Zero is level.
pub type Tilt = Vec2;

impl Pose {
    /// The widest pose: the frame's top ray above the horizontal.
    fn ceiling(yaw: f32) -> Self {
        Pose { yaw, elevation: CEILING_ELEVATION, fov: CEILING_FOV, boom: CAMERA_DISTANCE }
    }

    /// The pose nothing pulls on: from the height the ladder is measured
    /// by, the lens narrow.
    fn rest(yaw: f32) -> Self {
        Pose { yaw, elevation: (gameplay_camera_height() / CAMERA_DISTANCE).atan(), fov: REST_FOV, boom: CAMERA_DISTANCE }
    }

    /// The tightest pose.
    fn floor(yaw: f32) -> Self {
        Pose { yaw, elevation: FLOOR_ELEVATION, fov: FLOOR_FOV, boom: CAMERA_DISTANCE }
    }

    /// This pose `t` of the way to `to`, the yaw kept.
    fn toward(self, to: Pose, t: f32) -> Self {
        Pose {
            elevation: self.elevation.lerp(to.elevation, t),
            fov: self.fov.lerp(to.fov, t),
            boom: self.boom.lerp(to.boom, t),
            ..self
        }
    }

    /// The direction from the player to the camera on the ground.
    fn back(&self) -> Vec2 {
        let (sin, cos) = self.yaw.sin_cos();
        Vec2::new(sin, cos)
    }

    /// Where the camera stands relative to the player's feet on ground of
    /// `tilt`: the boom's length back on the ground, and up by the plane's
    /// rise there plus the elevation, never less than the least rise. So
    /// behind a player facing up a slope the camera stands low and near,
    /// facing down it high and far.
    fn offset(&self, tilt: Tilt) -> Vec3 {
        let back = self.back();
        let rise = (self.elevation.tan() + tilt.dot(back)).max(MIN_RISE.tan());
        Vec3::new(back.x * self.boom, self.boom * rise, back.y * self.boom)
    }

    /// The camera's depression down to the player: negative looking up.
    fn down(&self, tilt: Tilt) -> f32 {
        let offset = self.offset(tilt);
        offset.y.atan2(offset.xz().length())
    }

    /// The frame's tilt below the horizontal: the player a third of the
    /// way from the frame's centre to its bottom edge.
    fn pitch(&self, tilt: Tilt) -> f32 {
        self.down(tilt) - PLAYER_DROP * self.fov / 2.0
    }

    /// This pose tightened `t` of the way to the floor.
    fn tightened(self, t: f32) -> Self {
        self.toward(Pose::floor(self.yaw), t)
    }

    /// Where the camera stands and looks, for a player at `player` on
    /// ground of `tilt`.
    fn transform(&self, player: Vec3, tilt: Tilt) -> Transform {
        let back = self.back();
        let pitch = self.pitch(tilt);
        let forward = Vec3::new(-back.x * pitch.cos(), -pitch.sin(), -back.y * pitch.cos());
        Transform::from_translation(player + self.offset(tilt)).looking_to(forward, Vec3::Y)
    }
}

/// The camera's state between frames: the pose it is at, easing toward a
/// safe one; the pose the envelope is tightening it onto, while it is; the
/// smoothed pull of the ground ahead; and the fraction of the boom it
/// stands at.
#[derive(Resource)]
pub struct CameraPose {
    pub pose: Pose,
    limit: Option<Pose>,
    openness: f32,
    /// The ground's plane under the player, smoothed: what the camera
    /// sweeps on.
    tilt: Tilt,
    clearance: f32,
}

/// `from` eased toward `to` by the decay constant `k` over `dt`.
fn ease(from: f32, to: f32, k: f32, dt: f32) -> f32 {
    from + (to - from) * (1.0 - (-k * dt).exp())
}

/// Camera orbit state: discrete stops, one per heading, and smooth
/// interpolation. In gameplay the target stop follows the player's heading
/// and `current` is the pose's yaw; flyover steps it by key.
#[derive(Resource)]
pub struct CameraOrbit {
    /// Current interpolated angle (radians, 0 = behind player facing north)
    pub current: f32,
    /// Target stop index, counter-clockwise from behind the player
    pub target_index: usize,
    /// Whether a turn key is held, and the seconds until it steps again
    held: bool,
    repeat: f32,
}

impl Default for CameraOrbit {
    fn default() -> Self {
        Self { current: 0.0, target_index: 0, held: false, repeat: 0.0 }
    }
}

impl CameraOrbit {
    pub fn target_angle(&self) -> f32 {
        self.target_index as f32 * ORBIT_STEP
    }

    /// The heading the camera faces: the orbit angle runs counter-clockwise
    /// and a bearing clockwise, so the stop index counts down from north.
    pub fn forward(&self) -> Heading {
        Heading::from_slot(((ORBIT_STOPS - self.target_index) % ORBIT_STOPS) as u8)
    }

    /// Stand behind `heading`.
    pub fn follow(&mut self, heading: Heading) {
        self.target_index = (ORBIT_STOPS - heading.slot() as usize) % ORBIT_STOPS;
    }

    /// One step on the first call while held, then one every ORBIT_REPEAT_SECS.
    fn step(&mut self, delta: isize, dt: f32) {
        if self.held {
            self.repeat -= dt;
            if self.repeat > 0.0 { return; }
        }
        self.held = true;
        self.repeat = ORBIT_REPEAT_SECS;
        self.target_index = (self.target_index as isize + delta).rem_euclid(ORBIT_STOPS as isize) as usize;
    }

    pub fn step_cw(&mut self, dt: f32) {
        self.step(-1, dt);
    }

    pub fn step_ccw(&mut self, dt: f32) {
        self.step(1, dt);
    }

    /// The turn keys are up: the next press steps at once.
    pub fn release(&mut self) {
        self.held = false;
        self.repeat = 0.0;
    }
}

/// Shortest signed angle from `from` to `to` on the unit circle.
fn angle_diff(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(2.0 * PI);
    if d > PI { d - 2.0 * PI } else { d }
}

pub fn setup(
    mut commands: Commands,
) {
    commands.insert_resource(CameraOrbit::default());
    commands.insert_resource(CameraPose { pose: Pose::floor(0.0), limit: None, openness: 0.0, tilt: Vec2::ZERO, clearance: 1.0 });
    commands.insert_resource(ClearColor(HAZE_COLOR));

    commands.spawn((
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: REST_FOV,
            near: 1.0,
            // Culling only: the projection is infinite reverse-z. Past the
            // reach, so the coarsest band is drawn to its edge.
            far: common_bevy::summary::reach_wu() * 1.5,
            ..default()
        }),
        Transform::default(),
        Actor,
        VignetteSettings::default(),
        haze(),
        // Depth first, so the terrain's fragment shader runs once per pixel
        // that shows and never for one another tile covers.
        DepthPrepass,
    ));
}

/// How far a ray is marched against the loaded tiles, in world units, and
/// in how many steps: spaced as squares, dense near and sparse far.
const MARCH_STEPS: u32 = 12;

/// Where a ray from `from` along the unit `dir` first meets the terrain
/// within `reach`: `Ok(Some(t))` at that distance along the ray,
/// `Ok(None)` when it flies free, `Err(())` when it crosses ground that
/// is not loaded before meeting any. Read against the tiles' standing
/// heights, so a slope reads at the resolution of its tiles.
fn march(from: Vec3, dir: Vec3, reach: f32, map: &Map) -> Result<Option<f32>, ()> {
    for i in 1..=MARCH_STEPS {
        let f = i as f32 / MARCH_STEPS as f32;
        let t = reach * f * f;
        let at = from + dir * t;
        let here: Qrz = map.convert(at);
        let Some((floor, _)) = map.get_by_qr(here.q, here.r) else { return Err(()) };
        if at.y <= standing_y(floor, map) {
            return Ok(Some(t));
        }
    }
    Ok(None)
}

/// Ground distance ahead the sensor rays reach, and the sweep of yaw
/// offsets either side of the heading and the fan of elevations they are
/// cast at, from the player's eye.
const AHEAD_REACH_WU: f32 = 300.0;
const SWEEP_RAYS: i32 = 2;
const SWEEP_STEP: f32 = 20_f32.to_radians();
const FAN_RAYS: u32 = 8;
const FAN_STEP: f32 = 5_f32.to_radians();
/// Elevation of the lowest ray of the fan: the one that flying free to
/// the reach makes the ground open. A little above level, so a plain that
/// rises gently still counts as open.
const OPEN_ELEVATION: f32 = 3_f32.to_radians();
/// Height of the eye the sensor rays are cast from, over the feet.
const EYE_HEIGHT: f32 = 1.5;

/// The ground ahead, read by rays from the eye across a sweep about the
/// heading: how open it is, and the steepest sightline up it inside the
/// frame.
struct Ahead {
    /// 0 (closed) to 1 (open): the share of the sweep's lowest rays that
    /// fly free to the reach. What pulls the pose toward the ceiling.
    open: f32,
    /// The highest elevation, in radians above the horizontal, at which a
    /// ray inside the frame's width still meets the ground: None where
    /// none does.
    highest: Option<f32>,
}

/// The ground's plane under a player standing at `player`: the gradient
/// fitted to the tiles' standing heights on rings around the feet, capped
/// at the steepest tilt. None where a ring is not loaded. Six directions
/// on each ring make the fit's normal equations diagonal.
fn ground_tilt(player: Vec3, map: &Map) -> Option<Tilt> {
    let (mut xh, mut zh, mut xx) = (0.0_f32, 0.0_f32, 0.0_f32);
    for radius in TILT_RINGS_WU {
        for k in 0..6 {
            let angle = k as f32 * std::f32::consts::TAU / 6.0;
            let p = Vec2::from_angle(angle) * radius;
            let at = player + Vec3::new(p.x, 0.0, p.y);
            let here: Qrz = map.convert(at);
            let (floor, _) = map.get_by_qr(here.q, here.r)?;
            let h = standing_y(floor, map) - player.y;
            xh += p.x * h;
            zh += p.y * h;
            xx += p.x * p.x;
        }
    }
    let gradient = Vec2::new(xh / xx, zh / xx);
    Some(gradient.clamp_length_max(TILT_MAX.tan()))
}

/// The ground ahead, or None where too little of it is loaded to read.
/// `half_width` is the frame's horizontal half-angle: only rays inside it
/// feed the sightline the lens is opened to clear.
fn ground_ahead(player: Vec3, heading: Heading, half_width: f32, map: &Map) -> Option<Ahead> {
    let eye = player + Vec3::Y * EYE_HEIGHT;
    let ahead = heading.to_world_dir();
    let mut rays = 0.0_f32;
    let mut free = 0.0_f32;
    let mut highest: Option<f32> = None;
    for k in -SWEEP_RAYS..=SWEEP_RAYS {
        let yaw = k as f32 * SWEEP_STEP;
        let flat = Vec2::from_angle(yaw).rotate(ahead);
        rays += 1.0;
        // The grade along this ray: the steepest elevation at which a ray
        // still meets the ground within the reach; the lowest ray flying
        // free is open ground.
        let mut grade = None;
        for j in 0..FAN_RAYS {
            let up = OPEN_ELEVATION + j as f32 * FAN_STEP;
            let dir = Vec3::new(flat.x * up.cos(), up.sin(), flat.y * up.cos());
            match march(eye, dir, AHEAD_REACH_WU, map) {
                Ok(Some(_)) => grade = Some(up),
                Ok(None) => break,
                Err(()) => return None,
            }
        }
        if grade.is_none() { free += 1.0; }
        if yaw.abs() <= half_width {
            if let Some(up) = grade {
                highest = Some(highest.map_or(up, |h: f32| h.max(up)));
            }
        }
    }
    Some(Ahead { open: free / rays, highest })
}

/// The lens `pose` needs to keep the ground ahead in the frame with the
/// player in the lower third: the frame's top ray must clear the steepest
/// sightline up the ground, `highest` above the horizontal, and the pitch
/// the framing fixes puts the top `(1 + drop) · fov / 2` above the
/// camera's sightline down to the player.
fn lens_to_hold(pose: &Pose, tilt: Tilt, highest: Option<f32>) -> f32 {
    let Some(up) = highest else { return pose.fov };
    let needed = 2.0 * (pose.down(tilt) + up) / (1.0 + PLAYER_DROP);
    needed.clamp(pose.fov, HILL_FOV)
}

/// Frame samples across and down the frame for the footprint.
const FOOTPRINT_COLUMNS: u32 = 9;
const FOOTPRINT_ROWS: u32 = 7;
/// Steps of the ladder toward the floor tried before the yaw holds, and
/// the halvings that then bring the first safe step back to the edge it
/// crossed, so the frame is not tightened a whole step past it.
const TIGHTEN_STEPS: u32 = 16;
const EDGE_HALVINGS: u32 = 8;
/// The margin, as a fraction of the ladder, the frame keeps inside the
/// envelope's edge: it opens only while the pose that much looser is safe
/// too, and tightens only once its own pose is not, to the edge less the
/// margin. Between the two it holds, so the edge's flicker as the frame's
/// samples cross regions moves nothing.
const HOLD_MARGIN: f32 = 1.0 / 32.0;

/// How far a frame ray is marched against the tiles before the ground is
/// taken as the plane at the player's feet: the tiles the client holds.
fn march_reach_wu() -> f32 {
    common_bevy::chunk::FIXED_STREAM_APOTHEM_WU
}

/// Depressions below the horizontal, in radians, at which the frame is
/// sampled again when it spans the horizon: from a camera a few tens of
/// units up, all the ground from a few kilometres out to the haze limit
/// lies within a degree of the horizon, a sliver the grid's rows miss.
const HORIZON_ROWS: [f32; 2] = [1_f32.to_radians(), 3_f32.to_radians()];

/// The footprint of `pose` for a player standing at `player`: the ground
/// its frustum covers out to the haze limit, sampled on a grid of the
/// frame and, where the frame spans the horizon, on rows just below it.
/// A ray is marched against the loaded tiles first, so a hill in front is
/// where the ray lands; past them the ground is taken as the plane at the
/// player's feet, and a ray that lands past the haze limit is read at the
/// limit. A ray that rises above the horizontal and meets no tile shows
/// the sky, which needs nothing drawn, and is no sample.
fn footprint(pose: &Pose, player: Vec3, tilt: Tilt, aspect: f32, map: &Map) -> Vec<Vec2> {
    let camera = pose.transform(player, tilt);
    let pitch = pose.pitch(tilt);
    let half_v = (pose.fov / 2.0).tan();
    let half_h = half_v * aspect;
    let limit = haze_limit_wu();
    let mut rows: Vec<f32> = (0..FOOTPRINT_ROWS).map(|row| row as f32 / (FOOTPRINT_ROWS - 1) as f32 * 2.0 - 1.0).collect();
    let (top, bottom) = (pitch - pose.fov / 2.0, pitch + pose.fov / 2.0);
    if top < 0.0 {
        // The row at which a ray lands on the haze limit, then the horizon rows.
        let height = camera.translation.y - player.y;
        let at_limit = height.atan2(limit);
        rows.extend(
            HORIZON_ROWS.iter().copied().chain(std::iter::once(at_limit))
                .filter(|&down| down > top && down < bottom)
                .map(|down| (pitch - down).tan() / half_v),
        );
    }
    let mut points = Vec::with_capacity(rows.len() * FOOTPRINT_COLUMNS as usize);
    for y in rows {
        for col in 0..FOOTPRINT_COLUMNS {
            let x = col as f32 / (FOOTPRINT_COLUMNS - 1) as f32 * 2.0 - 1.0;
            let dir = (camera.rotation * Vec3::new(x * half_h, y * half_v, -1.0)).normalize();
            let from = camera.translation;
            let flat = dir.xz();
            if flat.length_squared() < 1e-6 { continue; }
            if let Ok(Some(t)) = march(from, dir, march_reach_wu(), map) {
                points.push((from + dir * t).xz());
                continue;
            }
            if dir.y >= 0.0 {
                continue;
            }
            let t = (player.y - from.y) / dir.y;
            let ground = from.xz() + flat * t;
            let hit = (ground.distance(from.xz()) <= limit).then_some(ground);
            points.push(hit.unwrap_or(from.xz() + flat.normalize() * limit));
        }
    }
    points
}

/// Whether `pose` is safe: every point of its footprint is drawn.
fn footprint_is_drawn(pose: &Pose, player: Vec3, tilt: Tilt, aspect: f32, map: &Map, drawn: &DrawnGround) -> bool {
    footprint(pose, player, tilt, aspect, map).into_iter().all(|p| drawn.at(p))
}

/// The loosest pose on the ladder from `top` to the floor at its yaw that
/// passes `test`: `top` itself where it passes, else the edge it crossed
/// to within the halvings' resolution, found between the ladder's last
/// failing step and its first passing one. None where not even the floor
/// passes.
fn loosest(top: Pose, test: impl Fn(&Pose) -> bool) -> Option<Pose> {
    let at = |t: f32| top.tightened(t);
    let first = (0..=TIGHTEN_STEPS).find(|&i| test(&at(i as f32 / TIGHTEN_STEPS as f32)))?;
    if first == 0 {
        return Some(top);
    }
    let (mut open, mut held) = ((first - 1) as f32 / TIGHTEN_STEPS as f32, first as f32 / TIGHTEN_STEPS as f32);
    for _ in 0..EDGE_HALVINGS {
        let mid = (open + held) / 2.0;
        if test(&at(mid)) { held = mid } else { open = mid }
    }
    Some(at(held))
}

/// One frame's step of the pose over `dt`. `current` turns toward the
/// wanted yaw, and opens toward the wanted boom and lens while the step
/// and the margin beyond it are safe: briskly where the wanted pose itself
/// is safe, slowly where the frame is waiting on ground to land, so
/// regions arriving one by one do not jog it. Where the frame's own pose
/// is unsafe it tightens onto the loosest pose that holds its margin — the
/// turn with the frame tightened as far as it takes, then the yaw held
/// with the same, then the floor — and `limit` remembers that pose so the
/// tightening runs to it rather than stopping where the frame first
/// scrapes back inside the edge. Otherwise the frame holds its boom and
/// lens and takes the turn.
fn step(current: Pose, wanted: Pose, limit: &mut Option<Pose>, dt: f32, safe: impl Fn(&Pose) -> bool) -> Pose {
    let diff = angle_diff(current.yaw, wanted.yaw);
    let yaw = if diff.abs() > SNAP_THRESHOLD {
        (current.yaw + diff * (1.0 - (-YAW_EASE * dt).exp())).rem_euclid(2.0 * PI)
    } else {
        wanted.yaw
    };
    let held = |p: &Pose| safe(p) && safe(&p.tightened(-HOLD_MARGIN));
    let k = if safe(&wanted) { OPEN_EASE } else { WAIT_EASE };
    let opened = Pose { yaw, ..current.toward(wanted, 1.0 - (-k * dt).exp()) };
    if held(&opened) {
        *limit = None;
        return opened;
    }
    let turned = Pose { yaw, ..current };
    let tighten = 1.0 - (-TIGHTEN_EASE * dt).exp();
    if !safe(&turned) {
        let to = loosest(turned, &held)
            .or_else(|| loosest(current, &held))
            .unwrap_or(Pose::floor(current.yaw));
        *limit = Some(to);
        return Pose { yaw: to.yaw, ..current.toward(to, tighten) };
    }
    match *limit {
        Some(to) if to.fov < current.fov => Pose { yaw, ..current.toward(to, tighten) },
        _ => turned,
    }
}

/// Points along the boom at which the ground is read for an obstruction,
/// and the halvings that then place it to within a hand's width.
const BOOM_SAMPLES: u32 = 12;
const BOOM_HALVINGS: u32 = 6;
/// Clearance kept between the camera and what it would enter, in world units.
const BOOM_CLEARANCE: f32 = 1.0;
/// Height above the player's feet the boom is measured from.
const HEAD_HEIGHT: f32 = 1.5;

/// Whether the ground or a solid decorator stands at `at`.
fn obstructed(at: Vec3, map: &Map) -> bool {
    let here: Qrz = map.convert(at);
    let Some((floor, _)) = map.get_by_qr(here.q, here.r) else { return false };
    let solid = matches!(
        map.get(here),
        Some(common_bevy::components::entity_type::EntityType::Decorator(d)) if d.is_solid
    );
    solid || surface_y(at.xz(), floor, map) + BOOM_CLEARANCE > at.y
}

/// The fraction of the boom, from the player's head to the camera, that is
/// clear of the ground and of solid decorators: 1 when nothing stands on
/// it. Sampled along the boom, then bisected between the last clear point
/// and the first obstructed one, so the fraction is continuous as the
/// obstruction moves.
fn boom_clearance(head: Vec3, camera: Vec3, map: &Map) -> f32 {
    let at = |t: f32| head.lerp(camera, t);
    let Some(first) = (1..=BOOM_SAMPLES).find(|&i| obstructed(at(i as f32 / BOOM_SAMPLES as f32), map)) else {
        return 1.0;
    };
    let (mut clear, mut blocked) = ((first - 1) as f32 / BOOM_SAMPLES as f32, first as f32 / BOOM_SAMPLES as f32);
    for _ in 0..BOOM_HALVINGS {
        let mid = (clear + blocked) / 2.0;
        if obstructed(at(mid), map) { blocked = mid } else { clear = mid }
    }
    clear
}

pub fn update(
    mut orbit: ResMut<CameraOrbit>,
    mut state: ResMut<CameraPose>,
    mut camera: Query<(&mut Projection, &mut Transform), (With<Camera3d>, Without<CloseupCamera>)>,
    actor: Query<(&Transform, &Heading), (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
    meshes: Res<SummaryMeshes>,
    edges: Res<EdgeCenters>,
    diagnostics: Res<DiagnosticsState>,
    time: Res<Time>,
) {
    let Ok((a_transform, &heading)) = actor.single() else { return };
    let Ok((mut projection, mut c_transform)) = camera.single_mut() else { return };
    let player = a_transform.translation + Vec3::Y * map.radius();
    let dt = time.delta_secs();
    let current = state.pose;

    // The yaw follows the heading. The ground ahead pulls the pose: open
    // ground toward the ceiling, a slope down and in, and the lens opens
    // as far as it takes to keep the rise in the frame. The pulls are
    // smoothed so the ground sampled ahead cannot flick the pose;
    // unloaded ground holds them.
    orbit.follow(heading);
    let aspect = match &*projection {
        Projection::Perspective(p) => p.aspect_ratio,
        _ => 1.0,
    };
    let half_width = (aspect * (current.fov / 2.0).tan()).atan();
    let ahead = ground_ahead(a_transform.translation, heading, half_width, &map);
    if let Some(ahead) = &ahead {
        state.openness = ease(state.openness, ahead.open, PULL_EASE, dt);
    }
    // The plane the camera sweeps on follows the ground under the player.
    if let Some(tilt) = ground_tilt(a_transform.translation, &map) {
        let k = 1.0 - (-TILT_EASE * dt).exp();
        state.tilt = state.tilt.lerp(tilt, k);
    }
    let tilt = state.tilt;
    let mut wanted = Pose { yaw: orbit.target_angle(), ..Pose::rest(0.0).toward(Pose::ceiling(0.0), state.openness) };
    if let Some(ahead) = &ahead {
        wanted.fov = lens_to_hold(&wanted, tilt, ahead.highest);
    }

    let next = if diagnostics.camera_envelope_off {
        step(current, wanted, &mut state.limit, dt, |_| true)
    } else {
        let drawn = DrawnGround::new(player.xz(), &edges.0, &meshes);
        step(current, wanted, &mut state.limit, dt, |p| footprint_is_drawn(p, player, tilt, aspect, &map, &drawn))
    };

    // An obstruction on the boom shortens it further: the camera stands
    // just short, pulling in quickly and letting out slowly.
    let stand = next.transform(player, tilt);
    let head = player + Vec3::Y * HEAD_HEIGHT;
    let clear = boom_clearance(head, stand.translation, &map);
    state.clearance = if clear < state.clearance {
        ease(state.clearance, clear, SHORTEN_EASE, dt)
    } else {
        ease(state.clearance, clear, LENGTHEN_EASE, dt)
    };
    let translation = head.lerp(stand.translation, state.clearance);

    state.pose = next;
    orbit.current = next.yaw;
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = next.fov;
    }
    *c_transform = Transform { translation, ..stand };
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f32 = 2.0;

    fn reach_of(pose: &Pose) -> f32 {
        let player = Vec3::new(300.0, 7.0, -40.0);
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        footprint(pose, player, Vec2::ZERO, WIDE, &map).into_iter()
            .map(|p| p.distance(player.xz()))
            .fold(0.0, f32::max)
    }

    /// The claim the streaming rests on: from rest a turn never waits,
    /// because the frame stays inside the tiles the client holds.
    #[test]
    fn the_resting_footprint_fits_inside_the_streamed_tiles() {
        let inside = common_bevy::chunk::FIXED_STREAM_APOTHEM_WU;
        for yaw in [0.0, 1.0, 2.5, 4.0] {
            assert!(reach_of(&Pose::rest(yaw)) < inside, "rest reaches {} of {inside}", reach_of(&Pose::rest(yaw)));
            assert!(reach_of(&Pose::floor(yaw)) < reach_of(&Pose::rest(yaw)), "the floor is tighter than rest");
        }
    }

    /// The ceiling's top ray is at or above the horizontal, so its footprint
    /// runs to the haze limit and no further.
    #[test]
    fn the_ceiling_reaches_the_haze_limit() {
        let ceiling = Pose::ceiling(0.3);
        let pitch = ceiling.pitch(Vec2::ZERO);
        assert!(pitch - ceiling.fov / 2.0 <= 1e-4, "top ray {}", pitch - ceiling.fov / 2.0);
        let far = reach_of(&ceiling);
        assert!((far - haze_limit_wu()).abs() < CAMERA_DISTANCE + 1.0, "{far} vs {}", haze_limit_wu());
    }

    /// Rising ground ahead opens the lens until the frame's top clears
    /// its sightline; level or falling ground leaves the lens alone.
    #[test]
    fn the_lens_opens_to_hold_a_rise_in_the_frame() {
        let rest = Pose::rest(0.0);
        let up = 20_f32.to_radians();
        let fov = lens_to_hold(&rest, Vec2::ZERO, Some(up));
        assert!(fov > rest.fov && fov <= HILL_FOV, "{fov}");
        let held = Pose { fov, ..rest };
        let top = held.pitch(Vec2::ZERO) - held.fov / 2.0;
        assert!(top <= -up + 1e-4 || fov == HILL_FOV, "top {top} above the slope's sightline {}", -up);
        assert_eq!(lens_to_hold(&rest, Vec2::ZERO, None), rest.fov);
    }

    /// A ray from the eye over flat ground flies free; one cast at a wall
    /// of tiles meets it, and the sensor reads the wall as a full slope.
    #[test]
    fn rays_meet_a_wall_and_fly_free_over_a_plain() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        let ground = EntityType::Decorator(Decorator { index: 0, is_solid: false });
        for q in -220..=220 {
            for r in -220..=220 {
                // A wall of 80 levels across the north (negative z) half, some tiles out.
                let wall = r < -12;
                map.insert(Qrz { q, r, z: if wall { 80 } else { 0 } }, ground);
            }
        }
        let feet = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let plain = ground_ahead(feet, Heading::from_degrees(180.0), 0.5, &map).expect("loaded");
        assert!(plain.open > 0.9 && plain.highest.is_none(), "open {}", plain.open);
        let wall = ground_ahead(feet, Heading::NORTH, 0.5, &map).expect("loaded");
        assert!(wall.open < 0.1 && wall.highest.is_some(), "open {}", wall.open);
    }

    /// On a slope the plane the camera sweeps on tilts with the ground, so
    /// the camera stands lower behind a player facing up it and higher
    /// behind one facing down it; on the level it is neither.
    #[test]
    fn the_sweep_plane_tilts_with_the_ground() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        let ground = EntityType::Decorator(Decorator { index: 0, is_solid: false });
        for q in -60..=60 {
            for r in -60..=60 {
                // A slope rising one level per tile toward -z.
                map.insert(Qrz { q, r, z: -(r + q / 2) }, ground);
            }
        }
        let feet = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let tilt = ground_tilt(feet, &map).expect("loaded");
        assert!(tilt.y < -0.2 && tilt.x.abs() < 0.1, "rises toward -z: {tilt:?}");
        let rest = Pose::rest(0.0);
        let facing_up = Pose { yaw: 0.0, ..rest };
        let facing_down = Pose { yaw: std::f32::consts::PI, ..rest };
        assert!(facing_up.offset(tilt).y < rest.offset(Vec2::ZERO).y, "camera behind a climber stands low");
        assert!(facing_down.offset(tilt).y > rest.offset(Vec2::ZERO).y, "camera behind a descender stands high");
        let reach = |p: &Pose| p.offset(tilt).length();
        assert!(reach(&facing_up) < reach(&facing_down), "an ellipse: nearer looking up, farther looking down");
        assert!(facing_up.offset(tilt).y >= rest.boom * MIN_RISE.tan() - 1e-4, "never under the player's feet");
        assert!(facing_up.pitch(tilt) < facing_down.pitch(tilt), "the frame looks up the slope, then down it");
    }

    /// An envelope on the frame's top ray: safe where it dips at least
    /// `edge` below the horizontal, which tightening makes monotone.
    fn dips_past(edge: f32) -> impl Fn(&Pose) -> bool {
        move |p: &Pose| p.pitch(Vec2::ZERO) - p.fov / 2.0 >= edge
    }

    /// The loosest passing pose on the ladder is the edge itself, not the
    /// ladder's step past it; a passing top is its own answer, and a ladder
    /// whose floor fails has none.
    #[test]
    fn the_loosest_pose_is_on_the_edge() {
        let edge = 10_f32.to_radians();
        let safe = dips_past(edge);
        let ceiling = Pose::ceiling(0.0);
        assert!(!safe(&ceiling) && safe(&Pose::floor(0.0)));
        let found = loosest(ceiling, &safe).expect("the floor is safe");
        let top = found.pitch(Vec2::ZERO) - found.fov / 2.0;
        assert!(safe(&found) && top - edge < 0.1_f32.to_radians(), "top {} for an edge at {edge}", top);
        assert_eq!(loosest(Pose::rest(0.0), &safe), Some(Pose::rest(0.0)));
        assert_eq!(loosest(ceiling, |_| false), None);
    }

    /// Pulled toward the ceiling against an edge that flickers by less
    /// than the margin, the frame opens until it holds its margin inside
    /// the edge and then stands still: never thrown back, never chasing
    /// the flicker. When the edge then moves in past the frame, the frame
    /// tightens once, to its margin inside the new edge, and stands still
    /// again; when the edge moves back out, it opens again.
    #[test]
    fn the_frame_holds_its_margin_inside_a_flickering_edge() {
        let edge = 10_f32.to_radians();
        let flicker = 0.3_f32.to_radians();
        let flickering = |edge: f32| move |p: &Pose, frame: u32| dips_past(edge + if frame % 2 == 0 { flicker } else { -flicker })(p);
        let dt = 1.0 / 60.0;
        let mut pose = Pose::rest(0.0);
        let mut limit = None;
        let run = |pose: &mut Pose, limit: &mut Option<Pose>, frames: u32, safe: &dyn Fn(&Pose, u32) -> bool| -> (f32, f32) {
            let (mut opened, mut closed) = (0.0_f32, 0.0_f32);
            for frame in 0..frames {
                let next = step(*pose, Pose::ceiling(0.0), limit, dt, |p| safe(p, frame));
                let d = next.elevation - pose.elevation;
                if d < 0.0 { opened = opened.max(-d) } else { closed = closed.max(d) }
                *pose = next;
            }
            (opened, closed)
        };
        let sliver = 0.001_f32.to_radians();
        let top = |p: &Pose| p.pitch(Vec2::ZERO) - p.fov / 2.0;

        let (_, closed) = run(&mut pose, &mut limit, 300, &flickering(edge));
        assert!(closed < sliver, "opening toward the edge closed the frame by {closed}");
        assert!(pose.elevation < Pose::rest(0.0).elevation, "the frame opened");
        assert!(dips_past(edge + flicker)(&pose) && !dips_past(edge + flicker)(&pose.tightened(-2.0 * HOLD_MARGIN)),
            "the frame holds its margin inside the edge, and no more: top {} for an edge at {edge}", top(&pose));
        let (opened, closed) = run(&mut pose, &mut limit, 120, &flickering(edge));
        assert!(opened < sliver && closed < sliver, "the held frame moved: opened {opened} closed {closed}");

        let inward = edge + 6_f32.to_radians();
        let (opened, _) = run(&mut pose, &mut limit, 300, &flickering(inward));
        assert!(opened < sliver, "tightening to the new edge opened the frame by {opened}");
        assert!(dips_past(inward + flicker)(&pose) && !dips_past(inward + flicker)(&pose.tightened(-2.0 * HOLD_MARGIN)),
            "the frame holds its margin inside the new edge: top {} for an edge at {inward}", top(&pose));
        let (opened, closed) = run(&mut pose, &mut limit, 120, &flickering(inward));
        assert!(opened < sliver && closed < sliver, "the held frame moved: opened {opened} closed {closed}");

        let (_, closed) = run(&mut pose, &mut limit, 300, &flickering(edge));
        assert!(closed < sliver, "opening back out closed the frame by {closed}");
        assert!(!dips_past(inward)(&pose), "the frame opened back out past the edge that moved away");
    }

    /// Tightening is monotone: each step toward the floor reaches no further
    /// than the last, so the first safe step of the ladder is the loosest.
    #[test]
    fn tightening_never_reaches_further() {
        let mut last = f32::MAX;
        for i in 0..=TIGHTEN_STEPS {
            let far = reach_of(&Pose::ceiling(0.0).tightened(i as f32 / TIGHTEN_STEPS as f32));
            assert!(far <= last + 1e-3, "step {i} reaches {far} after {last}");
            last = far;
        }
    }
}
