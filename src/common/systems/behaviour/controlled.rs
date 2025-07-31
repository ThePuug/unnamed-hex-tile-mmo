use bevy::prelude::*;

use crate::common::{
        components::{behaviour::*, heading::*, offset::*, *}, 
        message::{Component, Event, *}, 
        resources::{map::*, *}, 
        systems::physics,
    };

pub fn tick(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    query: Query<(Entity, &Behaviour)>,
    dt: Res<Time>,
    mut buffers: ResMut<InputQueues>,
) {
    let dt = dt.delta().as_millis() as u16;

    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::KeyBits(keybits) }} = message else { continue };
        let Some(buffer) = buffers.get_mut(&ent) 
            // disconnect by client could remove buffer while message in transit
            else { warn!("no {ent} in buffers"); continue };
        let Some(input0) = buffer.queue.front() else { panic!("no front on buffer") };
        let Event::Input { seq: seq0, .. } = input0 else { panic!("not input") };
        let input = Event::Input { ent, key_bits: keybits, dt: 0, seq: seq0.wrapping_add(1) };
        buffer.queue.push_front(input); 
        writer.write(Try { event: input });
    }

    for (ent, &behaviour) in query {
        let Behaviour::Controlled = behaviour else { continue; };

        let Some(buffer) = buffers.get_mut(&ent) 
            // disconnect by client could remove buffer while message in transit
            else { warn!("no {ent} in buffers"); continue };
        let Some(input0) = buffer.queue.pop_front() else { panic!("no front on buffer") };
        let Event::Input { key_bits: keybits0, dt: dt0, seq: seq0, .. } = input0 else { panic!("not input") };

        let dt0 = dt0 + dt;
        buffer.queue.push_front(Event::Input { ent, key_bits: keybits0, dt: dt0, seq: seq0 });
        writer.write(Try { event: Event::Input { ent, key_bits: keybits0, dt, seq: seq0 }});
    }
}

pub fn apply(
    mut reader: EventReader<Do>,
    mut query: Query<(&Loc, &Heading, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Input { ent, dt, key_bits, .. } } = message {
            let Ok((&loc, &heading, mut offset, mut air_time)) = query.get_mut(ent)
                // disconnect by client could remove entity while message in transit
                else { warn!("no {ent} in query"); continue; };
            (offset.state, air_time.state) = physics::apply(key_bits, dt as i16, *loc, heading, offset.state, air_time.state, &map);
        }
    }
}
