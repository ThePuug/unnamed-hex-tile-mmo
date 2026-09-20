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
/// Exponential decay constants of the pitch and lens: closing toward the
/// floor settles in a third of a second; opening out is brisk where the
/// envelope allows the pose and slow where it is waiting on ground to
/// land, so regions arriving one by one do not jog the frame.
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
/// Under a slope the camera comes down and in, so the frame can look up
/// it: the elevation and boom it reaches at a full slope, and the mean
/// grade ahead that counts as full.
const HILL_ELEVATION: f32 = 8_f32.to_radians();
const HILL_BOOM: f32 = 25.0;
const HILL_GRADE: f32 = 25_f32.to_radians();
/// The widest the lens opens to hold rising ground in the frame.
const HILL_FOV: f32 = 60_f32.to_radians();
/// Over a drop the camera goes up and looks down, so the slope under the
/// player's feet fills the frame: the elevation and lens it reaches at a
/// full fall, and the fall ahead that counts as full.
const DROP_ELEVATION: f32 = 40_f32.to_radians();
const DROP_FOV: f32 = 50_f32.to_radians();
const DROP_GRADE: f32 = 25_f32.to_radians();
/// Ground this far above the player's feet, in world units, is a rise the
/// frame keeps in view.
const RISE_MIN_WU: f32 = 4.0;
/// Maximum FOV for flyover mode (admin).
pub const MAX_FLYOVER_FOV: f32 = 90_f32.to_radians();

/// Camera height for normal gameplay (convenience alias).
pub fn gameplay_camera_height() -> f32 {
    camera_height(MAX_GAMEPLAY_FOV)
}

/// The haze: one colour that distance fades everything toward, and the sky
/// above the horizon, so the frontier at the reach never shows.
pub const HAZE_COLOR: Color = Color::linear_rgb(0.72, 0.78, 0.85);
/// Where the haze completes, as a fraction of the reach: inside it, so the
/// frontier stands behind full haze.
const HAZE_END_FRAC: f32 = 0.92;
/// Where the haze begins, as a fraction of the reach: early, so distance
/// reads as distance over the whole view.
const HAZE_START_FRAC: f32 = 0.2;

/// Ground distance at which the haze is complete: the footprint ends here.
pub fn haze_limit_wu() -> f32 {
    common_bevy::summary::reach_wu() * HAZE_END_FRAC
}

/// Distance fog to the haze over the reach, for the camera.
fn haze() -> DistanceFog {
    let reach = common_bevy::summary::reach_wu();
    DistanceFog {
        color: HAZE_COLOR,
        falloff: FogFalloff::Linear { start: reach * HAZE_START_FRAC, end: haze_limit_wu() },
        ..default()
    }
}

/// A camera pose. The camera stands `boom` behind the player along the
/// yaw and `boom · tan(elevation)` above it, and looks along the yaw
/// tilted `pitch` below the horizontal through a lens of `fov`: at the
/// player when the pitch equals the elevation, at the look point ahead
/// when it is less. The frame's top ray dips `pitch - fov / 2`, which is
/// what decides how far the footprint reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Radians counter-clockwise from behind a player facing north.
    pub yaw: f32,
    pub elevation: f32,
    pub pitch: f32,
    pub fov: f32,
    pub boom: f32,
}

impl Pose {
    /// A pose with the player in the lower third of the frame: the pitch
    /// is the elevation less the player's drop.
    fn framed(yaw: f32, elevation: f32, fov: f32, boom: f32) -> Self {
        Pose { yaw, elevation, pitch: elevation - PLAYER_DROP * fov / 2.0, fov, boom }
    }

    /// The widest pose: the frame's top ray horizontal.
    fn ceiling(yaw: f32) -> Self {
        Pose::framed(yaw, CEILING_ELEVATION, CEILING_FOV, CAMERA_DISTANCE)
    }

    /// The pose nothing pulls on: from the height the ladder is measured
    /// by, the lens narrow.
    fn rest(yaw: f32) -> Self {
        Pose::framed(yaw, (gameplay_camera_height() / CAMERA_DISTANCE).atan(), REST_FOV, CAMERA_DISTANCE)
    }

    /// The tightest pose.
    fn floor(yaw: f32) -> Self {
        Pose::framed(yaw, FLOOR_ELEVATION, FLOOR_FOV, CAMERA_DISTANCE)
    }

