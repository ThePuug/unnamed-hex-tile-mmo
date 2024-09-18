use bevy::prelude::*;
use renet::*;

use crate::{*, Event};

pub fn try_client_events(
    mut server: ResMut<RenetServer>,
    mut commands: Commands,
    lobby: Res<Lobby>,
 ) {
    for client_id in server.clients_id() {
        while let Some(serialized) = server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let message = bincode::deserialize(&serialized).unwrap();
            match message {
                Message::Try { event } => {
                    match event {
                        Event::Input { ent: _, key_bits } => {
                            if let Some(&ent) = lobby.0.get(&client_id) {
                                if let Some(mut commands) = commands.get_entity(ent) {
                                    commands.insert(key_bits);
                                    let message = bincode::serialize(&Message::Do { event: Event::Input { ent, key_bits } }).unwrap();
                                    server.broadcast_message(DefaultChannel::ReliableOrdered, message);
                                }
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