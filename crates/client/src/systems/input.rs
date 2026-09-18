//! Keyboard to input messages. The client owns the clock for its own
//! movement: every fixed tick it attributes the elapsed time to the open
//! input and sends it, and every change of keys or heading opens a new one.

use bevy::prelude::*;

use crate::systems::camera::CameraOrbit;
use crate::*;
use common_bevy::{
    components::{
        keybits::*,
        position::Position,
        target::Target,
        AirTime,
    },
    message::{AbilityType, Event, *},
    resources::*,
    systems::targeting::RangeTier,
};

pub const KEYCODE_JUMP: KeyCode = KeyCode::Numpad0;
pub const KEYCODE_UP: KeyCode = KeyCode::ArrowUp;
pub const KEYCODE_DOWN: KeyCode = KeyCode::ArrowDown;
pub const KEYCODE_LEFT: KeyCode = KeyCode::ArrowLeft;
pub const KEYCODE_RIGHT: KeyCode = KeyCode::ArrowRight;

/// Milliseconds an input stays open before a new one is opened for the same
/// keys, so the server confirms at least this often.
pub const INPUT_ROLL_MS: u128 = 1000;

/// Milliseconds of an open input accumulated before they go on the wire.
pub const INPUT_SEND_MS: u16 = 50;

/// Slots a backward diagonal turns away from straight back: 45°.
const BACK_DIAGONAL_SLOTS: i32 = 3;

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    panel: Res<crate::systems::character_panel::CharacterPanelState>,
    mut camera_orbit: ResMut<CameraOrbit>,
    mut query: Query<(Entity, &mut KeyBits, Option<&common_bevy::components::gcd::Gcd>, &Target), With<Actor>>,
    mut writer: MessageWriter<Try>,
    mut buffers: ResMut<InputQueues>,
    dt: Res<Time>,
) {
    // The character panel is modal: while it is open every gameplay key
    // reads as released, so the character stops and nothing fires.
    let released = ButtonInput::default();
    let keyboard: &ButtonInput<KeyCode> = if panel.visible { &released } else { &keyboard };
    let Ok((ent, mut keybits0, gcd_opt, target)) = query.single_mut() else { return };

    let delta_ns = dt.delta().as_nanos();
    keybits0.accumulator += delta_ns;

    // Check GCD before allowing ability usage
    let gcd_active = gcd_opt.map_or(false, |gcd| gcd.is_active(dt.elapsed()));

    // MVP Ability Set

    // Lunge ability (Q key) - Gap closer
    if keyboard.just_pressed(KeyCode::KeyQ) && !gcd_active {
        writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Lunge, target: target.entity }});
    }

    // Overpower ability (W key) - Heavy strike
    if keyboard.just_pressed(KeyCode::KeyW) && !gcd_active {
        writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Overpower, target: target.entity }});
    }

    // Counter ability (E key) - Reactive counter-attack
    if keyboard.just_pressed(KeyCode::KeyE) && !gcd_active {
        writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Counter, target: None }});
    }

    // Kick ability (R key) - Reactive knockback
    if keyboard.just_pressed(KeyCode::KeyR) && !gcd_active {
        writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Kick, target: None }});
    }

    // Dismiss front queue threat (no GCD check — independent of ability system)
    if keyboard.just_pressed(KeyCode::KeyD) {
        writer.write(Try { event: Event::Dismiss { ent }});
    }

    // Tier Lock Targeting

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

    let mut keybits = KeyBits { heading: keybits0.heading, ..default() };
    keybits.set_pressed([KB_JUMP], keyboard.any_just_pressed([KEYCODE_JUMP]));

    // Shift hands the arrows to camera panning.
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !shift_pressed && keyboard.any_pressed([KEYCODE_UP, KEYCODE_DOWN, KEYCODE_LEFT, KEYCODE_RIGHT]) {
        let up = keyboard.pressed(KEYCODE_UP);
        let down = keyboard.pressed(KEYCODE_DOWN);
        let left = keyboard.pressed(KEYCODE_LEFT);
        let right = keyboard.pressed(KEYCODE_RIGHT);

        // Left and right step the camera, alone or with Up; a backward
        // diagonal holds it. Forward is whatever stop the camera lands on.
        let turning = !(down && !up);
        if turning && left && !right {
            camera_orbit.step_ccw(dt.delta_secs());
        } else if turning && right && !left {
            camera_orbit.step_cw(dt.delta_secs());
        } else {
            camera_orbit.release();
        }

        let forward = camera_orbit.forward();
        let heading = if up && !down {
            Some(forward)
        } else if down && !up {
            let back = forward.reversed();
            Some(if left && !right { back.turned(BACK_DIAGONAL_SLOTS) }
                else if right && !left { back.turned(-BACK_DIAGONAL_SLOTS) }
                else { back })
        } else {
            None
        };

        if let Some(heading) = heading {
            keybits.set_pressed([KB_MOVE], true);
            keybits.heading = heading;
        }
    } else {
        camera_orbit.release();
    }

    // A new input opens when the keys change, or after INPUT_ROLL_MS so the
    // server confirms at least that often.
    if !keybits0.same_input(&keybits) || keybits0.accumulator >= INPUT_ROLL_MS * 1_000_000 {
        *keybits0 = keybits;
        let Some(buffer) = buffers.get_mut(&ent) else { return };
        open_input(buffer, ent, keybits, &mut writer);
    }
}

