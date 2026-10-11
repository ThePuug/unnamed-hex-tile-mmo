//! The gameplay camera: a pose behind the player that follows the heading,
//! opens toward the ceiling over open ground, and comes down to look up a
//! climb. No key moves it.

use bevy::{core_pipeline::prepass::DepthPrepass, pbr::{DistanceFog, FogFalloff}, prelude::*};
use crate::resources::RenderOrigin;
use crate::systems::{closeup::CloseupCamera, ease};
use std::f32::consts::PI;

use bevy::render::view::{ColorGrading, ColorGradingGlobal};
use crate::plugins::{diagnostics::DiagnosticsState, ink::Ink, vignette::VignetteSettings};
use common_bevy::{
    components::{heading::{Heading, HEADING_SLOTS}, position::VisualPosition, *},
    resources::map::Map,
    systems::movement::{standing_y, surface_y_from},
};

/// Orbit stops: one per heading, so the camera can stand behind any.
pub const ORBIT_STOPS: usize = HEADING_SLOTS as usize;
/// Angular separation between orbit stops.
const ORBIT_STEP: f32 = 2.0 * PI / ORBIT_STOPS as f32;
/// Exponential decay constant of the yaw's easing: slow enough that the
/// heading's steps blend into one turn, quick enough to lag it by a stop
/// or two at most.
const YAW_EASE: f32 = 8.0;
/// Exponential decay constant of the boom and lens easing toward the
/// wanted pose.
const POSE_EASE: f32 = 4.0;
/// Exponential decay constant of the pull's smoothing, so the ground
/// sampled ahead cannot flick the pose as the player walks.
const PULL_EASE: f32 = 2.0;
/// Exponential decay constant of the boom's clearance letting back out
/// after an obstruction. Shortening onto one is immediate, so the camera
/// never enters what stands on the boom.
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
/// The highest the boom is lifted to clear what stands on it.
const LIFT_ELEVATION: f32 = 55_f32.to_radians();
/// How far below the frame's centre the player stands, as a fraction of
/// the half-height: the look point is that much ahead on the ground. And
/// how far above it the crest of a climb stands, the same, so the frame
/// points up the climb with sky over it.
const PLAYER_DROP: f32 = 1.0 / 3.0;
const CREST_RISE: f32 = 1.0 / 3.0;
/// The widest the lens opens to hold rising ground in the frame, and the
/// rise ahead at which the frame has fully committed to the pose that
/// holds it.
const HILL_FOV: f32 = 60_f32.to_radians();
const HILL_GRADE: f32 = 25_f32.to_radians();
/// The plane the camera sweeps on is the ground's under the player: the
/// steepest it is allowed to tilt, short of the angle of repose so a brink
/// does not flip it, and the decay constant of its smoothing.
const TILT_MAX: f32 = 30_f32.to_radians();
const TILT_EASE: f32 = 3.0;
/// As the boom draws in, by the pose's reach or to an obstruction, the rig
/// slides this far over the player's right shoulder so the player does
/// not fill the frame.
const SHOULDER_WU: f32 = 1.2;
/// Among trees the camera closes in: the rest pose's elevation on a boom
/// this share of its length, with this lens, so the frame still looks down
/// on the player from over the shoulder and never up past the horizon. It
/// closes in quickly and draws back out slowly, so a gap in a wood does
/// not pump the boom.
const CLOSE_REACH: f32 = 0.075;
const CLOSE_FOV: f32 = 40_f32.to_radians();
const CLOSE_IN_EASE: f32 = 3.0;
const CLOSE_OUT_EASE: f32 = 0.4;
/// Ground distances around the player at which the plane is fitted, in
/// world units, in each of six directions; and, closed in among trees,
/// two and three tiles out, since a boom that short stands on the ground
/// at the player's feet and not on the hillside around it.
const TILT_RINGS_WU: [f32; 2] = [20.0, 45.0];
const CLOSE_TILT_RINGS_WU: [f32; 2] = [2.0 * TILE_ACROSS_WU, 3.0 * TILE_ACROSS_WU];
/// A tile's width flat to flat.
const TILE_ACROSS_WU: f32 = 1.732_050_8 * common::grid::HEX_RADIUS;

/// The haze at noon: one colour that distance fades everything toward, and
/// the sky above the horizon, so the frontier at the reach never shows.
/// `world::update` lights both by the sun and moon through the day, the
/// sky in the sun's tint and the haze in a share of it.
pub const HAZE_COLOR: Color = Color::linear_rgb(0.50, 0.66, 0.90);
/// Where the haze completes, as a fraction of the reach: inside it, so the
/// frontier stands behind full haze.
const HAZE_END_FRAC: f32 = 0.92;

/// Ground distance at which the haze is complete.
pub fn haze_limit_wu() -> f32 {
    common_bevy::summary::reach_wu() * HAZE_END_FRAC
}

