use bevy::{
    prelude::*, 
    render::camera::ScalingMode
};

use crate::common::{
    components::{ *, 
        offset::*,
    }, 
    resources::map::Map
};

pub fn setup(
    mut commands: Commands,
) {
    commands.spawn((
        Camera3d::default(),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 40.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::default(),
        Offset { state: Vec3::new(0., 30., 40.), ..default() },
        Actor
    ));
}

pub fn update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Projection, &mut Transform, &Offset), With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
) {
    if let Ok(a_transform) = actor.single() {
        if let Ok((c_projection, mut c_transform, c_offset)) = camera.single_mut() {
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
                    const MAX: f32 = 1.;
                    if keyboard.any_pressed([KeyCode::Minus]) {
                        c_orthographic.scale = (c_orthographic.scale * 1.01).clamp(MIN, MAX);
                    }
                    if keyboard.any_pressed([KeyCode::Equal]) {
                        c_orthographic.scale = (c_orthographic.scale / 1.01).clamp(MIN, MAX);
                    }    
                }
                _ => {}
            }
            c_transform.translation = a_transform.translation + c_offset.state;
            c_transform.look_at(a_transform.translation + Vec3::Y * map.radius(), Vec3::Y);
        }
    }
}