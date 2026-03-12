use bevy::prelude::*;
use std::f32::consts::PI;

use crate::plugins::vignette::VignetteSettings;
use common_bevy::{
    components::*,
    resources::map::Map,
};

/// Number of discrete orbit positions
const ORBIT_STOPS: usize = 6;
/// Angular separation between orbit stops (60°)
const ORBIT_STEP: f32 = PI / 3.0;
/// Exponential decay constant for orbit interpolation (~0.25s to settle)
const INTERPOLATION_SPEED: f32 = 12.0;
/// Threshold below which interpolation snaps to target
const SNAP_THRESHOLD: f32 = 0.005;

/// Camera distance from player (scaled up for narrow FOV perspective)
pub const CAMERA_DISTANCE: f32 = 120.0;
/// Camera height above player (preserves original pitch angle)
pub const CAMERA_HEIGHT: f32 = 90.0;
/// Default vertical field of view (narrow telephoto for isometric feel)
const DEFAULT_FOV: f32 = 15_f32.to_radians();

/// Camera orbit state with discrete 60° stops and smooth interpolation.
#[derive(Resource)]
pub struct CameraOrbit {
    /// Current interpolated angle (radians, 0 = behind player facing north)
    pub current: f32,
    /// Target stop index (0..5, each 60° apart)
    pub target_index: usize,
}

impl Default for CameraOrbit {
    fn default() -> Self {
        Self { current: 0.0, target_index: 0 }
    }
}

impl CameraOrbit {
    pub fn target_angle(&self) -> f32 {
        self.target_index as f32 * ORBIT_STEP
    }

    pub fn is_interpolating(&self) -> bool {
        angle_diff(self.current, self.target_angle()).abs() > SNAP_THRESHOLD
    }

    /// Step clockwise (triggered by Up+Right movement input)
    pub fn step_cw(&mut self) {
        if !self.is_interpolating() {
            self.target_index = (self.target_index + ORBIT_STOPS - 1) % ORBIT_STOPS;
        }
    }

    /// Step counterclockwise (triggered by Up+Left movement input)
    pub fn step_ccw(&mut self) {
        if !self.is_interpolating() {
            self.target_index = (self.target_index + 1) % ORBIT_STOPS;
        }
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
    ));
}

pub fn update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    mut camera: Query<(&mut Projection, &mut Transform), With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
    time: Res<Time>,
) {
    // Camera rotation is driven by movement input (update_keybits in input.rs).
    // No dedicated rotation keys.

    // Smooth interpolation toward target
    let target = orbit.target_angle();
    let diff = angle_diff(orbit.current, target);
    if diff.abs() > SNAP_THRESHOLD {
        orbit.current += diff * (1.0 - (-INTERPOLATION_SPEED * time.delta_secs()).exp());
        orbit.current = orbit.current.rem_euclid(2.0 * PI);
    } else {
        orbit.current = target;
    }

    if let Ok(a_transform) = actor.single() {
        if let Ok((c_projection, mut c_transform)) = camera.single_mut() {
            // Zoom controls
            match c_projection.into_inner() {
                Projection::Perspective(c_perspective) => {
                    const MIN: f32 = 6_f32.to_radians();
                    const MAX: f32 = 60_f32.to_radians();
                    if keyboard.any_pressed([KeyCode::Minus]) {
                        c_perspective.fov = (c_perspective.fov * 1.01).clamp(MIN, MAX);
                    }
                    if keyboard.any_pressed([KeyCode::Equal]) {
                        c_perspective.fov = (c_perspective.fov / 1.01).clamp(MIN, MAX);
                    }
                }
                Projection::Orthographic(c_orthographic) => {
                    const MIN: f32 = 0.08;
                    const MAX: f32 = 2.0;
                    if keyboard.any_pressed([KeyCode::Minus]) {
                        c_orthographic.scale = (c_orthographic.scale * 1.01).clamp(MIN, MAX);
                    }
                    if keyboard.any_pressed([KeyCode::Equal]) {
                        c_orthographic.scale = (c_orthographic.scale / 1.01).clamp(MIN, MAX);
                    }
                }
                _ => {}
            }

            // Calculate camera offset from orbit angle
            let offset = Vec3::new(
                orbit.current.sin() * CAMERA_DISTANCE,
                CAMERA_HEIGHT,
                orbit.current.cos() * CAMERA_DISTANCE,
            );

            c_transform.translation = a_transform.translation + offset;
            c_transform.look_at(a_transform.translation + Vec3::Y * map.radius(), Vec3::Y);
        }
    }
}