/// Distance fog to the haze, for the camera: deepening steadily with the
/// distance and complete at the haze limit, so each hill further off
/// stands paler and bluer than the one before it. As the square of the
/// distance it would lay under half a percent on a hill two thousand units
/// off, and the whole view short of the reach would stand in clear air.
fn haze() -> DistanceFog {
    DistanceFog {
        color: HAZE_COLOR,
        falloff: FogFalloff::from_visibility(haze_limit_wu()),
        ..default()
    }
}

/// A camera pose. The camera stands at the end of the boom behind the
/// player's eye along the yaw, at the height of the ground's plane
/// through the eye there plus the boom's `elevation`, never negative, and
/// looks along the yaw through a lens of `fov` with the player a third of
/// the way down the frame. The sweep is an ellipse on the plane: the
/// boom's reach on the ground holds and the height follows the slope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Radians counter-clockwise from behind a player facing north.
    pub yaw: f32,
    pub elevation: f32,
    pub fov: f32,
    /// The boom's length as a share of its full length.
    pub reach: f32,
}

/// The ground's plane under the player as a gradient: its rise per unit
/// of run in world x and z. Zero is level.
pub type Tilt = Vec2;

impl Pose {
    /// The widest pose: the frame's top ray above the horizontal.
    fn ceiling(yaw: f32) -> Self {
        Pose { yaw, elevation: CEILING_ELEVATION, fov: CEILING_FOV, reach: 1.0 }
    }

    /// The pose from the height the ladder and the streamed tiles are
    /// measured by, the lens narrow: the elevation the close pose keeps.
    fn rest(yaw: f32) -> Self {
        Pose { yaw, elevation: (camera_height(MAX_GAMEPLAY_FOV) / CAMERA_DISTANCE).atan(), fov: REST_FOV, reach: 1.0 }
    }

    /// The pose among trees: rest's elevation, over the shoulder on a
    /// short boom.
    fn close(yaw: f32) -> Self {
        Pose { yaw, fov: CLOSE_FOV, reach: CLOSE_REACH, ..Pose::rest(yaw) }
    }

    /// The pose that holds a climb of `up` ahead: the ceiling's lens, and
    /// the elevation that puts the crest its rise above the frame's centre
    /// on ground of `tilt` — no higher than the ceiling's and never below
    /// the eye, the lens opening for what the elevation cannot reach. On a
    /// slope the plane has already brought the camera down, so the same
    /// climb asks less.
    fn hill(yaw: f32, up: f32, tilt: Tilt) -> Self {
        let open = Pose::ceiling(yaw);
        let crest = (PLAYER_DROP + CREST_RISE) * open.fov / 2.0;
        let elevation = ((crest - up).tan() - tilt.dot(open.back())).atan().clamp(0.0, open.elevation);
        Pose { elevation, ..open }
    }

    /// This pose `t` of the way to `to`, the yaw kept.
    fn toward(self, to: Pose, t: f32) -> Self {
        Pose {
            elevation: self.elevation.lerp(to.elevation, t),
            fov: self.fov.lerp(to.fov, t),
            reach: self.reach.lerp(to.reach, t),
            ..self
        }
    }

    /// The direction from the player to the camera on the ground.
    fn back(&self) -> Vec2 {
        let (sin, cos) = self.yaw.sin_cos();
        Vec2::new(sin, cos)
    }

    /// The boom's length.
    fn boom(&self) -> f32 {
        CAMERA_DISTANCE * self.reach
    }

    /// Where the camera stands relative to the player's eye on ground of
    /// `tilt`: the boom's length back on the ground, and up by the plane's
    /// rise there plus the elevation. So behind a player facing up a slope
    /// the camera stands low and near, facing down it high and far.
    fn offset(&self, tilt: Tilt) -> Vec3 {
        let back = self.back();
        let boom = self.boom();
        let rise = self.elevation.tan() + tilt.dot(back);
        Vec3::new(back.x * boom, boom * rise, back.y * boom)
    }

