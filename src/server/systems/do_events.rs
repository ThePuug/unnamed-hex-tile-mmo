use bevy::prelude::*;
use renet::*;

use crate::common::{
    components::{ *,
        message::{Event, *},
    },
    resources::map::*,
};

pub fn do_events(
    mut conn: ResMut<RenetServer>,
    mut events: EventReader<Event>,
    mut commands: Commands,
    mut map: ResMut<Map>,
) {
    for &event in events.read() {
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
            _ => {
                warn!("Unexpected do event: {:?}", event);
            }
        }
    }
}