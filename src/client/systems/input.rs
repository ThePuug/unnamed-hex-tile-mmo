use bevy::prelude::*;
use qrz::Qrz;

use crate::{common::{
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

pub fn update_keybits(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &Heading, &mut KeyBits, Option<&crate::common::components::gcd::Gcd>), With<Actor>>,
    mut writer: EventWriter<Try>,
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

        // Knockback ability (E key) - Push enemy
        if keyboard.just_pressed(KeyCode::KeyE) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Knockback, target_loc: None }});
        }

        // Deflect ability (R key) - Clear all threats
        if keyboard.just_pressed(KeyCode::KeyR) && !gcd_active {
            writer.write(Try { event: Event::UseAbility { ent, ability: AbilityType::Deflect, target_loc: None }});
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "Queue invariant violation: attempted to remove last input")]
    fn test_do_input_removing_last_input_panics() {
        let mut app = App::new();
        app.add_event::<Do>();
        app.insert_resource(InputQueues::default());
        app.add_systems(Update, do_input);

        let entity = app.world_mut().spawn_empty().id();

        // Create a queue with exactly 1 input
        let mut queue = InputQueue::default();
        queue.queue.push_back(Event::Input {
            ent: entity,
            key_bits: KeyBits::default(),
            dt: 0,
            seq: 0,
        });

        app.world_mut().resource_mut::<InputQueues>().insert(entity, queue);

        // Server sends confirmation for that single input
        app.world_mut().send_event(Do {
            event: Event::Input {
                ent: entity,
                key_bits: KeyBits::default(),
                dt: 100,
                seq: 0,
            }
        });

        // This should panic because we're trying to remove the last input
        app.update();
    }

    #[test]
    fn test_do_input_removes_confirmed_input_when_multiple_exist() {
        let mut app = App::new();
        app.add_event::<Do>();
        app.insert_resource(InputQueues::default());
        app.add_systems(Update, do_input);

        let entity = app.world_mut().spawn_empty().id();

        // Create a queue with 3 inputs
        // Front (newest): seq 2, Middle: seq 1, Back (oldest): seq 0
        let mut queue = InputQueue::default();
        for seq in 0..3 {
            queue.queue.push_front(Event::Input {
                ent: entity,
                key_bits: KeyBits::default(),
                dt: 0,
                seq,
            });
        }

        app.world_mut().resource_mut::<InputQueues>().insert(entity, queue);

        // Server confirms seq 0 (oldest, at back)
        app.world_mut().send_event(Do {
            event: Event::Input {
                ent: entity,
                key_bits: KeyBits::default(),
                dt: 100,
                seq: 0,
            }
        });

        // Should NOT panic
        app.update();

        // Queue should now have 2 inputs remaining
        let buffers = app.world().resource::<InputQueues>();
        let buffer = buffers.get(&entity).unwrap();
        assert_eq!(buffer.queue.len(), 2);

        // Verify the correct input was removed (seq 0 is gone)
        if let Some(Event::Input { seq, .. }) = buffer.queue.back() {
            assert_eq!(*seq, 1, "Oldest remaining input should be seq 1");
        } else {
            panic!("Expected Input event in queue");
        }
    }

    #[test]
    fn test_do_input_maintains_invariant_with_multiple_confirmations() {
        let mut app = App::new();
        app.add_event::<Do>();
        app.insert_resource(InputQueues::default());
        app.add_systems(Update, do_input);

        let entity = app.world_mut().spawn_empty().id();

        // Create a queue with 3 inputs
        // Front (newest): seq 2, Middle: seq 1, Back (oldest): seq 0
        let mut queue = InputQueue::default();
        for seq in 0..3 {
            queue.queue.push_front(Event::Input {
                ent: entity,
                key_bits: KeyBits::default(),
                dt: 0,
                seq,
            });
        }

        app.world_mut().resource_mut::<InputQueues>().insert(entity, queue);

        // Process two confirmations (seq 0 and 1)
        for seq in 0..2 {
            app.world_mut().send_event(Do {
                event: Event::Input {
                    ent: entity,
                    key_bits: KeyBits::default(),
                    dt: 100,
                    seq,
                }
            });
            app.update();
        }

        // Queue should have exactly 1 input remaining (seq 2)
        let buffers = app.world().resource::<InputQueues>();
        let buffer = buffers.get(&entity).unwrap();
        assert_eq!(buffer.queue.len(), 1, "Should maintain invariant: exactly 1 input remains");

        if let Some(Event::Input { seq, .. }) = buffer.queue.back() {
            assert_eq!(*seq, 2, "Remaining input should be seq 2");
        }
    }
}