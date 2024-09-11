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
    components::{*, Event},
    input::*,
    hxpx::*,
};
use server::resources::*;

const PROTOCOL_ID: u64 = 7;

fn do_manage_connections(
    mut server_events: EventReader<ServerEvent>,
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    mut lobby: ResMut<Lobby>,
    ) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let pos = Hx { q: 0, r: 0, z: 0 };
                let player = Player { 
                    id: *client_id,
                    pos
                };
                let ent = commands.spawn(player).id();
                let message = bincode::serialize(&Message::Do { event: Event::Spawn { ent, typ: EntityType::Player, pos }}).unwrap();
                server.broadcast_message(DefaultChannel::ReliableOrdered, message);
                lobby.clients.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.clients.remove(&client_id).unwrap();
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
                        Event::Input { ent, input } => {
                            if let Some(client) = lobby.clients.get(&client_id) {
                                if let Some(mut entity) = commands.get_entity(ent) {
                                    if client == &ent {
                                        trace!("Player {} input: {:?}", ent, input);
                                        entity.insert(input);
                                        let message = bincode::serialize(&Message::Do { event }).unwrap();
                                        server.broadcast_message(DefaultChannel::ReliableOrdered, message);
                                    }
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

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(LogPlugin {
        level: bevy::log::Level::DEBUG,
        filter: "wgpu=error,bevy=warn".to_string(),
        custom_layer: |_| None,
    });
    app.init_resource::<Lobby>();

    app.add_plugins(RenetServerPlugin);
    app.add_plugins(NetcodeServerPlugin);
    let (server, transport) = new_renet_server();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.add_systems(Update,(do_manage_connections, try_client_events, handle_input).run_if(resource_exists::<RenetServer>));
    app.add_systems(Update, panic_on_error_system);

    debug!("starting server");
    app.run();
}
