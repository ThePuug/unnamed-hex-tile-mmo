use bevy::prelude::*;
use renet::*;

use crate::{*, Event};

pub fn try_client_events(
    mut conn: ResMut<RenetServer>,
    mut writer: EventWriter<Event>,
    lobby: Res<Lobby>,
 ) {
    for client_id in conn.clients_id() {
        while let Some(serialized) = conn.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let message = bincode::deserialize(&serialized).unwrap();
            match message {
                Message::Try { event } => {
                    match event {
                        Event::Input { ent: _, key_bits, dt } => {
                            if let Some(&ent) = lobby.0.get(&client_id) {
                                writer.send(Event::Input { ent, key_bits, dt });
                            }
                        }
                        _ => {
                            debug!("Unexpected try event: {:?}", event);
                        }
                    }
                }
                Message::Do { event } => {
                    warn!("Unexpected do event: {:?}", event);
                }
            }
        }
    }
 }