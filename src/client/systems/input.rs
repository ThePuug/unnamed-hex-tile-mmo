use bevy::prelude::*;
use qrz::Qrz;

use crate::{common::{
        components::{
            entity_type::{ *,
                actor::*,
            },
            heading::*,
            keybits::*,
        }, 
        message::{Component, Event}, systems::{
            gcd::*,
        },
        resources::*,
    }, *
};

pub const KEYCODE_JUMP: KeyCode = KeyCode::Numpad0;
pub const KEYCODE_UP: KeyCode = KeyCode::ArrowUp;
pub const KEYCODE_DOWN: KeyCode = KeyCode::ArrowDown;
pub const KEYCODE_LEFT: KeyCode = KeyCode::ArrowLeft;
pub const KEYCODE_RIGHT: KeyCode = KeyCode::ArrowRight;

pub const KEYCODE_GCD1: KeyCode = KeyCode::KeyQ;

/// Milliseconds between periodic input sends
pub const INPUT_SEND_INTERVAL_MS: u128 = 1000;

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &Heading, &mut KeyBits), With<Actor>>,
    mut writer: EventWriter<Try>,
    dt: Res<Time>,
) {
    if let Ok((ent, &heading, mut keybits0)) = query.single_mut() {
        keybits0.accumulator += dt.delta().as_nanos();

        if keyboard.just_released(KEYCODE_GCD1) {
            let typ = EntityType::Actor(ActorImpl::new(
                Origin::Fauna, 
                Form::Bestial, 
                Manifestation::Physical));
            writer.write(Try { event: Event::Gcd { ent, typ: GcdType::Spawn(typ)}});
        }

        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_JUMP], keyboard.any_just_pressed([KEYCODE_JUMP]));

        if keyboard.any_pressed([KEYCODE_UP, KEYCODE_DOWN, KEYCODE_LEFT, KEYCODE_RIGHT]) {
            if keyboard.pressed(KEYCODE_UP) {
                if keyboard.pressed(KEYCODE_LEFT) || !keyboard.pressed(KEYCODE_RIGHT)
                    &&(*heading == Qrz {q:-1, r: 0, z: 0}
                    || *heading == Qrz {q: 0, r:-1, z: 0}
                    || *heading == Qrz {q: 0, r: 1, z: 0}) {
                        keybits.set_pressed([KB_HEADING_R, KB_HEADING_NEG], true);
                    }
                else {
                    keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true);
                }
            } else if keyboard.pressed(KEYCODE_DOWN) {
                if keyboard.pressed(KEYCODE_LEFT) || !keyboard.pressed(KEYCODE_RIGHT)
                    &&(*heading == Qrz {q:-1, r: 0, z: 0}
                    || *heading == Qrz {q: 1, r:-1, z: 0}
                    || *heading == Qrz {q:-1, r: 1, z: 0}) {
                        keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true); 
                    }
                else {
                    keybits.set_pressed([KB_HEADING_R], true);
                }
            } 
            else if keyboard.pressed(KEYCODE_RIGHT) { 
                keybits.set_pressed([KB_HEADING_Q], true);
            } else if keyboard.pressed(KEYCODE_LEFT) {
                keybits.set_pressed([KB_HEADING_Q, KB_HEADING_NEG], true);
            }
        }

        // Send input if either keybits changed or periodic interval has elapsed
        if keybits0.key_bits != keybits.key_bits || keybits0.accumulator >= INPUT_SEND_INTERVAL_MS * 1_000_000 {
            *keybits0 = keybits;
            writer.write(Try { event: Event::Incremental { ent, component: Component::KeyBits(keybits) }});
        }
    }
}

pub fn do_input(
    mut reader: EventReader<Do>,
    mut buffers: ResMut<InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Input { ent, key_bits, dt, seq }} = message else { continue };
        let Some(buffer) = buffers.get_mut(&ent) else { 
            warn!("no {ent} in buffers"); 
            continue 
        };
        let Some(Event::Input { ent: ent0, key_bits: keybits0, dt: dt0, seq: seq0 }) = buffer.queue.pop_back() 
            else { 
                continue 
            };
        assert!(ent == ent0);
        assert!(key_bits == keybits0);
        assert!(seq == seq0);
        if (dt as i32 - dt0 as i32).abs() > 100 { warn!("dt: {dt} != {dt0}"); }
        if buffer.queue.len() > 2 { warn!("buffer.queue len: {}", buffer.queue.len()); }
    }
}