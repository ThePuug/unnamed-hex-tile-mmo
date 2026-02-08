use bevy::prelude::*;

use crate::common::{
        components::{behaviour::*, heading::*, keybits::*, offset::*, *}, 
        message::{Component, Event, *}, 
        plugins::nntree::*, 
        resources::{map::*, *}, 
        systems::physics,
    };

pub fn tick(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
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
    mut reader: MessageReader<Do>,
    mut query: Query<(&Loc, &mut Heading, &mut Offset, &mut AirTime, Option<&ActorAttributes>)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, dt, key_bits, .. } } = message {
            let Ok((&loc, mut heading, mut offset, mut airtime, attrs)) = query.get_mut(ent)
                // disconnect by client could remove entity while message in transit
                else { continue };
            let new_heading = Heading::from(key_bits);
            let dest = Loc::new(*new_heading + *loc);

            // Update heading component if non-default
            if new_heading != default() {
                *heading = new_heading;
            }

            if key_bits.is_pressed(KB_JUMP) && airtime.state.is_none() { airtime.state = Some(125); }
            let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);
            (offset.state, airtime.state) = physics::apply(dest, dt as i16, loc, offset.state, airtime.state, movement_speed, *heading, &map, &nntree);
        }
    }
}
