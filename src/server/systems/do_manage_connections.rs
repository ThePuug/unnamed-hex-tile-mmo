use bevy::prelude::*;
use renet::ServerEvent;

use crate::{*, Event,
    common::hx::*,
};

pub fn do_manage_connections(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<ServerEvent>,
    mut lobby: ResMut<Lobby>,
    map: Res<Map>,
    mut query: Query<&Pos>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let pos = Pos { hx: Hx { q: 0, r: 0, z: 1 }, offset: Vec3::ZERO };
                let ent = commands.spawn((
                    Transform::default(),
                    Heading(Hx { q: 0, r: 0, z: -1 }),
                    pos,
                )).id();
                let message = bincode::serialize(&Message::Do { event: Event::Spawn { 
                    ent,
                    typ: EntityType::Actor, 
                    hx: pos.hx,
                }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
                for (_, &ent) in lobby.0.iter() {
                    let pos = query.get_mut(ent).unwrap();
                    let message = bincode::serialize(&Message::Do { event: Event::Spawn { 
                        ent, 
                        typ: EntityType::Actor, 
                        hx: pos.hx,
                    }}).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.0.insert(*client_id, ent);
                for (&hx, &ent) in map.0.iter() {
                    let message = bincode::serialize(&Message::Do { event: Event::Spawn {
                        ent,
                        typ: EntityType::Decorator(DecoratorDescriptor { index: 1, is_solid: true }),
                        hx
                    }}).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.0.remove(&client_id).unwrap();
                commands.entity(ent).despawn();
                let message = bincode::serialize(&Message::Do { event: Event::Despawn { ent }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
        }
    }
 }