/// Opens input `seq + 1` at the front of the queue and puts it on the wire,
/// after flushing what the closing input has not sent yet, so the server
/// sees the old input whole before the new one.
fn open_input(buffer: &mut InputQueue, ent: Entity, key_bits: KeyBits, writer: &mut MessageWriter<Try>) {
    let Some(Event::Input { key_bits: kb0, seq: seq0, .. }) = buffer.queue.front().cloned() else {
        panic!("Queue invariant violation: entity {ent} has empty queue");
    };
    if buffer.unsent_ms > 0 {
        writer.write(Try { event: Event::Input { ent, key_bits: kb0, dt: buffer.unsent_ms, seq: seq0 }});
        buffer.unsent_ms = 0;
    }
    let seq = seq0.wrapping_add(1);
    buffer.queue.push_front(Event::Input { ent, key_bits, dt: 0, seq });
    writer.write(Try { event: Event::Input { ent, key_bits, dt: 0, seq }});
}

/// Attributes the fixed tick to the open input and sends it once
/// `INPUT_SEND_MS` have gathered. Sub-millisecond time carries over, so the
/// client's clock and the server's agree over any span.
pub fn tick(
    time: Res<Time>,
    mut buffers: ResMut<InputQueues>,
    mut writer: MessageWriter<Try>,
) {
    let delta_us = time.delta().as_micros() as u32;
    let entities: Vec<Entity> = buffers.entities().copied().collect();
    for ent in entities {
        let Some(buffer) = buffers.get_mut(&ent) else { continue };
        buffer.residual_us += delta_us;
        let dt = (buffer.residual_us / 1000) as u16;
        buffer.residual_us %= 1000;
        if dt == 0 { continue; }

        let Some(Event::Input { key_bits, dt: dt0, seq, .. }) = buffer.queue.front_mut() else {
            panic!("Queue invariant violation: entity {ent} has empty queue");
        };
        *dt0 = dt0.saturating_add(dt);
        let (key_bits, seq) = (*key_bits, *seq);

        buffer.unsent_ms += dt;
        if buffer.unsent_ms >= INPUT_SEND_MS {
            writer.write(Try { event: Event::Input { ent, key_bits, dt: buffer.unsent_ms, seq }});
            buffer.unsent_ms = 0;
        }
    }
}

/// Adopts the server's position for a closed input and drops it from the
/// queue. Prediction then replays only what is still open, from exactly
/// where the server left the entity.
pub fn do_confirm(
    mut reader: MessageReader<Do>,
    mut buffers: ResMut<InputQueues>,
    mut query: Query<(&mut Position, &mut AirTime)>,
) {
    for message in reader.read() {
        let Do { event: Event::Confirm { ent, seq, position, airtime } } = message else { continue };
        let (ent, seq) = (*ent, *seq);
        let Some(buffer) = buffers.get_mut(&ent) else { panic!("no {ent} in buffers") };

        // Never remove the last input: the open one accumulates.
        if buffer.queue.len() <= 1 {
            panic!("Queue invariant violation: attempted to remove last input (seq {seq}). Queue must always have at least 1 input.");
        }

        let removed = buffer.queue.pop_back().expect("queue should have at least 2 inputs");
        let Event::Input { seq: seq0, .. } = removed else { panic!("not input") };
        assert!(seq == seq0, "Seq mismatch: expected {seq0}, got {seq}");

        if let Ok((mut pos, mut air)) = query.get_mut(ent) {
            *pos = *position;
            air.state = *airtime;
        }

        if buffer.queue.len() > 5 {
            warn!("Input queue length: {} (confirmations lagging)", buffer.queue.len());
        }
    }
}
