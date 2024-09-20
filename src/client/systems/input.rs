use bevy::prelude::*;

use crate::{*, Event};

pub fn ui_input(
    mut writer: EventWriter<Event>,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &mut KeyBits), With<Actor>>,
) {
    if let Ok((ent, mut key_bits0)) = query.get_single_mut() {        
        let mut key_bits: KeyBits = default();
        if keyboard.any_pressed([KeyCode::ArrowUp, KeyCode::Lang3]) { key_bits |= KEYBIT_UP; }
        if keyboard.any_pressed([KeyCode::ArrowDown, KeyCode::NumpadEnter]) { key_bits |= KEYBIT_DOWN; }
        if keyboard.any_pressed([KeyCode::ArrowLeft, KeyCode::Convert]) { key_bits |= KEYBIT_LEFT; }
        if keyboard.any_pressed([KeyCode::ArrowRight, KeyCode::NonConvert]) { key_bits |= KEYBIT_RIGHT; }
        if keyboard.any_pressed([KeyCode::Minus]) { key_bits |= KEYBIT_ZOOM_OUT; }
        if keyboard.any_pressed([KeyCode::Equal]) { key_bits |= KEYBIT_ZOOM_IN; }

        *key_bits0 = key_bits;
        let dt_f32 = time.delta_seconds() * 1000.;
        let dt = if dt_f32 > u8::MAX as f32 { 255 } else { dt_f32 as u8 };
        writer.send(Event::Input { ent, key_bits, dt });
    }
}

pub fn camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), (With<Actor>, Without<Pos>)>,
    actor: Query<&Transform, (With<Pos>, With<Actor>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        let (mut c_transform, mut projection) = camera.single_mut();
        c_transform.translation = a_transform.translation + Vec3 { x: 0., y: 24., z: 0. };
        if keyboard.any_pressed([KeyCode::Minus]) { projection.scale *= 1.05; }
        if keyboard.any_pressed([KeyCode::Equal]) { projection.scale /= 1.05; }
    }
}
