use bevy::{core_pipeline::prepass::DepthPrepass, prelude::*};
use crate::systems::closeup::CloseupCamera;
use std::f32::consts::PI;

use crate::plugins::vignette::VignetteSettings;
use common_bevy::{
    components::*,
    resources::map::Map,
};

/// Orbit stops: one per heading, so the camera can stand behind any.
pub const ORBIT_STOPS: usize = HEADING_SLOTS as usize;
/// Angular separation between orbit stops.
const ORBIT_STEP: f32 = 2.0 * PI / ORBIT_STOPS as f32;
/// Seconds between steps while a flyover turn key is held.
const ORBIT_REPEAT_SECS: f32 = 0.08;
/// Exponential decay constant for orbit interpolation (~0.25s to settle)
const INTERPOLATION_SPEED: f32 = 12.0;
/// Threshold below which interpolation snaps to target
const SNAP_THRESHOLD: f32 = 0.005;

use common_bevy::components::heading::{Heading, HEADING_SLOTS};

// Re-export canonical camera constants from common.
pub use common::camera::{CAMERA_DISTANCE, MAX_GAMEPLAY_FOV, camera_height};

/// Default vertical field of view (narrow telephoto for isometric feel).
const DEFAULT_FOV: f32 = 15_f32.to_radians();
/// Maximum FOV for flyover mode (admin).
pub const MAX_FLYOVER_FOV: f32 = 90_f32.to_radians();

/// Camera height for normal gameplay (convenience alias).
pub fn gameplay_camera_height() -> f32 {
    camera_height(MAX_GAMEPLAY_FOV)
}

/// Camera orbit state: discrete stops, one per heading, and smooth
/// interpolation. In gameplay the target stop follows the player's heading;
/// flyover steps it by key.
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

    commands.spawn((
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: DEFAULT_FOV,
            near: 1.0,
            far: 10000.0,
            ..default()
        }),
        Transform::default(),
        Actor,
        VignetteSettings::default(),
        // Depth first, so the terrain's fragment shader runs once per pixel
        // that shows and never for one another tile covers.
        DepthPrepass,
    ));
}

pub fn update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    mut camera: Query<(&mut Projection, &mut Transform), (With<Camera3d>, Without<CloseupCamera>)>,
    actor: Query<(&Transform, &Heading), (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
    time: Res<Time>,
) {
    // The orbit follows the predicted heading; no key moves the camera.
    if let Ok((_, heading)) = actor.single() {
        orbit.follow(*heading);
    }

    // Smooth interpolation toward target
    let target = orbit.target_angle();
    let diff = angle_diff(orbit.current, target);
    if diff.abs() > SNAP_THRESHOLD {
        orbit.current += diff * (1.0 - (-INTERPOLATION_SPEED * time.delta_secs()).exp());
        orbit.current = orbit.current.rem_euclid(2.0 * PI);
    } else {
        orbit.current = target;
    }

    if let Ok((a_transform, _)) = actor.single() {
        if let Ok((c_projection, mut c_transform)) = camera.single_mut() {
            // Zoom controls (perspective FOV)
            if let Projection::Perspective(c_perspective) = c_projection.into_inner() {
                const MIN: f32 = 6_f32.to_radians();
                if keyboard.any_pressed([KeyCode::Minus]) {
                    c_perspective.fov = (c_perspective.fov * 1.01).clamp(MIN, MAX_GAMEPLAY_FOV);
                }
                if keyboard.any_pressed([KeyCode::Equal]) {
                    c_perspective.fov = (c_perspective.fov / 1.01).clamp(MIN, MAX_GAMEPLAY_FOV);
                }
            }

            // Calculate camera offset from orbit angle
            let height = gameplay_camera_height();
            let offset = Vec3::new(
                orbit.current.sin() * CAMERA_DISTANCE,
                height,
                orbit.current.cos() * CAMERA_DISTANCE,
            );

            c_transform.translation = a_transform.translation + offset;
            c_transform.look_at(a_transform.translation + Vec3::Y * map.radius(), Vec3::Y);
        }
    }
}
