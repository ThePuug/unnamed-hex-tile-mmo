use bevy::prelude::*;
use bevy_renet::netcode::{ServerAuthentication, ServerConfig};
use renet::ServerEvent;

use crate::{*,
    common::{
        message::{*, Event},
        plugins::nntree::*,
        components::{ *,
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        resources::*,
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
    mut queues: ResMut<InputQueues>,
    query: Query<(&Hx, &EntityType)>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let hx = Hx { q: 0, r: 0, z: 4 };
                let typ = EntityType::Actor;
                let ent = commands.spawn((
                    AirTime { state: Some(0), step: None },
                    Actor,
                    Transform::default(),
                    KeyBits::default(),
                    Heading::default(),
                    Offset::default(),
                    typ,
                    hx, 
                    NearestNeighbor::default(),
                )).id();
                let mut queue = InputQueue::default();
                queue.0.push_back(Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 });
                queues.0.insert(ent, queue);
                let message = bincode::serde::encode_to_vec(Do { event: Event::Spawn { ent, typ, hx }}, bincode::config::legacy()).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
                for (_, &ent) in lobby.0.iter() {
                    let (&hx, &typ) = query.get(ent).unwrap();
                    let message = bincode::serde::encode_to_vec(Do { event: Event::Spawn { typ, ent, hx }}, bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.0.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.0.remove_by_left(client_id).unwrap().1;
                queues.0.remove(&ent);
                commands.entity(ent).despawn();
                let message = bincode::serde::encode_to_vec(Do { event: Event::Despawn { ent }}, bincode::config::legacy()).unwrap();
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
            let (message, _) = bincode::serde::borrow_decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
            match message {
                Try { event: Event::Input { key_bits, dt, seq, .. } } => {
                    if let Some(&ent) = lobby.0.get_by_left(&client_id) {
                        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq }});
                    }
                }
                Try { event: Event::Gcd { typ, .. } } => {
                    if let Some(&ent) = lobby.0.get_by_left(&client_id) {
                        writer.send(Try { event: Event::Gcd { ent, typ }});
                    }
                }
                _ => {}
            }
        }
    }
 }

 pub fn send_do(
    mut commands: Commands,
    query: Query<&Hx>,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    mut map: ResMut<Map>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Spawn { mut ent, typ, hx } } => {
                ent = commands.spawn((
                    hx,
                    Offset::default(),
                    typ,
                    Transform {
                        translation: (hx,Vec3::ZERO).calculate(),
                        ..default()}, 
                )).id();
                map.insert(hx, ent);
                conn.broadcast_message(DefaultChannel::ReliableOrdered, 
                    bincode::serde::encode_to_vec(Do { event: Event::Spawn { ent, typ, hx }}, bincode::config::legacy()).unwrap());
            }
            Do { event: Event::Incremental { ent, attr } } => {
                let &hx = query.get(ent).unwrap();
                for other in nntree.0.within_unsorted_iter::<Hexhattan>(&hx.into(), 20_i16.into()) {
                    let message = bincode::serde::encode_to_vec(Do { event: Event::Incremental { ent, attr }}, bincode::config::legacy()).unwrap();
                    conn.send_message(*lobby.0.get_by_right(&Entity::from_bits(other.item)).unwrap(), DefaultChannel::ReliableOrdered, message);
                }
            }
            _ => {}
        }
    }
}