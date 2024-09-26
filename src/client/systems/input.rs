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

pub fn handle_input(
    mut writer: EventWriter<Try>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &mut Heading, &mut KeyBits), With<Actor>>,
) {
    keyboard.get_just_pressed().for_each(|key| { trace!("key pressed: {:?}", key) });
    if let Ok((ent, mut heading0, mut keybits0)) = query.get_single_mut() {
        let mut key_bits = KeyBits::default();
        if keyboard.any_just_pressed(KEYCODES_JUMP) { key_bits |= KB_JUMP; }

        let mut heading = *heading0;
        if keyboard.any_pressed([KEYCODES_UP, KEYCODES_DOWN, KEYCODES_LEFT, KEYCODES_RIGHT].concat()) {
            if keyboard.any_pressed(KEYCODES_UP) {
                if keyboard.any_pressed(KEYCODES_LEFT) || !keyboard.any_pressed(KEYCODES_RIGHT)
                    &&(heading.0 == Hx {q:-1, r: 0, z: 0}
                    || heading.0 == Hx {q:-1, r: 1, z: 0}
                    || heading.0 == Hx {q: 1, r:-1, z: 0}) { 
                        heading.0 = Hx {q:-1, r: 1, z: 0}; 
                        key_bits |= KB_HEADING_Q | KB_HEADING_R;
                    }
                else  { 
                    heading.0 = Hx {q: 0, r: 1, z: 0};
                    key_bits |= KB_HEADING_R
                }
            } else if keyboard.any_pressed(KEYCODES_DOWN) {
                if keyboard.any_pressed(KEYCODES_RIGHT) || !keyboard.any_pressed(KEYCODES_LEFT)
                    &&(heading.0 == Hx {q: 1, r: 0, z: 0}
                    || heading.0 == Hx {q: 1, r:-1, z: 0}
                    || heading.0 == Hx {q:-1, r: 1, z: 0}) { 
                        heading.0 = Hx {q: 1, r: -1, z: 0};
                        key_bits |= KB_HEADING_Q | KB_HEADING_R | KB_HEADING_NEG; 
                    }
                else { 
                    heading.0 = Hx {q: 0, r:-1, z: 0};
                    key_bits |= KB_HEADING_R | KB_HEADING_NEG;
                }
            } 
            else if keyboard.any_pressed(KEYCODES_RIGHT) { 
                heading.0 = Hx {q: 1, r: 0, z: 0}; 
                key_bits |= KB_HEADING_Q
            } else if keyboard.any_pressed(KEYCODES_LEFT) {
                heading.0 = Hx {q:-1, r: 0, z: 0}; 
                key_bits |= KB_HEADING_Q | KB_HEADING_NEG;
            }
        }

        if *heading0 != heading { *heading0 = heading; }

        if *keybits0 != key_bits {
            *keybits0 = key_bits;
            writer.send(Try { event: Event::Input { ent, key_bits } });
        }
    }
}

pub fn update_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), (With<Actor>, Without<Hx>, Without<Offset>)>,
    actor: Query<&Transform, (With<Actor>, With<Hx>, With<Offset>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        let (mut c_transform, mut projection) = camera.single_mut();
        c_transform.translation = a_transform.translation + Vec3 { x: 0., y: 24., z: 0. };
        if keyboard.any_pressed([KeyCode::Minus]) { projection.scale *= 1.05; }
        if keyboard.any_pressed([KeyCode::Equal]) { projection.scale /= 1.05; }
    }
}

