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
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut Offset), With<Camera3d>>,
    actor: Query<&Transform, (With<Actor>, Without<Camera3d>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        if let Ok((mut c_transform, mut c_offset)) = camera.get_single_mut() {
            const MIN: Vec3 = Vec3::new(0., 1.5, -3000.); 
            const MAX: Vec3 = Vec3::new(0., 1500., -3.);
            if keyboard.any_pressed([KeyCode::Minus]) { c_offset.state = (c_offset.state * 1.05).clamp(MIN, MAX); }
            if keyboard.any_pressed([KeyCode::Equal]) { c_offset.state = (c_offset.state / 1.05).clamp(MIN, MAX); }
            c_transform.translation = a_transform.translation + c_offset.state;
            c_transform.look_at(a_transform.translation + Vec3::Y * TILE_SIZE * 0.75, Vec3::Y);
        }
    }
}

pub fn do_input(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    mut query: Query<(&Hx, &Heading, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
    buffer: Res<InputQueue>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, key_bits, dt, .. } } = message {
            let (&hx, &heading, mut offset, mut air_time) = query.get_mut(ent).unwrap();
            (offset.state, air_time.state) = apply(key_bits, dt as i16, hx, heading, offset.state, air_time.state, &map);
            offset.step = offset.state;
            air_time.step = air_time.state;
            for &event in buffer.queue.iter().rev() { writer.send(Try { event }); }
        }
    }
}

pub fn try_input(
    mut reader: EventReader<Try>,
    mut query: Query<(&Hx, &Heading, &mut Offset, &mut AirTime)>,    
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Input { ent, key_bits, dt, .. } } = message {
            if let Ok((&hx, &heading, mut offset, mut air_time)) = query.get_mut(ent) {
                (offset.step, air_time.step) = apply(key_bits, dt as i16, hx, heading, offset.step, air_time.step, &map);
            }
        }
    }
}

pub fn update_sun(
    time: Res<Time>,
    mut q_sun: Query<(&mut DirectionalLight, &mut Transform, &mut GameTime)>,
) {
    let (mut light, mut transform, mut time_of_day) = q_sun.single_mut();
    time_of_day.0 = (time.elapsed().as_millis() % 20_000) as f32 / 20_000.;
    let time_ratio = ((time_of_day.0-0.5) * 2.5).clamp(-1.,1.);
    light.color = Color::linear_rgb(1., 1.-time_ratio.abs(), 1.-time_ratio.abs());
    light.illuminance = 10_000.*(1.-time_ratio.abs());
    transform.translation.x = 10_000.*cos((0.5-time_ratio / 2.) * PI);
    transform.translation.y = 10_000.*sin((0.5-time_ratio / 2.) * PI);
    transform.look_at(Vec3::ZERO, Vec3::Y);
}

pub fn generate_input(
    mut writer: EventWriter<Try>,
    query: Query<(Entity, &KeyBits), With<Actor>>,
    time: Res<Time>,
) {
    for (ent, &key_bits) in query.iter() {
        let dt = (time.delta_secs() * 1000.) as u16;
        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq: 0 } });
    }
}
