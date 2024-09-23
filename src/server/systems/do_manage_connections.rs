use bevy::prelude::*;
use renet::ServerEvent;

use crate::{*,
    common::{
        message::{*, Event},
        components::hx::*,
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
    query: Query<&Hx>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let hx = Hx { q: 0, r: 0, z: 1 };
                let offset = Offset(Vec3::ZERO);
                let ent = commands.spawn((
                    Transform::default(),
                    Heading(Hx { q: 0, r: 0, z: 0 }),
                    hx, offset,
                )).id();
                let message = bincode::serialize(&Do { event: Event::Spawn { 
                    ent,
                    typ: EntityType::Actor, 
                    hx,
                }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
                for (_, &ent) in lobby.0.iter() {
                    let &hx = query.get(ent).unwrap();
                    let message = bincode::serialize(&Do { event: Event::Spawn { 
                        ent, 
                        typ: EntityType::Actor, 
                        hx,
                    }}).unwrap();
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

pub fn try_client_events(
    mut writer: EventWriter<Try>,
    mut conn: ResMut<RenetServer>,
    lobby: Res<Lobby>,
) {
    for client_id in conn.clients_id() {
        while let Some(serialized) = conn.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let message = bincode::deserialize(&serialized).unwrap();
            trace!("Message: {:?}", message);
            match message {
                Try { event: Event::Move { ent, hx, heading } } => {
                    if let Some(&cent) = lobby.0.get(&client_id) {
                        if cent == ent {
                            writer.send(Try { event: Event::Move { ent, hx, heading }});
                        }
                    }
                }
                Try { event: Event::Discover { hx } } => {
                    writer.send(Try { event: Event::Discover { hx } });
                }
                _ => {}
            }
        }
    }
 }