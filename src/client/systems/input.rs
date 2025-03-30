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

// TODO: start fall on first monday, end month with summer, add drought season once per quarter
// TODO: shift day/night cycle by 12 minutes every day
const DAY_MS: u128 = 14400_000;         // 4 hour real time = 1 day game time
const WEEK_MS: u128 = DAY_MS*6;         // 1 day real time = 6-day week game time
const SEASON_MS: u128 = WEEK_MS*7;      // 1 week real time = 7-week season game time
const YEAR_MS: u128 = SEASON_MS*4;      // ~1 month (4w) real time = 4-season year game time, 
                                        
pub fn update_sun(
    time: Res<Time>,
    mut q_sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>,Without<Moon>)>,
    mut q_moon: Query<(&mut DirectionalLight, &mut Transform), (With<Moon>,Without<Sun>)>,
    mut a_light: ResMut<AmbientLight>,
    server: Res<Server>,
) {
    let dt = time.elapsed().as_millis() + server.elapsed_offset;
    let dtd = (dt % DAY_MS) as f32 / DAY_MS as f32;
    let dtm = (dt % SEASON_MS) as f32 / SEASON_MS as f32;
    let dty = (dt % YEAR_MS) as f32 / YEAR_MS as f32;

    // sun
    let (mut s_light, mut s_transform) = q_sun.single_mut();
    let mut s_rad_d = dtd * 2. * PI;
    let s_rad_y = dty * 2. * PI;

    // days are longer than nights
    s_rad_d = s_rad_d.clamp(0., 4.*PI/3.);

    let s_illuminance = 1.-cos(0.75*s_rad_d).powf(8.);
    s_light.color = Color::linear_rgb(1., s_illuminance, s_illuminance);
    s_light.illuminance = 10_000.*s_illuminance;
    a_light.brightness = 100.*s_illuminance;
    s_transform.translation.x = 1_000.*cos(0.75*s_rad_d);
    s_transform.translation.y = 1_000.*sin(0.75*s_rad_d).powf(2.);
    s_transform.translation.z = 1_000.*cos(s_rad_y);
    s_transform.look_at(Vec3::ZERO, Vec3::Y);

    // moon
    let (mut m_light, mut m_transform) = q_moon.single_mut();
    let mut m_rad_d = dtd * 2. * PI;
    let m_rad_m = dtm * 2. * PI;

    // overlap sun cycle by PI/6 to avoid no lightsource at dusk/dawn
    if PI/6. < m_rad_d && m_rad_d < 7.*PI/6. { m_rad_d = 7.*PI/6. };

    m_light.illuminance = 300.                  // max illuminance at full moon
        *(0.1+0.9*cos(0.5*m_rad_m).powf(2.))    // phase moon through month
        *(1.-cos(m_rad_d+5.*PI/6.).powf(8.));   // moon rise/fall
    m_transform.translation.x = 1_000.*cos(m_rad_d+5.*PI/6.);
    m_transform.translation.y = 1_000.*sin(m_rad_d+5.*PI/6.).powf(2.);
    m_transform.look_at(Vec3::ZERO, Vec3::Y);
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
