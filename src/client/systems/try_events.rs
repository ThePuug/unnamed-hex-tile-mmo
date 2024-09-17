use bevy::prelude::*;

use crate::{*, Event};

pub fn try_events(
    mut conn: ResMut<RenetClient>,
    mut events: EventReader<Event>,
) {
    for &event in events.read() {
        let message = bincode::serialize(&Message::Try { event }).unwrap();
        conn.send_message(DefaultChannel::ReliableOrdered, message);
    }
}