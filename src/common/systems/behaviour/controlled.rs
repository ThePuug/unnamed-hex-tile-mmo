use bevy::prelude::*;

use crate::common::{
        components::{behaviour::*, heading::*, keybits::*, offset::*, *}, 
        message::{Component, Event, *}, 
        plugins::nntree::*, 
        resources::{map::*, *}, 
        systems::physics,
    };

pub fn tick(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    query: Query<&Behaviour>,
    time: Res<Time>,
    mut buffers: ResMut<InputQueues>,
) {
    let dt = time.delta().as_millis() as u16;

    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::KeyBits(keybits) }} = message else { continue };
        let Some(buffer) = buffers.get_mut(&ent)
            // disconnect by client could remove buffer while message in transit
            else { continue };

        // Queue invariant: all queues must have at least 1 input
        let Some(input0) = buffer.queue.front() else {
            panic!("Queue invariant violation: entity {ent} has empty queue");
        };
        let Event::Input { seq: seq0, .. } = input0 else { panic!("not input") };

        // Always create a new input, even for periodic updates
        // This ensures dt doesn't overflow and inputs get confirmed regularly
        let input = Event::Input { ent, key_bits: keybits, dt: 0, seq: seq0.wrapping_add(1) };
        buffer.queue.push_front(input.clone());
        writer.write(Try { event: input });
    }

    // Only iterate over entities with queues (all queues always have at least 1 input)
    let entities_to_process: Vec<Entity> = buffers.entities().copied().collect();

    for ent in entities_to_process {
        // Verify entity still has Controlled behaviour
        let Ok(&behaviour) = query.get(ent) else { continue };
        let Behaviour::Controlled = behaviour else { continue; };

        let Some(buffer) = buffers.get_mut(&ent)
            // disconnect by client could remove buffer while message in transit
            else { continue };

        // Queue invariant: all queues must have at least 1 input
        // Access front input without removing it to maintain invariant
        let Some(input0) = buffer.queue.front_mut() else {
            panic!("Queue invariant violation: entity {ent} has empty queue");
        };
        let Event::Input { key_bits: keybits0, dt: dt0, seq: seq0, .. } = *input0 else { panic!("not input") };

        // Use saturating_add to prevent dt overflow (u16 max is ~65 seconds)
        let dt0_new = dt0.saturating_add(dt);

        // Update the front input in place (never leave queue empty)
        *input0 = Event::Input { ent, key_bits: keybits0, dt: dt0_new, seq: seq0 };

        writer.write(Try { event: Event::Input { ent, key_bits: keybits0, dt, seq: seq0 }});
    }
}

pub fn apply(
    mut reader: EventReader<Do>,
    mut query: Query<(&Loc, &Heading, &mut Offset, &mut AirTime, Option<&ActorAttributes>)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, dt, key_bits, .. } } = message {
            let Ok((&loc, &heading, mut offset, mut airtime, attrs)) = query.get_mut(ent)
                // disconnect by client could remove entity while message in transit
                else { continue };
            let dest = Loc::new(*Heading::from(key_bits) + *loc);
            if key_bits.is_pressed(KB_JUMP) && airtime.state.is_none() { airtime.state = Some(125); }
            let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);
            (offset.state, airtime.state) = physics::apply(dest, dt as i16, loc, offset.state, airtime.state, movement_speed, heading, &map, &nntree);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_maintains_queue_invariant_during_update() {
        let mut app = App::new();
        app.add_event::<Do>();
        app.add_event::<Try>();
        app.insert_resource(InputQueues::default());
        app.init_resource::<Time>();
        app.add_systems(Update, tick);

        let entity = app.world_mut().spawn((Behaviour::Controlled, Loc::default())).id();

        // Create initial queue with 1 input
        let mut queue = InputQueue::default();
        queue.queue.push_back(Event::Input {
            ent: entity,
            key_bits: KeyBits::default(),
            dt: 0,
            seq: 0,
        });

        app.world_mut().resource_mut::<InputQueues>().insert(entity, queue);

        // Simulate receiving a KeyBits update
        app.world_mut().send_event(Do {
            event: Event::Incremental {
                ent: entity,
                component: Component::KeyBits(KeyBits::default()),
            }
        });

        // Run the tick system
        app.update();

        // Verify invariant: queue should still have at least 1 input
        let buffers = app.world().resource::<InputQueues>();
        let buffer = buffers.get(&entity).unwrap();
        assert!(
            !buffer.queue.is_empty(),
            "Queue invariant violated: queue became empty during tick"
        );

        // Should now have 2 inputs (original + new one from KeyBits update)
        assert_eq!(buffer.queue.len(), 2, "Should have added new input");
    }

    #[test]
    fn test_tick_preserves_single_input_in_queue() {
        let mut app = App::new();
        app.add_event::<Do>();
        app.add_event::<Try>();
        app.insert_resource(InputQueues::default());
        app.init_resource::<Time>();
        app.add_systems(Update, tick);

        let entity = app.world_mut().spawn((Behaviour::Controlled, Loc::default())).id();

        // Create queue with 1 input (dt=100)
        let mut queue = InputQueue::default();
        queue.queue.push_back(Event::Input {
            ent: entity,
            key_bits: KeyBits::default(),
            dt: 100,
            seq: 5,
        });

        app.world_mut().resource_mut::<InputQueues>().insert(entity, queue);

        // Run tick multiple times
        for _ in 0..5 {
            app.update();
        }

        // Verify queue still has exactly 1 input (never becomes empty)
        let buffers = app.world().resource::<InputQueues>();
        let buffer = buffers.get(&entity).unwrap();
        assert_eq!(buffer.queue.len(), 1, "Queue should maintain exactly 1 input");

        // Verify the input is still there with same sequence
        if let Some(Event::Input { seq, .. }) = buffer.queue.front() {
            assert_eq!(*seq, 5, "Sequence number should be unchanged");
        } else {
            panic!("Expected Input event");
        }
    }

    #[test]
    #[should_panic(expected = "Queue invariant violation")]
    fn test_tick_with_empty_queue_panics() {
        let mut app = App::new();
        app.add_event::<Do>();
        app.add_event::<Try>();
        app.insert_resource(InputQueues::default());
        app.init_resource::<Time>();
        app.add_systems(Update, tick);

        let entity = app.world_mut().spawn((Behaviour::Controlled, Loc::default())).id();

        // Manually create an empty queue (bypassing the insert check for testing)
        let queue = InputQueue::default();
        app.world_mut().resource_mut::<InputQueues>().insert_for_test(entity, queue);

        // Send KeyBits update
        app.world_mut().send_event(Do {
            event: Event::Incremental {
                ent: entity,
                component: Component::KeyBits(KeyBits::default()),
            }
        });

        // This should panic when trying to read from empty queue
        app.update();
    }
}
