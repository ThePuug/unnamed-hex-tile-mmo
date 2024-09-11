mod common;
mod client;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_renet::{
    client_connected,
    renet::{
        transport::ClientAuthentication,
        ConnectionConfig, DefaultChannel, RenetClient,
    },
    transport::NetcodeClientPlugin,
    RenetClientPlugin,
};
use renet::transport::{NetcodeClientTransport, NetcodeTransportError};

use common::{
    components::{*, Event},
    hxpx::*,
    input::*
};
use client::{
    resources::*,
    input::*,
};

const PROTOCOL_ID: u64 = 7;

fn new_renet_client() -> (RenetClient, NetcodeClientTransport) {
    let server_addr = "127.0.0.1:5000".parse().unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let client_id = current_time.as_millis() as u64;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };

    let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();
    let client = RenetClient::new(ConnectionConfig::default());

    (client, transport)
}

fn setup(
    mut commands: Commands,
) {
    commands.spawn(Camera2dBundle::default());
}

fn do_server_events(
    mut conn: ResMut<RenetClient>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut client: ResMut<Client>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let message = bincode::deserialize(&serialized).unwrap();
        match message {
            Message::Do { event } => {
                match event {
                    Event::Spawn { ent, typ, pos } => {
                        debug!("Spawn {{ {}, {:?}, {:?} }}", ent, typ, pos);

                        let (texture, layout, texture_atlas_layout);
                        let pos_px = Px::from(pos);
                        match typ {
                            EntityType::Player => {
                                texture = asset_server.load("sprites/blank.png");
                                layout = TextureAtlasLayout::from_grid(UVec2{x:32,y:44}, 4, 3, None, None);
                                texture_atlas_layout = texture_atlas_layouts.add(layout);
                            }
                        }

                        let ent = commands
                            .get_or_spawn(ent)
                            .insert((
                                SpriteBundle {
                                    texture,
                                    transform: Transform::from_xyz(pos_px.x.into(),pos_px.y.into(),pos_px.z.into()),
                                    ..default()
                                },
                                TextureAtlas {
                                    layout: texture_atlas_layout,
                                    index: 0,
                                },
                                Input { keys: 0 }
                            )).id();
                        if client.ent == None { 
                            client.ent = Some(ent); 
                            debug!("Player {} is the local player", ent);
                        }
                    }
                    Event::Despawn { ent } => {
                        debug!("Player {} disconnected", ent);
                        commands.entity(ent).despawn();
                    }
                    Event::Input { ent, input } => {
                        trace!("Player {} input: {:?}", ent, input);
                        commands.entity(ent).insert(input);
                    }
                }
            }
            Message::Try { event } => {
                warn!("Unexpected try event: {:?}", event);
            }
        }
    }
}

fn panic_on_error_system(
    mut renet_error: EventReader<NetcodeTransportError>
) {
    for e in renet_error.read() {
        panic!("{}", e);
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins
        .set(ImagePlugin::default_nearest())
        .set(AssetPlugin {
            file_path: "../assets".into(),
            ..default()
        })
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            filter: "wgpu=error,bevy=warn,naga=warn".to_string(),
            custom_layer: |_| None,
        })
    );
    app.init_resource::<Client>();

    app.add_plugins(RenetClientPlugin);
    app.add_plugins(NetcodeClientPlugin);
    let (client, transport) = new_renet_client();
    app.insert_resource(client);
    app.insert_resource(transport);

    app.add_systems(Startup, setup);
    app.add_systems(Update, ui_input);
    app.add_systems(Update, (do_server_events, handle_input).run_if(client_connected));
    app.add_systems(Update, panic_on_error_system);

    trace!("Starting client...");
    app.run();
}