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
        Offset { state: Vec3::new(0., 10., -20.), step: Vec3::ZERO },
        Actor
    ));
}

pub fn update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut Offset), With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
    map: Res<Map>,
) {
    if let Ok(a_transform) = actor.get_single() {
        if let Ok((mut c_transform, mut c_offset)) = camera.get_single_mut() {
            const MIN: Vec3 = Vec3::new(0., 1.5, -100.); 
            const MAX: Vec3 = Vec3::new(0., 50., -3.);
            if keyboard.any_pressed([KeyCode::Minus]) { c_offset.state = (c_offset.state * 1.05).clamp(MIN, MAX); }
            if keyboard.any_pressed([KeyCode::Equal]) { c_offset.state = (c_offset.state / 1.05).clamp(MIN, MAX); }
            c_transform.translation = a_transform.translation + c_offset.state;
            c_transform.look_at(a_transform.translation + Vec3::Y * map.radius() * 0.75, Vec3::Y);
        }
    }
}