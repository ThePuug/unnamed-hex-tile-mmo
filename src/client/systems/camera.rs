use bevy::prelude::*;
use bevy_camera::ScalingMode;
use std::f32::consts::PI;

use crate::{
    client::plugins::vignette::VignetteSettings,
    common::{
        components::*,
        resources::map::Map
    }
};

/// Camera orbit angle around the player (radians, 0 = behind player facing north)
#[derive(Resource)]
pub struct CameraOrbitAngle(pub f32);

impl Default for CameraOrbitAngle {
    fn default() -> Self {
        Self(0.0)  // Start behind the player
    }
}

/// Camera distance from player and height
const CAMERA_DISTANCE: f32 = 40.0;
const CAMERA_HEIGHT: f32 = 30.0;

pub fn setup(
    mut commands: Commands,
) {
    // Initialize camera orbit angle resource
    commands.insert_resource(CameraOrbitAngle::default());

    commands.spawn((
        Camera3d::default(),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 40.0 },
            near: -10000.0,  // Extend clipping planes to prevent terrain culling
            far: 10000.0,
            ..OrthographicProjection::default_3d()
        }),
        Transform::default(),
        Actor,
        VignetteSettings::default(), // Add vignette post-processing
    ));
}

pub fn update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit_angle: ResMut<CameraOrbitAngle>,
    mut camera: Query<(&mut Projection, &mut Transform), With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
    time: Res<Time>,
) {
    // Camera orbit controls (Shift + Left/Right)
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if shift_pressed {
        const ORBIT_SPEED: f32 = 2.0;  // radians per second
        if keyboard.pressed(KeyCode::ArrowLeft) {
            orbit_angle.0 += ORBIT_SPEED * time.delta_secs();
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            orbit_angle.0 -= ORBIT_SPEED * time.delta_secs();
        }
        // Keep angle in 0..2π range
        orbit_angle.0 = orbit_angle.0.rem_euclid(2.0 * PI);
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
            // Angle 0 = behind player (south, +Z direction)
            let offset = Vec3::new(
                orbit_angle.0.sin() * CAMERA_DISTANCE,  // X rotates around Y axis
                CAMERA_HEIGHT,                           // Y stays constant (height above player)
                orbit_angle.0.cos() * CAMERA_DISTANCE,  // Z rotates around Y axis
            );

            c_transform.translation = a_transform.translation + offset;
            c_transform.look_at(a_transform.translation + Vec3::Y * map.radius(), Vec3::Y);
        }
    }
}