    /// The rig's slide over the player's right shoulder for a camera
    /// `reach` back on the ground: nothing at the boom's full length,
    /// growing as it draws in, whether the pose drew it in or an
    /// obstruction did.
    fn shift(&self, reach: f32) -> Vec3 {
        let back = self.back();
        Vec3::new(back.y, 0.0, -back.x) * (SHOULDER_WU * (1.0 - reach / CAMERA_DISTANCE))
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

    /// Where the camera stands and looks, for a player whose eye is at
    /// `eye` on ground of `tilt`, before the slide over the shoulder.
    fn transform(&self, eye: Vec3, tilt: Tilt) -> Transform {
        let back = self.back();
        let pitch = self.pitch(tilt);
        let forward = Vec3::new(-back.x * pitch.cos(), -pitch.sin(), -back.y * pitch.cos());
        Transform::from_translation(eye + self.offset(tilt)).looking_to(forward, Vec3::Y)
    }
}

/// The camera's state between frames: the pose it is at, easing toward the
/// wanted one; the smoothed pull of the ground ahead; and the fraction of
/// the boom it stands at.
#[derive(Resource)]
pub struct CameraPose {
    pub pose: Pose,
    /// How steeply the ground ahead climbs, in radians, smoothed.
    pub climb: f32,
    /// The ground's plane under the player, smoothed: what the camera
    /// sweeps on.
    pub tilt: Tilt,
    /// The fraction of the boom the camera stands at.
    pub clearance: f32,
    /// How far the camera has closed in among trees, 0 to 1, smoothed.
    closed: f32,
}

/// whether or not it is on the line: a boom drawn in among crowns would
/// otherwise show their insides at the near plane.
pub const NEAR_FADE_RADIUS: f32 = 2.5;

/// The yaw of the orbit stop behind `heading`, in radians: 0 behind a
/// player facing north, counter-clockwise from there.
fn stop_yaw(heading: Heading) -> f32 {
    ((ORBIT_STOPS - heading.slot() as usize) % ORBIT_STOPS) as f32 * ORBIT_STEP
}

/// Shortest signed angle from `from` to `to` on the unit circle.
fn angle_diff(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(2.0 * PI);
    if d > PI { d - 2.0 * PI } else { d }
}

pub fn setup(
    mut commands: Commands,
) {
    commands.insert_resource(CameraPose { pose: Pose::rest(0.0), climb: 0.0, tilt: Vec2::ZERO, clearance: 1.0, closed: 0.0 });
    commands.insert_resource(ClearColor(HAZE_COLOR));

    commands.spawn((
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: REST_FOV,
            // Near enough that the boom's ground clearance, which cannot be
            // less, leaves a pose room beneath the eye. Reverse-z float
            // depth, so precision does not depend on it.
            near: 0.25,
            // Culling only: the projection is infinite reverse-z. Past the
            // reach, so the coarsest band is drawn to its edge.
            far: common_bevy::summary::reach_wu() * 1.5,
            ..default()
        }),
        Transform::default(),
        Actor,
        // Every UI node is drawn by the world's camera: the closeup's draws
        // into a texture.
        IsDefaultUiCamera,
        VignetteSettings::default(),
        haze(),
        // Depth first, so the terrain's fragment shader runs once per pixel
        // that shows and never for one another tile covers.
        DepthPrepass,
        Ink,
        grade(),
    ));
}

/// The look's grade: bright colour, cooled toward blue. Every draw is
/// graded by the view's tone mapping, the forest's own draws included, so
/// this is the one place the palette is turned.
pub fn grade() -> ColorGrading {
    ColorGrading {
        global: ColorGradingGlobal {
            temperature: -0.08,
            post_saturation: 1.15,
            ..default()
        },
        ..default()
    }
}

/// How far a ray is marched against the loaded tiles, in world units, and
/// in how many steps: spaced as squares, dense near and sparse far.
const MARCH_STEPS: u32 = 12;

