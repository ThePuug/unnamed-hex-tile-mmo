use bevy::prelude::*;
use renet::*;

use crate::{*,
    common::message::{*, Event},
};

pub fn do_events(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    mut map: ResMut<TerrainedMap>,
) {
    for &message in reader.read() {
        trace!("Message: {:?}", message);
        match message {
            Do { event: Event::Spawn { mut ent, typ, hx } } => {
                ent = commands.spawn((
                    hx,
                    Offset::default(),
                    typ,
                    Transform {
                        translation: (hx,Offset::default()).into_screen(),
                        ..default()}, 
                )).id();
                map.insert(hx, ent);
                let message = bincode::serialize(&Do { event: Event::Spawn { ent, typ, hx }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
            Do { event: Event::Move { ent, hx, heading } } => {
                let message = bincode::serialize(&Do { event: Event::Move { ent, hx, heading }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
            _ => {}
        }
    }
}