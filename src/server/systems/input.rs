use bevy::prelude::*;

use crate::{ *,
    common::{
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        message::{*, Event},
    },
};

pub fn generate_input(
    mut writer: EventWriter<Do>,
    time: Res<Time>,
    query: Query<Entity, With<Actor>>,
    mut queues: ResMut<InputQueues>,
) {
    for ent in query.iter() {
        let dt = (time.delta_seconds() * 1000.) as u16;
        let queue = queues.0.get_mut(&ent).unwrap();
        match queue.0.pop_front().unwrap() {
            Event::Input { key_bits, dt: dt0, seq, .. } => { 
                writer.send(Do { event: Event::Input { ent, key_bits, dt, seq: 0 } }); 
                queue.0.push_front(Event::Input { ent, key_bits, dt: dt0+dt, seq });
            }
            _ => unreachable!()
        }
    }
}

pub fn do_input(
    mut reader: EventReader<Do>,
    mut query: Query<(&Heading, &Hx, &mut Offset, &mut AirTime)>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Input { ent, key_bits, dt, .. } } => {
                let (&heading, &hx, mut offset, mut air_time) = query.get_mut(ent).unwrap();
                (offset.state, air_time.state) = apply(key_bits, dt as i16, heading, hx, offset.state, air_time.state, &map);
            },
            _ => {}
        }
    }
}

pub fn try_input(
    mut reader: EventReader<Try>,
    mut query: Query<&mut KeyBits>,
    mut conn: ResMut<RenetServer>,
    mut queues: ResMut<InputQueues>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits, seq, .. } } => {
                let queue = queues.0.get_mut(&ent).unwrap();
                queue.0.push_back(Event::Input { ent, key_bits, dt: 0, seq });
                *query.get_mut(ent).unwrap() = key_bits;

                match queue.0.pop_front().unwrap() {
                    Event::Input { key_bits, dt, seq, .. } => {
                        conn.send_message(*lobby.0.get_by_right(&ent).unwrap(), 
                            DefaultChannel::ReliableOrdered, 
                            bincode::serialize(&Do { event: Event::Input { 
                                ent,
                                key_bits, 
                                dt,
                                seq,
                            }}).unwrap());
                    }
                    _ => unreachable!()
                }
            }
            _ => {}
        }
    }
}