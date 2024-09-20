use bevy::prelude::*;
use renet::*;

use crate::{*, Event};

pub fn do_events(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Event>,
    mut map: ResMut<Map>,
) {
    for &event in reader.read() {
        match event {
            Event::Spawn { mut ent, typ, hx } => {
                let pos = Pos { hx, ..default() };
                ent = commands.spawn((
                    pos,
                    typ,
                    Transform {
                        translation: pos.into_screen(),
                        ..default()}, 
                )).id();
                map.0.insert(hx, ent);
                let message = bincode::serialize(&Message::Do { event: Event::Spawn { ent, typ, hx }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
            _ => {}
        }
    }
}