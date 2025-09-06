use bevy::prelude::*;
use bevy_renet::netcode::{ServerAuthentication, ServerConfig};
use qrz::*;
use ::renet::ServerEvent;

use crate::{ common::{
        components::{ *,
            behaviour::*,
            entity_type::{ *,
                actor::*,
            },
            keybits::*,
        }, 
        message::{ Component, Event, * }, 
        plugins::nntree::*, 
        resources::*
    }, *
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
    mut writer: EventWriter<Do>,
    mut lobby: ResMut<Lobby>,
    mut buffers: ResMut<InputQueues>,
    query: Query<(&Loc, &EntityType)>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    nntree: Res<NNTree>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let typ = EntityType::Actor(ActorImpl::new(
                    Origin::Starborn, 
                    Form::Humanoid, 
                    Manifestation::Physical));
                let qrz = Qrz { q: 0, r: 0, z: 4 };
                let loc = Loc::new(qrz);
                let ent = commands.spawn((
                    typ,
                    loc,
                    Behaviour::Controlled,
                )).id();
                commands.entity(ent).insert(NearestNeighbor::new(ent, loc));
                writer.write(Do { event: Event::Spawn { ent, typ, qrz }});

                // init input buffer for client
                buffers.extend_one((ent, InputQueue { 
                    queue: [Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 }].into() }));

                // init client
                let dt = time.elapsed().as_millis() + runtime.elapsed_offset;
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Init { ent, dt }}, 
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                // spawn nearby actors
                for other in nntree.locate_within_distance(loc, 20*20) {
                    let (&loc, &typ) = query.get(other.ent).unwrap();
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Spawn { typ, ent: other.ent, qrz: *loc }}, 
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.remove_by_left(&client_id).unwrap().1;
                buffers.remove(&ent);
                commands.entity(ent).despawn();
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Despawn { ent }}, 
                    bincode::config::legacy()).unwrap();
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
                Try { event: Event::Incremental { component: Component::KeyBits(keybits), .. } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::Incremental { ent, component: Component::KeyBits(keybits) }});
                }
                Try { event: Event::Gcd { typ, .. } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::Gcd { ent, typ }});
                }
                Try { event: Event::Spawn { ent, .. } } => {
                    writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default() }});
                }
                _ => {}
            }
        }
    }
 }

 pub fn send_do(
    query: Query<&Loc>,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Spawn { ent, typ, qrz } } => {
                for other in nntree.locate_within_distance(Loc::new(qrz), 20*20) {
                    let Some(client_id) = lobby.get_by_right(&other.ent) else { continue; };
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Spawn { ent, typ, qrz }}, 
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            Do { event: Event::Incremental { ent, component } } => {
                match component {
                    Component::KeyBits(_) => continue,
                    _ => {}
                }
                let &loc = query.get(ent).unwrap();
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Incremental { ent, component }}, 
                            bincode::config::legacy()).unwrap();                        
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            _ => {}
        }
    }
}