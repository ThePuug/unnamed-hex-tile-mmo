use bevy::prelude::*;
use qrz::Qrz;
use std::f32::consts::PI;

use crate::{
    client::systems::camera::CameraOrbitAngle,
    common::{
        components::{
            heading::*,
            keybits::*,
        },
        message::{AbilityType, Component, Event},
        resources::*,
        systems::targeting::RangeTier,
    }, *
};

pub const KEYCODE_JUMP: KeyCode = KeyCode::Numpad0;
pub const KEYCODE_UP: KeyCode = KeyCode::ArrowUp;
pub const KEYCODE_DOWN: KeyCode = KeyCode::ArrowDown;
pub const KEYCODE_LEFT: KeyCode = KeyCode::ArrowLeft;
pub const KEYCODE_RIGHT: KeyCode = KeyCode::ArrowRight;

/// Milliseconds between periodic input sends
pub const INPUT_SEND_INTERVAL_MS: u128 = 1000;

/// Hex direction indices (camera angle system: 0° = South)
const HEX_DIRECTIONS: [Qrz; 6] = [
    Qrz { q: 1, r: -1, z: 0 },   // 0: NE (30°)
    Qrz { q: 1, r: 0, z: 0 },    // 1: East (90°)
    Qrz { q: 0, r: 1, z: 0 },    // 2: SE (150°)
    Qrz { q: -1, r: 1, z: 0 },   // 3: SW (210°)
    Qrz { q: -1, r: 0, z: 0 },   // 4: West (270°)
    Qrz { q: 0, r: -1, z: 0 },   // 5: NW (330°)
];

const HEX_ANGLES: [f32; 6] = [
    0.0_f32.to_radians(),    // 0°
    60.0_f32.to_radians(),   // 60°
    120.0_f32.to_radians(),  // 120°
    180.0_f32.to_radians(),  // 180°
    240.0_f32.to_radians(),  // 240°
    300.0_f32.to_radians(),  // 300°
];

/// Find direction index from Qrz
fn qrz_to_index(dir: &Qrz) -> Option<usize> {
    HEX_DIRECTIONS.iter().position(|&d| d.q == dir.q && d.r == dir.r)
}

/// Rotate a Qrz direction by a number of hex steps
fn rotate_qrz(dir: &Qrz, steps: i32) -> Qrz {
    if let Some(idx) = qrz_to_index(dir) {
        let new_idx = (idx as i32 + steps).rem_euclid(6) as usize;
        HEX_DIRECTIONS[new_idx]
    } else {
        *dir  // Return unchanged if not a standard hex direction
    }
}

