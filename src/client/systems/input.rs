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
        // Periodic updates prevent dt overflow but won't create duplicate inputs
        // because controlled::tick will skip if key_bits haven't changed
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
        let Some(buffer) = buffers.get_mut(&ent) else { panic!("no {ent} in buffers") };

        if buffer.queue.is_empty() {
            warn!("Received confirmation for seq {seq} but queue is empty");
            continue;
        }

        // Due to network latency and periodic updates, client and server queues may temporarily
        // have different lengths. The server sends confirmations from the back (oldest first).
        // We should accept confirmations as long as they're reasonably close to our back.

        // Find the input in our queue - it should be near the back
        let mut found_at = None;
        for (idx, input) in buffer.queue.iter().enumerate().rev() {
            if let Event::Input { seq: input_seq, .. } = input {
                if *input_seq == seq {
                    found_at = Some(idx);
                    break;
                }
            }
        }

        let Some(idx) = found_at else {
            // Input not found - likely already confirmed or queue desync
            let back_seq = buffer.queue.back().and_then(|e|
                if let Event::Input { seq, .. } = e { Some(*seq) } else { None });
            warn!("Received confirmation for seq {seq} but not found in queue (back: {:?}, len: {})",
                back_seq, buffer.queue.len());
            continue;
        };

        // Remove the confirmed input
        let removed = buffer.queue.remove(idx).unwrap();
        let Event::Input { ent: ent0, key_bits: kb0, dt: dt0, seq: seq0 } = removed else { unreachable!() };

        assert!(ent == ent0);
        assert!(seq == seq0);

        // Verify key_bits match - if not, we have a serious desync
        if kb0 != key_bits {
            warn!("Confirmed seq {seq} but key_bits mismatch: client={:?}, server={:?}", kb0, key_bits);
        }

        // dt mismatch is expected due to client-side prediction
        if (dt as i32 - dt0 as i32).abs() > 100 {
            warn!("dt mismatch for seq {seq}: server={dt}, client={dt0}");
        }

        // Warn if queue is getting too long (indicates confirmations not keeping up)
        if buffer.queue.len() > 5 {
            warn!("Input queue length: {} (confirmations lagging)", buffer.queue.len());
        }
    }
}