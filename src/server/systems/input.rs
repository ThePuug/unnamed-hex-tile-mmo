use bevy::prelude::*;

use crate::{ *,
    common::{
        components::keybits::*,
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
                // trace!("ent({}) expect kb({}), seq({})", ent, key_bits.key_bits, seq);
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
                        // trace!("ent({}) sent kb({}), seq({}) for dt({})", ent, key_bits.key_bits, seq, dt);
                    }
                    _ => unreachable!()
                }
            }
            _ => {}
        }
    }
}