use bevy::prelude::*;
use qrz::Qrz;

use crate::systems::camera::CameraOrbit;
use crate::*;
use common_bevy::{
    components::{
        keybits::*,
        target::Target,
    },
    message::{AbilityType, Component, Event},
    resources::*,
    systems::targeting::RangeTier,
};

pub const KEYCODE_JUMP: KeyCode = KeyCode::Numpad0;
pub const KEYCODE_UP: KeyCode = KeyCode::ArrowUp;
pub const KEYCODE_DOWN: KeyCode = KeyCode::ArrowDown;
pub const KEYCODE_LEFT: KeyCode = KeyCode::ArrowLeft;
pub const KEYCODE_RIGHT: KeyCode = KeyCode::ArrowRight;

/// Milliseconds between periodic input sends
pub const INPUT_SEND_INTERVAL_MS: u128 = 1000;

/// Hex direction indices ordered by visual angle (0°, 60°, 120°, 180°, 240°, 300°).
///
/// For **flat-top** the visual angles align with the 6 directions:
///   0°=N(0,-1), 60°=NE(1,-1), 120°=SE(1,0), 180°=S(0,1), 240°=SW(-1,1), 300°=NW(-1,0)
///
/// For **pointy-top** the visual angles are offset 30° but the 60° step spacing is the same:
///   0→NE(1,-1), 1→E(1,0), 2→SE(0,1), 3→SW(-1,1), 4→W(-1,0), 5→NW(0,-1)
///
/// In both cases, stepping +1 index = +60° clockwise rotation.
const HEX_DIRECTIONS_FLAT: [Qrz; 6] = [
    Qrz { q: 0, r: -1, z: 0 },   // 0: N   (0°)
    Qrz { q: 1, r: -1, z: 0 },   // 1: NE  (60°)
    Qrz { q: 1, r: 0, z: 0 },    // 2: SE  (120°)
    Qrz { q: 0, r: 1, z: 0 },    // 3: S   (180°)
    Qrz { q: -1, r: 1, z: 0 },   // 4: SW  (240°)
    Qrz { q: -1, r: 0, z: 0 },   // 5: NW  (300°)
];

const HEX_DIRECTIONS_POINTY: [Qrz; 6] = [
    Qrz { q: 1, r: -1, z: 0 },   // 0: NE  (30°)
    Qrz { q: 1, r: 0, z: 0 },    // 1: E   (90°)
    Qrz { q: 0, r: 1, z: 0 },    // 2: SE  (150°)
    Qrz { q: -1, r: 1, z: 0 },   // 3: SW  (210°)
    Qrz { q: -1, r: 0, z: 0 },   // 4: W   (270°)
    Qrz { q: 0, r: -1, z: 0 },   // 5: NW  (330°)
];

/// Find direction index from Qrz in the given direction table.
fn qrz_to_index(dir: &Qrz, table: &[Qrz; 6]) -> Option<usize> {
    table.iter().position(|&d| d.q == dir.q && d.r == dir.r)
}

/// Rotate a Qrz direction by a number of hex steps through the given table.
fn rotate_qrz(dir: &Qrz, steps: i32, table: &[Qrz; 6]) -> Qrz {
    if let Some(idx) = qrz_to_index(dir, table) {
        let new_idx = (idx as i32 + steps).rem_euclid(6) as usize;
        table[new_idx]
    } else {
        *dir
    }
}

