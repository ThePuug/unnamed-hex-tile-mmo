mod common;
mod server;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_renet::{
    renet::{
        transport::{ServerAuthentication, ServerConfig},
        ConnectionConfig, RenetServer, ServerEvent,
    },
    transport::NetcodeServerPlugin,
    RenetServerPlugin,
};
use renet::{
    transport::{NetcodeServerTransport, NetcodeTransportError}, 
    DefaultChannel,
};

use common::{
    components::prelude::{Event, *},
    input::*,
};
use server::resources::*;

const PROTOCOL_ID: u64 = 7;

fn new_renet_server() -> (RenetServer, NetcodeServerTransport) {
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

fn panic_on_error_system(mut renet_error: EventReader<NetcodeTransportError>) {
    for e in renet_error.read() {
        panic!("{}", e);
    }
}

fn do_manage_connections(
    mut server_events: EventReader<ServerEvent>,
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    mut lobby: ResMut<Lobby>,
    mut query: Query<&Transform>,
    ) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let ent = commands.spawn(Transform::default()).id();
                let message = bincode::serialize(&Message::Do { event: Event::Spawn { 
                    ent, 
                    typ: EntityType::Player, 
                    translation: Vec3::ZERO 
                }}).unwrap();
                server.broadcast_message(DefaultChannel::ReliableOrdered, message);
                for (_, &ent) in lobby.0.iter() {
                    let transform = query.get_mut(ent).unwrap();
                    let message = bincode::serialize(&Message::Do { event: Event::Spawn { 
                        ent, 
                        typ: EntityType::Player, 
                        translation: transform.translation,
                    }}).unwrap();
                    server.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.0.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.0.remove(&client_id).unwrap();
                commands.entity(ent).despawn();
                let message = bincode::serialize(&Message::Do { event: Event::Despawn { ent }}).unwrap();
                server.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
        }
    }
 }

 fn try_client_events(
    mut server: ResMut<RenetServer>,
    mut commands: Commands,
    lobby: Res<Lobby>,
 ) {
    for client_id in server.clients_id() {
        while let Some(serialized) = server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let message = bincode::deserialize(&serialized).unwrap();
            match message {
                Message::Try { event } => {
                    match event {
                        Event::Input { ent: _, key_bits } => {
                            if let Some(&ent) = lobby.0.get(&client_id) {
                                if let Some(mut commands) = commands.get_entity(ent) {
                                    commands.insert(key_bits);
                                    let message = bincode::serialize(&Message::Do { event: Event::Input { ent, key_bits } }).unwrap();
                                    server.broadcast_message(DefaultChannel::ReliableOrdered, message);
                                }
                            }
                        }
                        _ => {
                            warn!("Unexpected try event: {:?}", event);
                        }
                    }
                }
                Message::Do { event } => {
                    warn!("Unexpected do event: {:?}", event);
                }
            }
        }
    }
 }

fn main() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,bevy=warn,".to_owned()
                    +"server=info,"
                    +"server::common::input=info,"
                    ,
            custom_layer: |_| None,
        },
        TransformPlugin,
        RenetServerPlugin,
        NetcodeServerPlugin,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        do_manage_connections,
        try_client_events,
        handle_input,
    ));

    let (server, transport) = new_renet_server();

    app.init_resource::<Lobby>();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.run();
}
