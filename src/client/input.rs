use bevy::prelude::*;
use renet::{DefaultChannel, RenetClient};

use crate::{
    common::{
        components::prelude::{*, Event},
        input::*,
    },
    client::resources::*,
};

pub fn ui_input(
    mut conn: ResMut<RenetClient>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut client: ResMut<Client>,
) {
    if let Some(ent) = client.ent {
        let mut key_bits = default();
        if keyboard.pressed(KeyCode::ArrowUp) { key_bits |= KEYBIT_UP; }
        if keyboard.pressed(KeyCode::ArrowDown) { key_bits |= KEYBIT_DOWN; }
        if keyboard.pressed(KeyCode::ArrowLeft) { key_bits |= KEYBIT_LEFT; }
        if keyboard.pressed(KeyCode::ArrowRight) { key_bits |= KEYBIT_RIGHT; }

        if key_bits != client.key_bits {
            client.key_bits = key_bits;
            trace!("New input: {:?}", client.key_bits);
            let message = bincode::serialize(&Message::Try { event: Event::Input { ent, key_bits }}).unwrap();
            conn.send_message(DefaultChannel::ReliableOrdered, message);
        }
    }
}
