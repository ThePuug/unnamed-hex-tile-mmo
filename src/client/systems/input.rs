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
    }
};

pub const KEYCODES_JUMP: [KeyCode; 1] = [KeyCode::Space];
pub const KEYCODES_UP: [KeyCode; 2] = [KeyCode::ArrowUp, KeyCode::Lang3];
pub const KEYCODES_DOWN: [KeyCode; 2] = [KeyCode::ArrowDown, KeyCode::NumpadEnter];
pub const KEYCODES_LEFT: [KeyCode; 2] = [KeyCode::ArrowLeft, KeyCode::Convert];
pub const KEYCODES_RIGHT: [KeyCode; 2] = [KeyCode::ArrowRight, KeyCode::NonConvert];

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Heading, &mut KeyBits), With<Actor>>,
) {
    if let Ok((&heading, mut keybits0)) = query.get_single_mut() {
        let mut key_bits = KeyBits::default();
        key_bits.set_pressed([KB_JUMP], keyboard.any_just_pressed(KEYCODES_JUMP));

        if keyboard.any_pressed([KEYCODES_UP, KEYCODES_DOWN, KEYCODES_LEFT, KEYCODES_RIGHT].concat()) {
            if keyboard.any_pressed(KEYCODES_UP) {
                if keyboard.any_pressed(KEYCODES_LEFT) || !keyboard.any_pressed(KEYCODES_RIGHT)
                    &&(heading.0 == Hx {q:-1, r: 0, z: 0}
                    || heading.0 == Hx {q:-1, r: 1, z: 0}
                    || heading.0 == Hx {q: 1, r:-1, z: 0}) {
                        key_bits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true);
                    }
                else  {
                    key_bits.set_pressed([KB_HEADING_R], true);
                }
            } else if keyboard.any_pressed(KEYCODES_DOWN) {
                if keyboard.any_pressed(KEYCODES_RIGHT) || !keyboard.any_pressed(KEYCODES_LEFT)
                    &&(heading.0 == Hx {q: 1, r: 0, z: 0}
                    || heading.0 == Hx {q: 1, r:-1, z: 0}
                    || heading.0 == Hx {q:-1, r: 1, z: 0}) {
                        key_bits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true); 
                    }
                else {
                    key_bits.set_pressed([KB_HEADING_R, KB_HEADING_NEG], true);
                }
            } 
            else if keyboard.any_pressed(KEYCODES_RIGHT) { 
                key_bits.set_pressed([KB_HEADING_Q], true);
            } else if keyboard.any_pressed(KEYCODES_LEFT) {
                key_bits.set_pressed([KB_HEADING_Q, KB_HEADING_NEG], true);
            }
        }

        if *keybits0 != key_bits { *keybits0 = key_bits; }
    }
}

pub fn update_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), With<Actor>>,
    actor: Query<&Transform, (With<Actor>, Without<OrthographicProjection>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        let (mut c_transform, mut projection) = camera.single_mut();
        c_transform.translation = a_transform.translation + Vec3 { x: 0., y: 24., z: 0. };
        if keyboard.any_pressed([KeyCode::Minus]) { projection.scale *= 1.05; }
        if keyboard.any_pressed([KeyCode::Equal]) { projection.scale /= 1.05; }
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
        match message {
            Do { event: Event::Input { ent, key_bits, dt, .. } } => {
                let (&heading, &hx, mut offset, mut air_time) = query.get_mut(ent).unwrap();
                (offset.state, air_time.state) = apply(key_bits, dt as i16, &heading, &hx, offset.state, air_time.state, &map);
                offset.step = offset.state;
                air_time.step = air_time.state;
                for &it in queue.0.iter().rev() {
                    match it {
                        Event::Input { key_bits, dt, seq, .. } => {
                            writer.send(Try { event: Event::Input { ent, key_bits, dt, seq } });
                        }
                        _ => unreachable!()
                    }
                }
            }, 
            _ => {}
        }
    }
}

pub fn try_input(
    mut reader: EventReader<Try>,
    mut query: Query<(&Heading, &Hx, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits, dt, .. } } => {
                let (&heading, &hx, mut offset, mut air_time) = query.get_mut(ent).unwrap();
                (offset.step, air_time.step) = apply(key_bits, dt as i16, &heading, &hx, offset.step, air_time.step, &map);
            }, 
            _ => {}
        }
    }
}

pub fn generate_input(
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    query: Query<(Entity, &KeyBits), With<Actor>>,
) {
    for (ent, &key_bits) in query.iter() {
        let dt = (time.delta_seconds() * 1000.) as u16;
        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq: 0 } });
    }
}
