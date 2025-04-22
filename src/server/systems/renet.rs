use bevy::prelude::*;
use bevy_renet::netcode::{ServerAuthentication, ServerConfig};
use qrz::Convert;
use renet::ServerEvent;

use crate::{*,
    common::{
        message::{*, Event},
        plugins::nntree::*,
        components::{ *,
            heading::*,
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

#[allow(clippy::too_many_arguments)]
pub fn do_manage_connections(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<ServerEvent>,
    mut lobby: ResMut<Lobby>,
    mut buffers: ResMut<InputQueues>,
    query: Query<(&Loc, &EntityType)>,
    time: Res<Time>,
    runtime: Res<RunTime>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let ent = commands.spawn((
                    Actor,
                    EntityType::Actor,
                    Loc::from_qrz(0, 0, 4), 
                    AirTime { state: Some(0), step: None },
                    Transform::default(),
                    KeyBits::default(),
                    Heading::default(),
                    Offset::default(),
                    NearestNeighbor::default(),
                )).id();
                let mut buffer = InputQueue::default();
                buffer.queue.push_back(Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 });
                buffers.insert(ent, buffer);

                // init client
                let dt = time.elapsed().as_millis() + runtime.elapsed_offset;
                let message = bincode::serde::encode_to_vec(Do { event: Event::Init { ent, dt }}, bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                // spawn new actor everywhere
                let message = bincode::serde::encode_to_vec(Do { event: Event::Spawn { ent, typ: EntityType::Actor, qrz: *Loc::from_qrz(0, 0, 4) }}, bincode::config::legacy()).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);

                // spawn lobby on client
                for (_, &ent) in lobby.iter() {
                    let (&loc, &typ) = query.get(ent).unwrap();
                    let message = bincode::serde::encode_to_vec(Do { event: Event::Spawn { typ, ent, qrz: *loc }}, bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.remove_by_left(client_id).unwrap().1;
                buffers.remove(&ent);
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
                    if let Some(&ent) = lobby.get_by_left(&client_id) {
                        writer.send(Try { event: Event::Input { ent, key_bits, dt, seq }});
                    }
                }
                Try { event: Event::Gcd { typ, .. } } => {
                    if let Some(&ent) = lobby.get_by_left(&client_id) {
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
    query: Query<&Loc>,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    mut map: ResMut<Map>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Spawn { mut ent, typ, qrz } } => {
                ent = commands.spawn((
                    Loc::new(qrz),
                    Offset::default(),
                    typ,
                    Transform {
                        translation: map.convert(qrz),
                        ..default()}, 
                )).id();
                map.insert(qrz, ent);
                for other in nntree.within_unsorted_iter::<Hexhattan>(&Loc::new(qrz).into(), 20_i16.into()) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Spawn { ent, typ, qrz }}, 
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*lobby.get_by_right(&Entity::from_bits(other.item)).unwrap(), DefaultChannel::ReliableOrdered, message);
                }
            }
            Do { event: Event::Incremental { ent, attr } } => {
                let &qrz = query.get(ent).unwrap();
                for other in nntree.within_unsorted_iter::<Hexhattan>(&qrz.into(), 20_i16.into()) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Incremental { ent, attr }}, 
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*lobby.get_by_right(&Entity::from_bits(other.item)).unwrap(), DefaultChannel::ReliableOrdered, message);
                }
            }
            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                // seq 0 is the currently accumulating input
                // we still execute input as it accumulate every STEP, but
                // do not confirm the input until new input is received for accumulation
                if seq == 0 { continue; }
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Input { ent, key_bits, dt, seq }}, 
                    bincode::config::legacy()).unwrap();
                conn.send_message(*lobby.get_by_right(&ent).unwrap(), DefaultChannel::ReliableOrdered, message);
            }
            _ => {}
        }
    }
}