use bevy::prelude::*;
use renet::ServerEvent;

use crate::{*,
    common::{
        message::{*, Event},
        components::{
            hx::*,
            keybits::*,
        }
    },
};

pub fn new_renet_server() -> (RenetServer, NetcodeServerTransport) {
    let public_addr = "0.0.0.0:5000".parse().unwrap();
    let socket = UdpSocket::bind(public_addr).unwrap();
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let server_config = ServerConfig {
        current_time,
        max_clients: 64,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![public_addr],
        authentication: ServerAuthentication::Unsecure,
    };

    let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
    let server = RenetServer::new(ConnectionConfig::default());

    (server, transport)
}

pub fn do_manage_connections(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<ServerEvent>,
    mut lobby: ResMut<Lobby>,
    query: Query<(&Hx, &EntityType)>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let hx = Hx { q: 0, r: 0, z: 4 };
                let typ = EntityType::Actor;
                let ent = commands.spawn((
                    Transform::default(),
                    KeyBits::default(),
                    Heading::default(),
                    Offset::default(),
                    typ,
                    hx, 
                )).id();
                let message = bincode::serialize(&Do { event: Event::Spawn { ent, typ, hx }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
                for (_, &ent) in lobby.0.iter() {
                    let (&hx, &typ) = query.get(ent).unwrap();
                    let message = bincode::serialize(&Do { event: Event::Spawn { typ, ent, hx }}).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.0.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.0.remove(&client_id).unwrap();
                commands.entity(ent).despawn();
                let message = bincode::serialize(&Do { event: Event::Despawn { ent }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
        }
    }
 }

pub fn write_try(
    mut writer: EventWriter<Try>,
    mut conn: ResMut<RenetServer>,
    lobby: Res<Lobby>,
) {
    for client_id in conn.clients_id() {
        while let Some(serialized) = conn.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let message = bincode::deserialize(&serialized).unwrap();
            // trace!("Message: {:?}", message);
            match message {
                Try { event: Event::Input { key_bits, dt, .. } } => {
                    if let Some(&ent) = lobby.0.get(&client_id) {
                        writer.send(Try { event: Event::Input { ent, key_bits, dt }});
                    }
                }
                Try { event: Event::Discover { hx, .. } } => { 
                    if let Some(&ent) = lobby.0.get(&client_id) {
                        writer.send(Try { event: Event::Discover { ent, hx }});
                    }
                }
                _ => {}
            }
        }
    }
 }

 pub fn broadcast_do(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    mut map: ResMut<Map>,
) {
    for &message in reader.read() {
        // trace!("Message: {:?}", message);
        match message {
            Do { event: Event::Spawn { mut ent, typ, hx } } => {
                ent = commands.spawn((
                    hx,
                    Offset::default(),
                    typ,
                    Transform {
                        translation: (hx,Offset::default()).calculate(),
                        ..default()}, 
                )).id();
                map.insert(hx, ent);
                let message = bincode::serialize(&Do { event: Event::Spawn { ent, typ, hx }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
            Do { event: Event::Move { ent, hx, heading } } => {
                let message = bincode::serialize(&Do { event: Event::Move { ent, hx, heading }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
            _ => {}
        }
    }
}