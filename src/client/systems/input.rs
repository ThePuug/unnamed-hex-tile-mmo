use bevy::prelude::*;

use crate::{*,
    common::{
        components::{
            hx::*,
            keybits::*,
        },
        message::{*, Event},
    },
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

pub fn generate_input(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    query: Query<(Entity, Option<&AirTime>, &KeyBits), With<Actor>>,
) {
    if let Ok((ent, air_time, &key_bits)) = query.get_single() {
        let dt = (time.delta_seconds() * 1000.) as u16;
        if key_bits.all_pressed([KB_JUMP]) && air_time.is_none() {
            commands.entity(ent).insert(AirTime(500));
        }
        writer.send(Try { event: Event::Input { ent, key_bits, dt } });
    }
}
