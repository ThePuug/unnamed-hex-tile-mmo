use bevy::prelude::*;

use crate::{*,
    common::{
        message::Event,
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        systems::gcd::*,
    }
};

pub const KEYCODE_JUMP: KeyCode = KeyCode::Numpad0;
pub const KEYCODE_UP: KeyCode = KeyCode::ArrowUp;
pub const KEYCODE_DOWN: KeyCode = KeyCode::ArrowDown;
pub const KEYCODE_LEFT: KeyCode = KeyCode::ArrowLeft;
pub const KEYCODE_RIGHT: KeyCode = KeyCode::ArrowRight;

pub const KEYCODE_GCD1: KeyCode = KeyCode::KeyQ;

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &Heading, &mut KeyBits), With<Actor>>,
    mut writer: EventWriter<Try>,
) {
    if let Ok((ent, &heading, mut keybits0)) = query.get_single_mut() {
        if keyboard.just_released(KEYCODE_GCD1) {
            writer.send(Try { event: Event::Gcd { ent, typ: GcdType::Attack} });
        }

        let mut key_bits = KeyBits::default();
        key_bits.set_pressed([KB_JUMP], keyboard.any_just_pressed([KEYCODE_JUMP]));

        if keyboard.any_pressed([KEYCODE_UP, KEYCODE_DOWN, KEYCODE_LEFT, KEYCODE_RIGHT]) {
            if keyboard.pressed(KEYCODE_UP) {
                if keyboard.pressed(KEYCODE_LEFT) || !keyboard.pressed(KEYCODE_RIGHT)
                    &&(heading.0 == Hx {q:-1, r: 0, z: 0}
                    || heading.0 == Hx {q: 0, r:-1, z: 0}
                    || heading.0 == Hx {q: 0, r: 1, z: 0}) {
                        key_bits.set_pressed([KB_HEADING_R, KB_HEADING_NEG], true);
                    }
                else {
                    key_bits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true);
                }
            } else if keyboard.pressed(KEYCODE_DOWN) {
                if keyboard.pressed(KEYCODE_LEFT) || !keyboard.pressed(KEYCODE_RIGHT)
                    &&(heading.0 == Hx {q:-1, r: 0, z: 0}
                    || heading.0 == Hx {q: 1, r:-1, z: 0}
                    || heading.0 == Hx {q:-1, r: 1, z: 0}) {
                        key_bits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true); 
                    }
                else {
                    key_bits.set_pressed([KB_HEADING_R], true);
                }
            } 
            else if keyboard.pressed(KEYCODE_RIGHT) { 
                key_bits.set_pressed([KB_HEADING_Q], true);
            } else if keyboard.pressed(KEYCODE_LEFT) {
                key_bits.set_pressed([KB_HEADING_Q, KB_HEADING_NEG], true);
            }
        }

        if *keybits0 != key_bits { *keybits0 = key_bits; }
    }
}

pub fn update_camera(
    // keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        let mut c_transform = camera.single_mut();
        c_transform.translation = a_transform.translation + Vec3 { x: 0., y: 500., z: -500. };
        // if keyboard.any_pressed([KeyCode::Minus]) { c_transform.translation.z *= 1.05; }
        // if keyboard.any_pressed([KeyCode::Equal]) { c_transform.translation.z /= 1.05; }
    }
}

pub fn do_input(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut query: Query<(&Heading, &Hx, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
    queue: Res<InputQueue>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, key_bits, dt, .. } } = message {
            let (&heading, &hx, mut offset, mut air_time) = query.get_mut(ent).unwrap();
            (offset.state, air_time.state) = apply(key_bits, dt as i16, heading, hx, offset.state, air_time.state, &map);
            offset.step = offset.state;
            air_time.step = air_time.state;
            for it in queue.0.iter().rev() {
                match *it {
                    Event::Input { key_bits, dt, seq, .. } => {
                        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq } });
                    }
                    _ => unreachable!()
                }
            }
        }
    }
}

pub fn try_input(
    mut reader: EventReader<Try>,
    mut query: Query<(&Heading, &Hx, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Input { ent, key_bits, dt, .. } } = message {
            let (&heading, &hx, mut offset, mut air_time) = query.get_mut(ent).unwrap();
            (offset.step, air_time.step) = apply(key_bits, dt as i16, heading, hx, offset.step, air_time.step, &map);
        }
    }
}

pub fn generate_input(
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    query: Query<(Entity, &KeyBits), With<Actor>>,
) {
    for (ent, &key_bits) in query.iter() {
        let dt = (time.delta_secs() * 1000.) as u16;
        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq: 0 } });
    }
}