/// Where a ray from `from` along the unit `dir` first meets the terrain
/// within `reach`: `Ok(Some(t))` at that distance along the ray,
/// `Ok(None)` when it flies free, `Err(())` when it crosses ground that
/// is not loaded before meeting any. Read against the tiles' standing
/// heights, so a slope reads at the resolution of its tiles. The ray is
/// in rendered coordinates, read on the map through `origin`.
fn march(from: Vec3, dir: Vec3, reach: f32, map: &Map, origin: &RenderOrigin) -> Result<Option<f32>, ()> {
    for i in 1..=MARCH_STEPS {
        let f = i as f32 / MARCH_STEPS as f32;
        let t = reach * f * f;
        let at = from + dir * t;
        let here = origin.tile_at(map, at);
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
/// the reach makes the ground no climb. A little above level, so a plain
/// that rises gently is none.
const OPEN_ELEVATION: f32 = 3_f32.to_radians();
/// Height of the eye the sensor rays are cast from, over the feet.
const EYE_HEIGHT: f32 = 1.5;

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

/// The ground's plane under a player standing at `player`: the gradient
/// fitted to the tiles' standing heights on rings around the feet, capped
/// at the steepest tilt. None where a ring is not loaded. Six directions
/// on each ring make the fit's normal equations diagonal.
fn ground_tilt(player: Vec3, rings: [f32; 2], map: &Map, origin: &RenderOrigin) -> Option<Tilt> {
    let (mut xh, mut zh, mut xx) = (0.0_f32, 0.0_f32, 0.0_f32);
    for radius in rings {
        for k in 0..6 {
            let angle = k as f32 * std::f32::consts::TAU / 6.0;
            let p = Vec2::from_angle(angle) * radius;
            let at = player + Vec3::new(p.x, 0.0, p.y);
            let here = origin.tile_at(map, at);
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

/// How steeply the ground ahead climbs, in radians above the horizontal,
/// read by rays from the eye across a sweep about the heading: the
/// steepest elevation at which a ray still meets the ground, over the rays
/// inside the frame's horizontal half-angle `half_width` that agree with
/// most of them on whether there is ground there. Zero where there is
/// none; None where too little of it is loaded to read. What brings the
/// camera down to look up it.
fn climb_ahead(feet: Vec3, heading: Heading, half_width: f32, map: &Map, origin: &RenderOrigin) -> Option<f32> {
    let eye = feet + Vec3::Y * EYE_HEIGHT;
    let ahead = heading.to_world_dir();
    let mut grades = Vec::with_capacity((2 * SWEEP_RAYS + 1) as usize);
    for k in -SWEEP_RAYS..=SWEEP_RAYS {
        let yaw = k as f32 * SWEEP_STEP;
        if yaw.abs() > half_width {
            continue;
        }
        let flat = Vec2::from_angle(yaw).rotate(ahead);
        // The grade along this ray: the steepest elevation at which a ray
        // still meets the ground within the reach; the lowest ray flying
        // free is no climb.
        let mut grade = None;
        for j in 0..FAN_RAYS {
            let up = OPEN_ELEVATION + j as f32 * FAN_STEP;
            let dir = Vec3::new(flat.x * up.cos(), up.sin(), flat.y * up.cos());
            match march(eye, dir, AHEAD_REACH_WU, map, origin) {
                Ok(Some(_)) => grade = Some(up),
                Ok(None) => break,
                Err(()) => return None,
            }
        }
        grades.push(grade.unwrap_or(0.0));
    }
    Some(majority_mean(&grades))
}

/// The lens `pose` needs to keep the ground ahead in the frame with the
/// player in the lower third: the frame's top ray must clear the climb,
/// `up` above the horizontal, and the pitch the framing fixes puts the top
/// `(1 + drop) · fov / 2` above the camera's sightline down to the player.
fn lens_to_hold(pose: &Pose, tilt: Tilt, up: f32) -> f32 {
    if up <= 0.0 {
        return pose.fov;
    }
    let needed = 2.0 * (pose.down(tilt) + up) / (1.0 + PLAYER_DROP);
    needed.clamp(pose.fov, HILL_FOV)
}

/// `pose` raised no further than clears its boom of what stands on it —
/// the hillside behind a player facing down a slope steeper than the plane
/// follows — its lens and reach untouched. `pose` itself where the boom is
/// already clear, or where not even the lift's highest clears it, since
/// only shortening the boom then can.
fn lifted(pose: Pose, eye: Vec3, tilt: Tilt, map: &Map, origin: &RenderOrigin) -> Pose {
    let top = pose.elevation.max(LIFT_ELEVATION);
    let at = |t: f32| Pose { elevation: pose.elevation.lerp(top, t), ..pose };
    first_passing(at, |p| boom_is_clear(p, eye, tilt, map, origin)).unwrap_or(pose)
}

/// Steps of the ladder a lift tries, and the halvings that then bring the
/// first passing step back to the edge it crossed, so the camera is not
/// lifted a whole step past it.
const LADDER_STEPS: u32 = 16;
const EDGE_HALVINGS: u32 = 8;

/// The first pose on `ladder`, from `ladder(0.0)` to `ladder(1.0)`, that
/// passes `test`: the ladder's start where it passes, else the edge it
/// crossed to within the halvings' resolution, found between the ladder's
/// last failing step and its first passing one. None where not even its
/// end passes.
fn first_passing(at: impl Fn(f32) -> Pose, test: impl Fn(&Pose) -> bool) -> Option<Pose> {
    let first = (0..=LADDER_STEPS).find(|&i| test(&at(i as f32 / LADDER_STEPS as f32)))?;
    if first == 0 {
        return Some(at(0.0));
    }
    let (mut open, mut held) = ((first - 1) as f32 / LADDER_STEPS as f32, first as f32 / LADDER_STEPS as f32);
    for _ in 0..EDGE_HALVINGS {
        let mid = (open + held) / 2.0;
        if test(&at(mid)) { held = mid } else { open = mid }
    }
    Some(at(held))
}

/// One frame's step of the pose over `dt`: the yaw turning toward the
/// wanted one, the boom and lens easing toward the wanted pose.
fn step(current: Pose, wanted: Pose, dt: f32) -> Pose {
    let diff = angle_diff(current.yaw, wanted.yaw);
    let yaw = if diff.abs() > SNAP_THRESHOLD {
        (current.yaw + diff * (1.0 - (-YAW_EASE * dt).exp())).rem_euclid(2.0 * PI)
    } else {
        wanted.yaw
    };
    Pose { yaw, ..current.toward(wanted, 1.0 - (-POSE_EASE * dt).exp()) }
}

/// Points along the boom at which the ground is read for an obstruction,
/// about a tile apart at its full length so a bump a tile across is not
/// stepped over, and the halvings that then place it to within a hand's
/// width.
const BOOM_SAMPLES: u32 = 24;
const BOOM_HALVINGS: u32 = 6;
/// Clearance kept between the camera and what it would enter, in world
/// units: no less than the near plane, or the ground at the frame's
/// bottom edge is clipped when the boom is shortened onto a slope.
const BOOM_CLEARANCE: f32 = 0.3;
/// The height above the ground the camera stands at, at least, at the
/// boom's full length: the sightline to the player keeps a clearance over
/// the ground growing from `BOOM_CLEARANCE` at the eye to this, so a bump
/// between them stays under the frame.
const STAND_HEIGHT: f32 = 4.0;

/// Whether the ground, within `clearance` of it, or a solid decorator
/// stands at `at`, a rendered point.
fn obstructed(at: Vec3, clearance: f32, map: &Map, origin: &RenderOrigin) -> bool {
    let here = origin.tile_at(map, at);
    let Some((floor, _)) = map.get_by_qr(here.q, here.r) else { return false };
    let solid = matches!(
        map.get(here),
        Some(common_bevy::components::entity_type::EntityType::Decorator(d)) if d.is_solid
    );
    solid || surface_y_from(origin.tile(), at.xz(), floor, map) + clearance > at.y
}

/// Whether `pose`'s sightline from the player's eye at `eye` on ground of
/// `tilt` keeps its clearance over the ground all the way to the camera,
/// and nothing solid stands on it: the rig slid over the shoulder as far
/// as the pose's own reach slides it.
fn boom_is_clear(pose: &Pose, eye: Vec3, tilt: Tilt, map: &Map, origin: &RenderOrigin) -> bool {
    let shift = pose.shift(pose.boom());
    let (head, foot) = (eye + shift, pose.transform(eye, tilt).translation + shift);
    let rise = (STAND_HEIGHT - BOOM_CLEARANCE) * pose.boom() / CAMERA_DISTANCE;
    (1..=BOOM_SAMPLES).all(|i| {
        let t = i as f32 / BOOM_SAMPLES as f32;
        !obstructed(head + (foot - head) * t, BOOM_CLEARANCE + rise * t, map, origin)
    })
}

/// The fraction of the boom, from the player's eye to the camera, that is
/// clear of the ground and of solid decorators: 1 when nothing stands on
/// it. Sampled along the boom, then bisected between the last clear point
/// and the first obstructed one, so the fraction is continuous as the
/// obstruction moves.
fn boom_clearance(eye: Vec3, camera: Vec3, map: &Map, origin: &RenderOrigin) -> f32 {
    let at = |t: f32| eye + (camera - eye) * t;
    let Some(first) = (1..=BOOM_SAMPLES).find(|&i| obstructed(at(i as f32 / BOOM_SAMPLES as f32), BOOM_CLEARANCE, map, origin)) else {
        return 1.0;
    };
    let (mut clear, mut blocked) = ((first - 1) as f32 / BOOM_SAMPLES as f32, first as f32 / BOOM_SAMPLES as f32);
    for _ in 0..BOOM_HALVINGS {
        let mid = (clear + blocked) / 2.0;
        if obstructed(at(mid), BOOM_CLEARANCE, map, origin) { blocked = mid } else { clear = mid }
    }
    clear
}

/// The boom's clearance this frame, from `current` toward `measured`:
/// onto an obstruction at once, so the camera never enters what stands
/// on the boom; back out eased.
fn clearance_step(current: f32, measured: f32, dt: f32) -> f32 {
    ease(current, measured, LENGTHEN_EASE, dt).min(measured)
}

pub fn update(
    mut state: ResMut<CameraPose>,
    mut camera: Query<(&mut Projection, &mut Transform), (With<Camera3d>, Without<CloseupCamera>)>,
    actor: Query<(&VisualPosition, &Heading), (With<crate::components::Viewed>, Without<Camera3d>)>,
    map: Res<Map>,
    diagnostics: Res<DiagnosticsState>,
    time: Res<Time>,
    render_origin: Res<RenderOrigin>,
) {
    let Ok((visual, &heading)) = actor.single() else { return };
    let Ok((mut projection, mut c_transform)) = camera.single_mut() else { return };
    // The visual, not the actor's Transform: the same value the actor is
    // drawn at, whichever of the two systems runs first. Everything here is
    // in rendered coordinates; the map is read through the origin.
    let feet = visual.current();
    let origin = &*render_origin;
    let eye = feet + Vec3::Y * EYE_HEIGHT;
    let dt = time.delta_secs();
    let current = state.pose;

    // The yaw follows the heading. The ground ahead pulls the pose: open
    // ground leaves it at the ceiling, a climb brings it down to the pose
    // that holds it, and the lens opens as far as it takes for what is
    // left. The pull is smoothed so the ground sampled ahead cannot flick
    // the pose; unloaded ground holds it.
    let aspect = match &*projection {
        Projection::Perspective(p) => p.aspect_ratio,
        _ => 1.0,
    };
    let half_width = (aspect * (current.fov / 2.0).tan()).atan();
    if let Some(climb) = climb_ahead(feet, heading, half_width, &map, origin) {
        state.climb = ease(state.climb, climb, PULL_EASE, dt);
    }
    // The plane the camera sweeps on follows the ground under the player.
    let rings = [0, 1].map(|i| TILT_RINGS_WU[i].lerp(CLOSE_TILT_RINGS_WU[i], state.closed));
    if let Some(tilt) = ground_tilt(feet, rings, &map, origin) {
        let k = 1.0 - (-TILT_EASE * dt).exp();
        state.tilt = state.tilt.lerp(tilt, k);
    }
    let tilt = state.tilt;
    let yaw = stop_yaw(heading);
    let mut wanted = Pose::ceiling(yaw).toward(Pose::hill(yaw, state.climb, tilt), (state.climb / HILL_GRADE).min(1.0));
    wanted.fov = lens_to_hold(&wanted, tilt, state.climb);

    // Among trees the camera closes in over the shoulder.
    let tile = origin.tile_at(&map, feet);
    let among = crate::plugins::cover::among_trees(&map, tile.q, tile.r);
    let (to, k) = if among { (1.0, CLOSE_IN_EASE) } else { (0.0, CLOSE_OUT_EASE) };
    state.closed = ease(state.closed, to, k, dt);
    wanted = wanted.toward(Pose::close(wanted.yaw), state.closed);

    // The close-up holds the boom level with the eye whatever the ground says.
    if diagnostics.camera_closeup {
        wanted = Pose { yaw: wanted.yaw, elevation: 0.0, fov: CEILING_FOV, reach: 1.0 };
    }
    let next = step(current, wanted, dt);
    // What stands on the boom lifts the camera over it at once, so the
    // camera never enters it; it settles back as the step eases it toward
    // the wanted pose.
    let next = if diagnostics.camera_closeup { next } else { lifted(next, eye, tilt, &map, origin) };

    // An obstruction no elevation clears shortens the boom: the camera
    // stands just short. The rig's slide over the shoulder follows the
    // reach the camera has, from the last frame's clearance.
    let stand = next.transform(eye, tilt);
    let shift = next.shift(next.boom() * state.clearance);
    let (head, foot) = (eye + shift, stand.translation + shift);
    state.clearance = clearance_step(state.clearance, boom_clearance(head, foot, &map, origin), dt);
    let translation = head + (foot - head) * state.clearance;

    state.pose = next;
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = next.fov;
    }
    *c_transform = Transform { translation, ..stand };
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::systems::movement::surface_y;
    use qrz::{Convert, Qrz};

    const WIDE: f32 = 2.0;

    /// Among trees the camera stands near, slid over the shoulder, its
    /// frame's top under the horizon.
    #[test]
    fn the_close_pose_stands_near_over_the_shoulder() {
        for yaw in [0.0, 1.0, 2.5, 4.0] {
            let close = Pose::close(yaw);
            assert!(close.boom() < 0.2 * Pose::rest(yaw).boom(), "boom {}", close.boom());
            assert!(close.shift(close.boom()).length() > 0.5 * SHOULDER_WU, "not over the shoulder");
            assert!(close.pitch(Vec2::ZERO) - close.fov / 2.0 > 0.0, "top ray above the horizon");
        }
    }

    /// Rising ground ahead opens the lens until the frame's top clears
    /// its sightline; level or falling ground leaves the lens alone.
    #[test]
    fn the_lens_opens_to_hold_a_rise_in_the_frame() {
        let rest = Pose::rest(0.0);
        let up = 20_f32.to_radians();
        let fov = lens_to_hold(&rest, Vec2::ZERO, up);
        assert!(fov > rest.fov && fov <= HILL_FOV, "{fov}");
        let held = Pose { fov, ..rest };
        let top = held.pitch(Vec2::ZERO) - held.fov / 2.0;
        assert!(top <= -up + 1e-4 || fov == HILL_FOV, "top {top} above the slope's sightline {}", -up);
        assert_eq!(lens_to_hold(&rest, Vec2::ZERO, 0.0), rest.fov);
    }

    /// A ray from the eye over flat ground flies free; one cast at a wall
    /// of tiles meets it, and the sensor reads the wall as a climb.
    #[test]
    fn rays_meet_a_wall_and_fly_free_over_a_plain() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -220..=220 {
            for r in -220..=220 {
                // A wall of 80 levels across the north (negative z) half, some tiles out.
                let wall = r < -12;
                map.insert(Qrz { q, r, z: if wall { 80 } else { 0 } }, ground);
            }
        }
        let feet = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let plain = climb_ahead(feet, Heading::from_degrees(180.0), 0.5, &map, &RenderOrigin::default()).expect("loaded");
        assert_eq!(plain, 0.0);
        let wall = climb_ahead(feet, Heading::NORTH, 0.5, &map, &RenderOrigin::default()).expect("loaded");
        assert!(wall > 0.0, "climb {wall}");
    }

    /// On a slope the plane the camera sweeps on tilts with the ground, so
    /// the camera stands lower behind a player facing up it and higher
    /// behind one facing down it; on the level it is neither.
    #[test]
    fn the_sweep_plane_tilts_with_the_ground() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -60..=60 {
            for r in -60..=60 {
                // A slope rising one level per tile toward -z.
                map.insert(Qrz { q, r, z: -(r + q / 2) }, ground);
            }
        }
        let feet = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let tilt = ground_tilt(feet, TILT_RINGS_WU, &map, &RenderOrigin::default()).expect("loaded");
        assert!(tilt.y < -0.2 && tilt.x.abs() < 0.1, "rises toward -z: {tilt:?}");
        let rest = Pose::rest(0.0);
        let facing_up = Pose { yaw: 0.0, ..rest };
        let facing_down = Pose { yaw: std::f32::consts::PI, ..rest };
        assert!(facing_up.offset(tilt).y < rest.offset(Vec2::ZERO).y, "camera behind a climber stands low");
        assert!(facing_down.offset(tilt).y > rest.offset(Vec2::ZERO).y, "camera behind a descender stands high");
        let reach = |p: &Pose| p.offset(tilt).length();
        assert!(reach(&facing_up) < reach(&facing_down), "an ellipse: nearer looking up, farther looking down");
        assert!(facing_up.pitch(tilt) < facing_down.pitch(tilt), "the frame looks up the slope, then down it");
    }

    /// A climb ahead brings the hill pose down until its crest stands its
    /// rise above the frame's centre, under the top: a steeper climb, a
    /// lower pose, never above the ceiling and never below the eye, the
    /// boom at its full length and the lens opening for what the elevation
    /// cannot reach; and on a slope the plane has already brought the
    /// camera down, so the same climb asks less.
    #[test]
    fn the_hill_pose_holds_the_climb() {
        let mut last = Pose::ceiling(0.0).elevation;
        for up in [5, 15, 25, 30] {
            let up = (up as f32).to_radians();
            let hill = Pose::hill(0.0, up, Vec2::ZERO);
            assert!(hill.elevation <= last && hill.elevation >= 0.0, "{} for a climb of {up}", hill.elevation);
            assert_eq!(hill.boom(), CAMERA_DISTANCE);
            let above_centre = hill.pitch(Vec2::ZERO) + up;
            let placed = (above_centre - CREST_RISE * hill.fov / 2.0).abs() < 1e-4 && above_centre < hill.fov / 2.0;
            let fov = lens_to_hold(&hill, Vec2::ZERO, up);
            let held = Pose { fov, ..hill };
            let top = held.pitch(Vec2::ZERO) - held.fov / 2.0;
            assert!(placed || top <= -up + 1e-4, "crest {above_centre} above the centre, top {top}, for a climb of {up}");
            last = hill.elevation;
        }
        let up = 25_f32.to_radians();
        let rising_ahead = Vec2::new(0.0, -0.4);
        assert!(Pose::hill(0.0, up, rising_ahead).elevation > Pose::hill(0.0, up, Vec2::ZERO).elevation, "the plane does part of the work");
    }

    /// Facing down ground that rises behind the player more steeply than
    /// the plane follows, the hillside stands on the open pose's boom: the
    /// camera is lifted over it, its boom at its full length and its lens
    /// untouched, until it stands its height over the ground and no higher.
    #[test]
    fn a_hillside_behind_angles_the_camera_up() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -60..=60 {
            for r in -60..=60 {
                // Two levels a tile toward +z, behind a player facing north.
                map.insert(Qrz { q, r, z: 2 * (r + q / 2) }, ground);
            }
        }
        let feet = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let eye = feet + Vec3::Y * EYE_HEIGHT;
        let tilt = ground_tilt(feet, TILT_RINGS_WU, &map, &RenderOrigin::default()).expect("loaded");
        let clear = |p: &Pose| boom_is_clear(p, eye, tilt, &map, &RenderOrigin::default());
        let open = Pose::ceiling(0.0);
        assert!(!clear(&open), "the hillside stands on the open pose's boom");
        let up = lifted(open, eye, tilt, &map, &RenderOrigin::default());
        assert!(clear(&up) && up.elevation > open.elevation, "{up:?}");
        assert!(up.fov == open.fov && up.boom() == CAMERA_DISTANCE, "{up:?}");
        let camera = up.transform(eye, tilt).translation;
        let under: Qrz = map.convert(camera);
        let (floor, _) = map.get_by_qr(under.q, under.r).expect("loaded");
        let height = camera.y - surface_y(camera.xz(), floor, &map);
        assert!(height >= STAND_HEIGHT - 1e-3, "stands {height} over the ground");
        let lower = Pose { elevation: up.elevation - 1_f32.to_radians(), ..up };
        assert!(!clear(&lower), "lifted past the edge: {up:?}");
        assert_eq!(lifted(up, eye, tilt, &map, &RenderOrigin::default()), up, "a clear boom is left alone");
    }

    /// The camera reads the ground through the origin's tile, so the same
    /// hillside at the world's origin and millions of tiles out gives the
    /// same tilt, climb, lift and clearance, bit for bit.
    #[test]
    fn the_camera_reads_the_ground_the_same_however_far_out() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let read = |at: Qrz| {
            let map = Map::new(qrz::Map::new(1.0, 0.8));
            let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
            for q in -60..=60 {
                for r in -60..=60 {
                    map.insert(Qrz { q: at.q + q, r: at.r + r, z: 2 * (r + q / 2) }, ground);
                }
            }
            let mut origin = RenderOrigin::default();
            origin.rebase(&map, at);
            let feet = origin.render_tile(&map, Qrz { z: 1, ..at });
            let eye = feet + Vec3::Y * EYE_HEIGHT;
            let tilt = ground_tilt(feet, TILT_RINGS_WU, &map, &origin).expect("loaded");
            let climb = climb_ahead(feet, Heading::NORTH, 0.5, &map, &origin);
            let up = lifted(Pose::ceiling(0.0), eye, tilt, &map, &origin);
            let clear = boom_clearance(eye, up.transform(eye, tilt).translation, &map, &origin);
            (tilt, climb, up, clear)
        };
        assert_eq!(read(Qrz { q: 0, r: 0, z: 0 }), read(Qrz { q: -1_600_000, r: 2_400_000, z: 0 }));
    }

    /// A stand the pose asks for is one the boom allows: on level ground
    /// nothing obstructs the boom at the lowest pose, level with the eye,
    /// so the camera stands where the pose put it, not pulled up the boom
    /// toward the head.
    #[test]
    fn the_lowest_stand_is_clear_on_level_ground() {
        use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -30..=30 {
            for r in -30..=30 {
                map.insert(Qrz { q, r, z: 0 }, ground);
            }
        }
        let feet = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let eye = feet + Vec3::Y * EYE_HEIGHT;
        for yaw in [0.0, 1.0, 2.5] {
            let low = Pose { elevation: 0.0, ..Pose::rest(yaw) };
            let (stand, shift) = (low.transform(eye, Vec2::ZERO), low.shift(low.boom()));
            let clear = boom_clearance(eye + shift, stand.translation + shift, &map, &RenderOrigin::default());
            assert_eq!(clear, 1.0, "the boom at yaw {yaw} is obstructed at {clear}");
        }
    }

    /// Where the player's eye lands in the frame, as fractions of the
    /// half-width and half-height, right and up positive.
    fn eye_in_frame(pose: &Pose, tilt: Tilt) -> Vec2 {
        let eye = Vec3::new(3.0, 20.0, -7.0);
        let mut camera = pose.transform(eye, tilt);
        camera.translation += pose.shift(pose.boom());
        let p = camera.compute_affine().inverse().transform_point3(eye);
        let half_v = (pose.fov / 2.0).tan();
        Vec2::new(p.x / -p.z / (half_v * WIDE), p.y / -p.z / half_v)
    }

    /// The slide over the shoulder follows the reach the camera has, not
    /// the boom the pose asked for: nothing at full reach, more the
    /// shorter it is, so an obstruction that draws the camera in brings
    /// the shoulder with it.
    #[test]
    fn the_shoulder_follows_the_reach() {
        let pose = Pose::rest(0.4);
        assert_eq!(pose.shift(CAMERA_DISTANCE).length(), 0.0);
        let (near, far) = (pose.shift(4.0).length(), pose.shift(30.0).length());
        assert!(near > far && far > 0.0, "{near} at 4, {far} at 30");
    }

    /// An obstruction shortens the boom at once, so the camera never
    /// enters what stands on it; the boom lets back out eased.
    #[test]
    fn an_obstruction_shortens_the_boom_at_once() {
        let dt = 1.0 / 60.0;
        assert_eq!(clearance_step(1.0, 0.3, dt), 0.3);
        let out = clearance_step(0.3, 1.0, dt);
        assert!(out > 0.3 && out < 1.0, "eased out to {out}");
        assert!(clearance_step(0.9, 0.3, dt) <= 0.3);
    }

    /// The shoulder moves the player across the frame, not down it: at
    /// full boom the eye is on the frame's centre line, a third of the way
    /// down; drawn in, it is left of it, at the same height.
    #[test]
    fn the_shoulder_moves_the_player_across_the_frame() {
        let level = Pose { elevation: 0.0, ..Pose::rest(1.1) };
        let at_full = eye_in_frame(&level, Vec2::ZERO);
        assert!(at_full.x.abs() < 1e-4 && (at_full.y + PLAYER_DROP).abs() < 0.01, "{at_full:?}");
        let near = Pose { reach: CLOSE_REACH, ..level };
        let drawn_in = eye_in_frame(&near, Vec2::ZERO);
        assert!(drawn_in.x < -0.1, "left of centre: {drawn_in:?}");
        assert!((drawn_in.y - at_full.y).abs() < 0.02, "same height: {drawn_in:?} vs {at_full:?}");
    }
}
