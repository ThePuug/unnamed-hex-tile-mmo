use bevy::prelude::*;

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
        Transform::default(),
        Offset { state: Vec3::new(0., 10., 20.), step: Vec3::ZERO },
        Actor
    ));
}

pub fn update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Projection, &mut Transform, &Offset), With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
) {
    if let Ok(a_transform) = actor.get_single() {
        if let Ok((c_projection, mut c_transform, c_offset)) = camera.get_single_mut() {
            const MIN: f32 = 6_f32.to_radians();
            const MAX: f32 = 60_f32.to_radians();
            let Projection::Perspective(c_perspective) = c_projection.into_inner() 
            else { return; };
            if keyboard.any_pressed([KeyCode::Minus]) { 
                c_perspective.fov = (c_perspective.fov * 1.01).clamp(MIN, MAX); 
            }
            if keyboard.any_pressed([KeyCode::Equal]) { 
                c_perspective.fov = (c_perspective.fov / 1.01).clamp(MIN, MAX);
            }
            c_transform.translation = a_transform.translation + c_offset.state;
            c_transform.look_at(a_transform.translation + Vec3::Y * map.radius(), Vec3::Y);
        }
    }
}