/// Convert a Qrz direction to KeyBits flags
fn qrz_to_keybits(dir: &Qrz) -> KeyBits {
    let mut keybits = KeyBits::default();
    match (dir.q, dir.r) {
        (1, 0) => keybits.set_pressed([KB_HEADING_Q], true),                              // East / SE(flat)
        (-1, 0) => keybits.set_pressed([KB_HEADING_Q, KB_HEADING_NEG], true),             // West / NW(flat)
        (1, -1) => keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG], true), // NE
        (0, -1) => keybits.set_pressed([KB_HEADING_R, KB_HEADING_NEG], true),             // NW / N(flat)
        (0, 1) => keybits.set_pressed([KB_HEADING_R], true),                              // SE / S(flat)
        (-1, 1) => keybits.set_pressed([KB_HEADING_Q, KB_HEADING_R], true),               // SW
        _ => {}
    }
    keybits
}

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_orbit: ResMut<CameraOrbit>,
    map: Res<map::Map>,
    mut query: Query<(Entity, &mut KeyBits, Option<&common_bevy::components::gcd::Gcd>, &Target), With<Actor>>,
    mut writer: MessageWriter<Try>,
    dt: Res<Time>,
) {
    if let Ok((ent, mut keybits0, gcd_opt, target)) = query.single_mut() {
        // Note: We removed client-side death prediction
        // The server will reject inputs for dead players, preventing premature input blocking
        let delta_ns = dt.delta().as_nanos();
        keybits0.accumulator += delta_ns;

        // Check GCD before allowing ability usage
        let gcd_active = gcd_opt.map_or(false, |gcd| gcd.is_active(dt.elapsed()));

        // ADR-009 MVP Ability Set

        // Lunge ability (Q key) - Gap closer
        if keyboard.just_pressed(KeyCode::KeyQ) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Lunge, target: target.entity }});
        }

        // Overpower ability (W key) - Heavy strike
        if keyboard.just_pressed(KeyCode::KeyW) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Overpower, target: target.entity }});
        }

        // Counter ability (E key) - Reactive counter-attack (ADR-014)
        if keyboard.just_pressed(KeyCode::KeyE) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Counter, target: None }});
        }

        // Kick ability (R key) - Reactive knockback
        if keyboard.just_pressed(KeyCode::KeyR) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Kick, target: None }});
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

        let orientation = map.orientation();
        let dir_table = match orientation {
            qrz::HexOrientation::FlatTop => &HEX_DIRECTIONS_FLAT,
            qrz::HexOrientation::PointyTop => &HEX_DIRECTIONS_POINTY,
        };

        // Skip movement input when Shift is pressed (camera panning mode)
        let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        if !shift_pressed && keyboard.any_pressed([KEYCODE_UP, KEYCODE_DOWN, KEYCODE_LEFT, KEYCODE_RIGHT]) {
            // Use discrete target_index as the stable camera frame for direction resolution
            let camera_rotation_idx = camera_orbit.target_index;

            let up = keyboard.pressed(KEYCODE_UP);
            let down = keyboard.pressed(KEYCODE_DOWN);
            let left = keyboard.pressed(KEYCODE_LEFT);
            let right = keyboard.pressed(KEYCODE_RIGHT);

            // Determine visual direction and camera rotation side-effect.
            //
            // Up/Up+Left/Up+Right: move forward (with optional diagonal).
            //   Up+Left also steps camera CCW. Up+Right also steps camera CW.
            // Down/Down+Left/Down+Right: move backward. No camera rotation.
            // Left or Right alone: camera rotation only, no movement.
            let visual_dir = if up && !down {
                if left && !right {
                    camera_orbit.step_ccw();
                    Qrz { q: -1, r: 0, z: 0 }    // NW (forward-left)
                } else if right && !left {
                    camera_orbit.step_cw();
                    Qrz { q: 1, r: -1, z: 0 }     // NE (forward-right)
                } else {
                    Qrz { q: 0, r: -1, z: 0 }     // N (forward)
                }
            } else if down && !up {
                if left && !right {
                    Qrz { q: -1, r: 1, z: 0 }     // SW (backward-left)
                } else if right && !left {
                    Qrz { q: 1, r: 0, z: 0 }      // SE (backward-right)
                } else {
                    Qrz { q: 0, r: 1, z: 0 }      // S (backward)
                }
            } else if left && !right {
                camera_orbit.step_ccw();
                Qrz { q: 0, r: 0, z: 0 }          // Rotate only, no movement
            } else if right && !left {
                camera_orbit.step_cw();
                Qrz { q: 0, r: 0, z: 0 }          // Rotate only, no movement
            } else {
                Qrz { q: 0, r: 0, z: 0 }
            };

            if visual_dir.q != 0 || visual_dir.r != 0 {
                // Rotate visual direction to world space using the pre-rotation camera frame
                let world_dir = rotate_qrz(&visual_dir, -(camera_rotation_idx as i32), dir_table);

                let jump_flag = keybits.key_bits & KB_JUMP;
                keybits = qrz_to_keybits(&world_dir);
                keybits.key_bits |= jump_flag;
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
    mut reader: MessageReader<Do>,
    mut buffers: ResMut<InputQueues>,
) {
    for message in reader.read() {
        let Do { event: Event::Input { ent, key_bits, dt, seq }} = message else { continue };
        let ent = *ent;
        let key_bits = *key_bits;
        let dt = *dt;
        let seq = *seq;
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
