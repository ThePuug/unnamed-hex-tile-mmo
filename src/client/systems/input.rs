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

pub fn handle_input(
    mut writer: EventWriter<Try>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &mut Heading, &mut KeyBits), With<Actor>>,
) {
    if let Ok((ent, mut heading0, mut keybits0)) = query.get_single_mut() {
        let mut key_bits = KeyBits::default();
        if keyboard.any_just_pressed([KeyCode::Space]) { key_bits |= KB_JUMP; }

        if keyboard.any_pressed([KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight,]) {
            if keyboard.any_pressed([KeyCode::ArrowUp]) {
                if keyboard.any_pressed([KeyCode::ArrowLeft]) || !keyboard.any_pressed([KeyCode::ArrowRight])
                    &&(heading0.0 == Hx {q:-1, r: 0, z: 0}
                    || heading0.0 == Hx {q:-1, r: 1, z: 0}
                    || heading0.0 == Hx {q: 1, r:-1, z: 0}) { 
                        heading0.0 = Hx {q:-1, r: 1, z: 0}; 
                        key_bits |= KB_HEADING_Q | KB_HEADING_R;
                    }                    
                else  { 
                    heading0.0 = Hx {q: 0, r: 1, z: 0};
                    key_bits |= KB_HEADING_R
                }
            } else if keyboard.any_pressed([KeyCode::ArrowDown]) {
                if keyboard.any_pressed([KeyCode::ArrowRight]) || !keyboard.any_pressed([KeyCode::ArrowLeft])
                    &&(heading0.0 == Hx {q: 1, r: 0, z: 0}
                    || heading0.0 == Hx {q: 1, r:-1, z: 0}
                    || heading0.0 == Hx {q:-1, r: 1, z: 0}) { 
                        heading0.0 = Hx {q: 1, r: -1, z: 0};
                        key_bits |= KB_HEADING_Q | KB_HEADING_R | KB_HEADING_NEG; 
                    }
                else { 
                    heading0.0 = Hx {q: 0, r:-1, z: 0};
                    key_bits |= KB_HEADING_R | KB_HEADING_NEG;
                }
            } 
            else if keyboard.any_pressed([KeyCode::ArrowRight]) { 
                heading0.0 = Hx {q: 1, r: 0, z: 0}; 
                key_bits |= KB_HEADING_Q
            } else if keyboard.any_pressed([KeyCode::ArrowLeft]) { 
                heading0.0 = Hx {q:-1, r: 0, z: 0}; 
                key_bits |= KB_HEADING_Q | KB_HEADING_NEG;
            }
        }

        if *keybits0 != key_bits {
            *keybits0 = key_bits;
            writer.send(Try { event: Event::Input { ent, key_bits } });
        }
    }
}

pub fn camera(
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