    /// This pose `t` of the way to `to`, the yaw kept.
    fn toward(self, to: Pose, t: f32) -> Self {
        Pose {
            elevation: self.elevation.lerp(to.elevation, t),
            pitch: self.pitch.lerp(to.pitch, t),
            fov: self.fov.lerp(to.fov, t),
            boom: self.boom.lerp(to.boom, t),
            ..self
        }
    }

    /// The camera's height above the player's feet.
    fn height(&self) -> f32 {
        self.boom * self.elevation.tan()
    }

    /// This pose tightened `t` of the way to the floor.
    fn tightened(self, t: f32) -> Self {
        self.toward(Pose::floor(self.yaw), t)
    }

    /// Where the camera stands and looks, for a player at `player`.
    fn transform(&self, player: Vec3) -> Transform {
        let (sin, cos) = self.yaw.sin_cos();
        let offset = Vec3::new(sin * self.boom, self.boom * self.elevation.tan(), cos * self.boom);
        let forward = Vec3::new(-sin * self.pitch.cos(), -self.pitch.sin(), -cos * self.pitch.cos());
        Transform::from_translation(player + offset).looking_to(forward, Vec3::Y)
    }
}

/// The camera's state between frames: the pose it is at, easing toward a
/// safe one; the smoothed pull of the ground ahead; and the fraction of the
/// boom it stands at.
#[derive(Resource)]
pub struct CameraPose {
    pub pose: Pose,
    openness: f32,
    /// How steeply the ground ahead climbs, 0 to 1, smoothed: what brings
    /// the camera down and in to look up it.
    slope: f32,
    /// How steeply the ground ahead falls away, 0 to 1, smoothed: what
    /// brings the camera up to look down it.
    drop: f32,
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
    commands.insert_resource(CameraPose { pose: Pose::floor(0.0), openness: 0.0, slope: 0.0, drop: 0.0, clearance: 1.0 });
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
/// Ground distance the rays cast below the horizontal reach, and how many
/// there are: the slope under the feet is nearer than the view ahead. A
/// ray that flies free that far has ground falling away beneath it.
const DROP_REACH_WU: f32 = 120.0;
const DOWN_FAN_RAYS: u32 = 9;
/// Elevation of the lowest ray of the fan: the one that flying free to
/// the reach makes the ground open. A little above level, so a plain that
/// rises gently still counts as open.
const OPEN_ELEVATION: f32 = 3_f32.to_radians();
/// Height of the eye the sensor rays are cast from, over the feet.
const EYE_HEIGHT: f32 = 1.5;

/// The ground ahead, read by rays from the eye across a sweep about the
/// heading: how open it is, how steeply it climbs, and the steepest
/// sightline up it inside the frame.
struct Ahead {
    /// 0 (closed) to 1 (open): the share of the sweep's level rays that
    /// fly free to the reach. What pulls the pose toward the ceiling.
    open: f32,
    /// 0 (level or falling) to 1 (a full grade): what brings the camera
    /// down and in to look up the slope. The rays that agree with the
    /// majority of the sweep set it, so a hill across most of the view is
    /// a hill whatever the one ray ahead crosses.
    slope: f32,
    /// 0 (level or rising) to 1 (a full fall): what brings the camera up
    /// to look down the slope. The steepest depression at which a ray
    /// still flies free of the ground, majority-weighted like the slope.
    fall: f32,
    /// The highest elevation, in radians above the horizontal, at which a
    /// ray inside the frame's width still meets the ground: None where
    /// none does.
    highest: Option<f32>,
}

/// The majority's mean over a sweep of readings: the readings that agree
/// with most of the sweep on whether there is anything there set it, so
/// one ray crossing a gap does not.
fn majority_mean(readings: &[f32]) -> f32 {
    let some = readings.iter().filter(|&&g| g > 0.0).count();
    let majority_some = 2 * some > readings.len();
    let (sum, n) = readings.iter()
        .filter(|&&g| (g > 0.0) == majority_some)
        .fold((0.0, 0), |(s, n), &g| (s + g, n + 1));
    if n > 0 { sum / n as f32 } else { 0.0 }
}

/// The ground ahead, or None where too little of it is loaded to read.
/// `half_width` is the frame's horizontal half-angle: only rays inside it
/// feed the sightline the lens is opened to clear.
fn ground_ahead(player: Vec3, heading: Heading, half_width: f32, map: &Map) -> Option<Ahead> {
    let eye = player + Vec3::Y * EYE_HEIGHT;
    let ahead = heading.to_world_dir();
    let mut grades = Vec::with_capacity((2 * SWEEP_RAYS + 1) as usize);
    let mut falls = Vec::with_capacity((2 * SWEEP_RAYS + 1) as usize);
    let mut free = 0.0_f32;
    let mut highest: Option<f32> = None;
    for k in -SWEEP_RAYS..=SWEEP_RAYS {
        let yaw = k as f32 * SWEEP_STEP;
        let flat = Vec2::from_angle(yaw).rotate(ahead);
        // The fall along this ray: the steepest depression at which a ray
        // still flies free to the drop reach.
        let mut fall = 0.0;
        for j in 1..=DOWN_FAN_RAYS {
            let down = j as f32 * FAN_STEP;
            let dir = Vec3::new(flat.x * down.cos(), -down.sin(), flat.y * down.cos());
            match march(eye, dir, DROP_REACH_WU, map) {
                Ok(None) => fall = down,
                Ok(Some(_)) => break,
                Err(()) => return None,
            }
        }
        falls.push(fall);
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
        grades.push(grade.unwrap_or(0.0));
        if yaw.abs() <= half_width {
            if let Some(up) = grade {
                highest = Some(highest.map_or(up, |h: f32| h.max(up)));
            }
        }
    }
    let open = free / grades.len() as f32;
    Some(Ahead {
        open,
        slope: (majority_mean(&grades) / HILL_GRADE).clamp(0.0, 1.0),
        fall: (majority_mean(&falls) / DROP_GRADE).clamp(0.0, 1.0),
        highest,
    })
}

/// The lens `pose` needs to keep the ground ahead in the frame with the
/// player in the lower third: the frame's top ray must clear the steepest
/// sightline up the ground, `highest` above the horizontal, and the pitch
/// the framing fixes puts the top `(1 + drop) · fov / 2` above the
/// player's own sightline.
fn lens_to_hold(pose: &Pose, highest: Option<f32>) -> f32 {
    let Some(up) = highest else { return pose.fov };
    let needed = 2.0 * (pose.elevation + up) / (1.0 + PLAYER_DROP);
    needed.clamp(pose.fov, HILL_FOV)
}

/// Frame samples across and down the frame for the footprint.
const FOOTPRINT_COLUMNS: u32 = 9;
const FOOTPRINT_ROWS: u32 = 7;
/// Steps toward the floor tried before the yaw holds.
const TIGHTEN_STEPS: u32 = 16;

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
fn footprint(pose: &Pose, player: Vec3, aspect: f32, map: &Map) -> Vec<Vec2> {
    let camera = pose.transform(player);
    let half_v = (pose.fov / 2.0).tan();
    let half_h = half_v * aspect;
    let limit = haze_limit_wu();
    let mut rows: Vec<f32> = (0..FOOTPRINT_ROWS).map(|row| row as f32 / (FOOTPRINT_ROWS - 1) as f32 * 2.0 - 1.0).collect();
    let (top, bottom) = (pose.pitch - pose.fov / 2.0, pose.pitch + pose.fov / 2.0);
    if top < 0.0 {
        // The row at which a ray lands on the haze limit, then the horizon rows.
        let height = camera.translation.y - player.y;
        let at_limit = height.atan2(limit);
        rows.extend(
            HORIZON_ROWS.iter().copied().chain(std::iter::once(at_limit))
                .filter(|&down| down > top && down < bottom)
                .map(|down| (pose.pitch - down).tan() / half_v),
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
fn footprint_is_drawn(pose: &Pose, player: Vec3, aspect: f32, map: &Map, drawn: &DrawnGround) -> bool {
    footprint(pose, player, aspect, map).into_iter().all(|p| drawn.at(p))
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
        state.slope = ease(state.slope, ahead.slope, PULL_EASE, dt);
        state.drop = ease(state.drop, ahead.fall, PULL_EASE, dt);
    }
    let open = Pose::rest(0.0).toward(Pose::ceiling(0.0), state.openness);
    let hill = Pose::framed(0.0, HILL_ELEVATION, open.fov, HILL_BOOM);
    let drop = Pose::framed(0.0, DROP_ELEVATION, DROP_FOV, CAMERA_DISTANCE);
    let pulled = open.toward(hill, state.slope).toward(drop, state.drop);
    let mut wanted = Pose { yaw: orbit.target_angle(), ..pulled };
    if let Some(ahead) = &ahead {
        wanted = Pose::framed(wanted.yaw, wanted.elevation, lens_to_hold(&wanted, ahead.highest), wanted.boom);
    }

    // Where the frame would go this frame, before the envelope.
    let diff = angle_diff(current.yaw, wanted.yaw);
    let yaw = if diff.abs() > SNAP_THRESHOLD {
        (current.yaw + diff * (1.0 - (-YAW_EASE * dt).exp())).rem_euclid(2.0 * PI)
    } else {
        wanted.yaw
    };
    let opened = Pose { yaw, ..current.toward(wanted, 1.0 - (-OPEN_EASE * dt).exp()) };

    let target = if diagnostics.camera_envelope_off {
        opened
    } else {
        let drawn = DrawnGround::new(player.xz(), &edges.0, &meshes);
        let safe = |p: &Pose| footprint_is_drawn(p, player, aspect, &map, &drawn);
        // The turn with the frame tightened as far as it takes, then the
        // yaw held with the same, then the floor.
        let pick = |yaw: f32| {
            (0..=TIGHTEN_STEPS)
                .map(|i| Pose { yaw, ..opened }.tightened(i as f32 / TIGHTEN_STEPS as f32))
                .find(safe)
        };
        pick(opened.yaw)
            .or_else(|| pick(current.yaw))
            .unwrap_or(Pose { yaw: current.yaw, ..opened }.tightened(1.0))
    };

    // Every axis eases: closing toward the floor quickly, opening briskly
    // where the envelope let the frame go, slowly where it held it back.
    // The lens says which way the frame is going.
    let k = if target.fov < current.fov {
        TIGHTEN_EASE
    } else if target == opened {
        OPEN_EASE
    } else {
        WAIT_EASE
    };
    let next = Pose { yaw: target.yaw, ..current.toward(target, 1.0 - (-k * dt).exp()) };

    // An obstruction on the boom shortens it further: the camera stands
    // just short, pulling in quickly and letting out slowly.
    let stand = next.transform(player);
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
        footprint(pose, player, WIDE, &map).into_iter()
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
        assert!(ceiling.pitch - ceiling.fov / 2.0 <= 1e-4, "top ray {}", ceiling.pitch - ceiling.fov / 2.0);
        let far = reach_of(&ceiling);
        assert!((far - haze_limit_wu()).abs() < CAMERA_DISTANCE + 1.0, "{far} vs {}", haze_limit_wu());
    }

    /// Rising ground ahead opens the lens until the frame's top clears
    /// its sightline; level or falling ground leaves the lens alone.
    #[test]
    fn the_lens_opens_to_hold_a_rise_in_the_frame() {
        let rest = Pose::rest(0.0);
        let up = 20_f32.to_radians();
        let fov = lens_to_hold(&rest, Some(up));
        assert!(fov > rest.fov && fov <= HILL_FOV, "{fov}");
        let framed = Pose::framed(0.0, rest.elevation, fov, rest.boom);
        let top = framed.pitch - framed.fov / 2.0;
        assert!(top <= -up + 1e-4 || fov == HILL_FOV, "top {top} above the slope's sightline {}", -up);
        assert_eq!(lens_to_hold(&rest, None), rest.fov);
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
        assert!(plain.open > 0.9 && plain.slope == 0.0, "open {} slope {}", plain.open, plain.slope);
        let wall = ground_ahead(feet, Heading::NORTH, 0.5, &map).expect("loaded");
        assert!(wall.open < 0.1 && wall.slope > 0.5 && wall.highest.is_some(), "open {} slope {}", wall.open, wall.slope);
        assert_eq!(plain.fall, 0.0, "a plain falls nowhere");
        // On the wall's brink, facing out over it, the rays below fly free.
        let brink = map.convert(Qrz { q: 0, r: -13, z: 81 });
        let over = ground_ahead(brink, Heading::from_degrees(180.0), 0.5, &map).expect("loaded");
        assert!(over.fall > 0.5 && over.slope == 0.0, "fall {} slope {}", over.fall, over.slope);
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
