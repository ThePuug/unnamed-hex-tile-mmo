use bevy::prelude::*;
use renet::{DefaultChannel, RenetClient};

use crate::{*, Event};

pub fn ui_input(
    mut conn: ResMut<RenetClient>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut client: ResMut<Client>,
) {
    if let Some(ent) = client.ent {
        let mut key_bits = default();
        if keyboard.any_pressed([KeyCode::ArrowUp, KeyCode::Lang3]) { key_bits |= KEYBIT_UP; }
        if keyboard.any_pressed([KeyCode::ArrowDown, KeyCode::NumpadEnter]) { key_bits |= KEYBIT_DOWN; }
        if keyboard.any_pressed([KeyCode::ArrowLeft, KeyCode::Convert]) { key_bits |= KEYBIT_LEFT; }
        if keyboard.any_pressed([KeyCode::ArrowRight, KeyCode::NonConvert]) { key_bits |= KEYBIT_RIGHT; }

        if key_bits != client.key_bits {
            client.key_bits = key_bits;
            let message = bincode::serialize(&Message::Try { event: Event::Input { ent, key_bits }}).unwrap();
            conn.send_message(DefaultChannel::ReliableOrdered, message);
        }
    }
}