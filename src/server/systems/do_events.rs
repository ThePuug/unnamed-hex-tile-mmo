use bevy::prelude::*;
use renet::*;

use crate::common::{
    components::message::{*, Event},
    hxpx::*,
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
            Event::Spawn { mut ent, typ, translation } => {
                ent = commands.spawn((
                    Transform::from_translation(translation), 
                    typ
                )).id();
                map.0.insert(Hx::from(translation), ent);
                let message = bincode::serialize(&Message::Do { event: Event::Spawn { ent, typ, translation }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
            _ => {
                warn!("Unexpected do event: {:?}", event);
            }
        }
    }
}