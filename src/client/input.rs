use bevy::prelude::*;
use renet::{DefaultChannel, RenetClient};

use crate::{
    common::components::{*, Event},
    client::resources::*,
};

pub fn ui_input(
    mut conn: ResMut<RenetClient>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut client: ResMut<Client>,
) {
    if let Some(ent) = client.ent {
        let mut keys = 0;
        if keyboard.pressed(KeyCode::KeyW) { keys = keys | (KeyBit::UP as u8); }
        if keyboard.pressed(KeyCode::KeyS) { keys = keys | (KeyBit::DOWN as u8); }
        if keyboard.pressed(KeyCode::KeyA) { keys = keys | (KeyBit::LEFT as u8); }
        if keyboard.pressed(KeyCode::KeyD) { keys = keys | (KeyBit::RIGHT as u8); }

        if keys != client.input.keys {
            client.input.keys = keys;
            trace!("New input: {:?}", client.input.keys);
        }

        let message = bincode::serialize(&Message::Try { event: Event::Input { ent, input: client.input }}).unwrap();
        conn.send_message(DefaultChannel::ReliableOrdered, message);
    }
}
