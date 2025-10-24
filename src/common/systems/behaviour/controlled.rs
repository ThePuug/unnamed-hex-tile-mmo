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
    dt: Res<Time>,
    mut buffers: ResMut<InputQueues>,
) {
    let dt = dt.delta().as_millis() as u16;

    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::KeyBits(keybits) }} = message else { continue };
        let Some(buffer) = buffers.get_mut(&ent)
            // disconnect by client could remove buffer while message in transit
            else { continue };
        let Some(input0) = buffer.queue.front() else { panic!("no front on buffer") };
        let Event::Input { seq: seq0, .. } = input0 else { panic!("not input") };

        // Always create a new input, even for periodic updates
        // This ensures dt doesn't overflow and inputs get confirmed regularly
        let input = Event::Input { ent, key_bits: keybits, dt: 0, seq: seq0.wrapping_add(1) };
        buffer.queue.push_front(input);
        buffers.mark_non_empty(ent);
        writer.write(Try { event: input });
    }

    // Only iterate over entities with non-empty queues instead of all controlled entities
    let entities_to_process: Vec<Entity> = buffers.non_empty_entities().copied().collect();
    
    for ent in entities_to_process {
        // Verify entity still has Controlled behaviour
        let Ok(&behaviour) = query.get(ent) else { continue };
        let Behaviour::Controlled = behaviour else { continue; };

        let Some(buffer) = buffers.get_mut(&ent) 
            // disconnect by client could remove buffer while message in transit
            else { continue };
        let Some(input0) = buffer.queue.pop_front() else { panic!("no front on buffer") };
        let Event::Input { key_bits: keybits0, dt: dt0, seq: seq0, .. } = input0 else { panic!("not input") };

        // Use saturating_add to prevent dt overflow (u16 max is ~65 seconds)
        let dt0 = dt0.saturating_add(dt);
        buffer.queue.push_front(Event::Input { ent, key_bits: keybits0, dt: dt0, seq: seq0 });
        // Queue still has items, keep it marked as non-empty
        writer.write(Try { event: Event::Input { ent, key_bits: keybits0, dt, seq: seq0 }});
    }
}

pub fn apply(
    mut reader: EventReader<Do>,
    mut query: Query<(&Loc, &mut Offset, &mut AirTime, Option<&ActorAttributes>)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, dt, key_bits, .. } } = message {
            let Ok((&loc, mut offset, mut airtime, attrs)) = query.get_mut(ent)
                // disconnect by client could remove entity while message in transit
                else { continue };
            let dest = Loc::new(*Heading::from(key_bits) + *loc);
            if key_bits.is_pressed(KB_JUMP) && airtime.state.is_none() { airtime.state = Some(125); }
            let movement_speed = attrs.map(|a| a.movement_speed).unwrap_or(0.005);
            (offset.state, airtime.state) = physics::apply(dest, dt as i16, loc, offset.state, airtime.state, movement_speed, &map, &nntree);
        }
    }
}

pub fn interpolate_remote(
    mut query: Query<(Entity, &mut Offset, &Behaviour, Option<&ActorAttributes>)>,
    buffers: Res<InputQueues>,
    dt: Res<Time>,
) {
    let dt = dt.delta().as_millis() as f32;

    for (entity, mut offset, &behaviour, attrs) in &mut query {
        let Behaviour::Controlled = behaviour else { continue; };

        // Only process remote players (entities without input buffers)
        // Local players are handled by physics system
        if buffers.get(&entity).is_some() {
            continue;
        }

        let movement_speed = attrs.map(|a| a.movement_speed).unwrap_or(0.005);

        // Move step toward state (zero for remote players) at movement_speed
        offset.prev_step = offset.step;

        let direction = offset.state - offset.step;
        let distance = direction.length();

        if distance > 0.001 {
            let move_dist = movement_speed * dt;
            if move_dist >= distance {
                offset.step = offset.state;
            } else {
                offset.step += direction.normalize() * move_dist;
            }
        } else {
            offset.step = offset.state;
        }
    }
}
