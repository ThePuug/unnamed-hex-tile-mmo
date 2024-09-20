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

        *key_bits0 = key_bits;
        let dt_f32 = time.delta_seconds() * 1000.;
        let dt = if dt_f32 > u8::MAX as f32 { 255 } else { dt_f32 as u8 };
        writer.send(Event::Input { ent, key_bits, dt });
    }
}

pub fn camera(
    mut camera: Query<&mut Transform, (With<Actor>, With<OrthographicProjection>, Without<Pos>)>,
    actor: Query<&Transform, (With<Pos>, With<Actor>)>,
) {
    if let Ok(transform) = actor.get_single() {
        camera.single_mut().translation = transform.translation;
    }
}