/// Convert a Qrz direction to KeyBits flags
fn qrz_to_keybits(dir: &Qrz) -> KeyBits {
    let mut keybits = KeyBits::default();
    match (dir.q, dir.r) {
        (1, 0) => keybits.set_pressed([KB_HEADING_Q], true),                              // East
        (-1, 0) => keybits.set_pressed([KB_HEADING_Q, KB_HEADING_NEG], true),             // West
        (1, -1) => keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true), // NE
        (0, -1) => keybits.set_pressed([KB_HEADING_R, KB_HEADING_NEG], true),             // NW
        (0, 1) => keybits.set_pressed([KB_HEADING_R], true),                              // SE
        (-1, 1) => keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true),               // SW
        _ => {}
    }
    keybits
}

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_angle: Res<CameraOrbitAngle>,
    mut query: Query<(Entity, &Heading, &mut KeyBits, Option<&crate::common::components::gcd::Gcd>), With<Actor>>,
    mut writer: MessageWriter<Try>,
    dt: Res<Time>,
) {
    if let Ok((ent, &heading, mut keybits0, gcd_opt)) = query.single_mut() {
        // Note: We removed client-side death prediction
        // The server will reject inputs for dead players, preventing premature input blocking
        let delta_ns = dt.delta().as_nanos();
        keybits0.accumulator += delta_ns;

        // Check GCD before allowing ability usage
        let gcd_active = gcd_opt.map_or(false, |gcd| gcd.is_active(dt.elapsed()));

        // ADR-009 MVP Ability Set

        // Lunge ability (Q key) - Gap closer
        if keyboard.just_pressed(KeyCode::KeyQ) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Lunge, target_loc: None }});
        }

        // Overpower ability (W key) - Heavy strike
        if keyboard.just_pressed(KeyCode::KeyW) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Overpower, target_loc: None }});
        }

        // Counter ability (E key) - Reactive counter-attack (ADR-014)
        if keyboard.just_pressed(KeyCode::KeyE) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Counter, target_loc: None }});
        }

        // Deflect ability (R key) - Clear all threats
        if keyboard.just_pressed(KeyCode::KeyR) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Deflect, target_loc: None }});
        }

        // ADR-022: Dismiss front queue threat (no GCD check — independent of ability system)
        if keyboard.just_pressed(KeyCode::KeyD) {
            writer.write(Try { event: Event::Dismiss { ent }});
        }

        // ADR-010 Phase 1: Tier Lock Targeting

        // 1 key: Lock to Close tier (1-2 hexes)
        if keyboard.just_pressed(KeyCode::Digit1) {
            writer.write(Try { event: Event::SetTierLock { ent, tier: RangeTier::Close }});
        }

        // 2 key: Lock to Mid tier (3-6 hexes)
        if keyboard.just_pressed(KeyCode::Digit2) {
            writer.write(Try { event: Event::SetTierLock { ent, tier: RangeTier::Mid }});
        }

        // 3 key: Lock to Far tier (7+ hexes)
        if keyboard.just_pressed(KeyCode::Digit3) {
            writer.write(Try { event: Event::SetTierLock { ent, tier: RangeTier::Far }});
        }

        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_JUMP], keyboard.any_just_pressed([KEYCODE_JUMP]));

        // Skip movement input when Shift is pressed (camera panning mode)
        let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        if !shift_pressed && keyboard.any_pressed([KEYCODE_UP, KEYCODE_DOWN, KEYCODE_LEFT, KEYCODE_RIGHT]) {
            // Snap camera to nearest hex rotation
            let camera_normalized = camera_angle.0.rem_euclid(2.0 * PI);
            let mut camera_rotation_idx = 0;
            let mut min_diff = f32::MAX;
            for (i, &angle) in HEX_ANGLES.iter().enumerate() {
                let diff = (camera_normalized - angle).abs();
                let diff = diff.min(2.0 * PI - diff);
                if diff < min_diff {
                    min_diff = diff;
                    camera_rotation_idx = i;
                }
            }

            // Rotate heading to visual space (what player appears to be facing from camera view)
            let visual_heading = rotate_qrz(&*heading, camera_rotation_idx as i32);

            // Apply existing movement logic with visual heading
            if keyboard.pressed(KEYCODE_UP) {
                if keyboard.pressed(KEYCODE_LEFT) || !keyboard.pressed(KEYCODE_RIGHT)
                    &&(visual_heading == Qrz {q:-1, r: 0, z: 0}
                    || visual_heading == Qrz {q: 0, r:-1, z: 0}
                    || visual_heading == Qrz {q: 0, r: 1, z: 0}) {
                        keybits.set_pressed([KB_HEADING_R, KB_HEADING_NEG], true);  // NW in visual space
                    }
                else {
                    keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true);  // NE in visual space
                }
            } else if keyboard.pressed(KEYCODE_DOWN) {
                if keyboard.pressed(KEYCODE_LEFT) || !keyboard.pressed(KEYCODE_RIGHT)
                    &&(visual_heading == Qrz {q:-1, r: 0, z: 0}
                    || visual_heading == Qrz {q: 1, r:-1, z: 0}
                    || visual_heading == Qrz {q:-1, r: 1, z: 0}) {
                        keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true);  // SW in visual space
                    }
                else {
                    keybits.set_pressed([KB_HEADING_R], true);  // SE in visual space
                }
            }
            else if keyboard.pressed(KEYCODE_RIGHT) {
                keybits.set_pressed([KB_HEADING_Q], true);  // East in visual space
            } else if keyboard.pressed(KEYCODE_LEFT) {
                keybits.set_pressed([KB_HEADING_Q, KB_HEADING_NEG], true);  // West in visual space
            }

            // Now convert keybits (visual direction) back to world direction
            // Extract direction from keybits
            let visual_dir = if keybits.key_bits & KB_HEADING_Q != 0 {
                if keybits.key_bits & KB_HEADING_R != 0 {
                    if keybits.key_bits & KB_HEADING_NEG != 0 {
                        Qrz { q: 1, r: -1, z: 0 }  // NE
                    } else {
                        Qrz { q: -1, r: 1, z: 0 }  // SW
                    }
                } else {
                    if keybits.key_bits & KB_HEADING_NEG != 0 {
                        Qrz { q: -1, r: 0, z: 0 }  // West
                    } else {
                        Qrz { q: 1, r: 0, z: 0 }   // East
                    }
                }
            } else if keybits.key_bits & KB_HEADING_R != 0 {
                if keybits.key_bits & KB_HEADING_NEG != 0 {
                    Qrz { q: 0, r: -1, z: 0 }  // NW
                } else {
                    Qrz { q: 0, r: 1, z: 0 }   // SE
                }
            } else {
                Qrz { q: 0, r: 0, z: 0 }  // No movement
            };

            // Rotate back to world space
            let world_dir = rotate_qrz(&visual_dir, -(camera_rotation_idx as i32));

            // Convert back to keybits (preserve jump flag)
            let jump_flag = keybits.key_bits & KB_JUMP;
            keybits = qrz_to_keybits(&world_dir);
            keybits.key_bits |= jump_flag;
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
    mut reader: MessageReader<Do>,
    mut buffers: ResMut<InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::Input { ent, key_bits, dt, seq }} = message else { continue };
        let Some(buffer) = buffers.get_mut(&ent) else { panic!("no {ent} in buffers") };

        // Maintain invariant: all queues always have at least 1 input
        // Never remove the last input (the accumulating one)
        if buffer.queue.len() <= 1 {
            panic!("Queue invariant violation: attempted to remove last input (seq {seq}). Queue must always have at least 1 input.");
        }

        // Server sends confirmations in order from back (oldest first)
        // Simply pop from back
        let removed = buffer.queue.pop_back().expect("queue should have at least 2 inputs");
        let Event::Input { ent: ent0, key_bits: kb0, dt: dt0, seq: seq0 } = removed else { panic!("not input") };

        // Verify the confirmation matches what we expected
        assert!(ent == ent0, "Entity mismatch");
        assert!(seq == seq0, "Seq mismatch: expected {seq0}, got {seq}");

        if key_bits != kb0 {
            warn!("KeyBits mismatch for seq {seq}: client={:?}, server={:?}", kb0, key_bits);
        }

        // dt mismatch is expected due to client-side prediction
        if (dt as i32 - dt0 as i32).abs() > 109 {
            warn!("dt mismatch for seq {seq}: server={dt}, client={dt0}");
        }

        // Warn if queue is getting too long (indicates confirmations not keeping up)
        if buffer.queue.len() > 5 {
            warn!("Input queue length: {} (confirmations lagging)", buffer.queue.len());
        }
    }
}