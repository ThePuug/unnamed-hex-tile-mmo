#![feature(let_chains)]
#![feature(more_float_constants)]

mod common;
mod client;

use std::time::{Duration, SystemTime};
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_renet::{
    renet::{
        transport::ClientAuthentication,
        ConnectionConfig, RenetClient,
    },
    transport::NetcodeClientPlugin,
    RenetClientPlugin,
};
use renet::transport::{NetcodeClientTransport, NetcodeTransportError};

use common::{
    components::{ *,
        keybits::*, 
        message::Event,
    },
    resources::map::*,
    systems::handle_input::*,
};
use client::{
    components::animationconfig::*,
    resources::*,
    systems::{
        renet::*,
        input::*,
        sprites::*,
    },
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

fn panic_on_error_system(
    mut renet_error: EventReader<NetcodeTransportError>
) {
    for e in renet_error.read() {
        panic!("{}", e);
    }
}

fn setup(
    mut commands: Commands,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Camera2dBundle::default(),
        Actor
    ));
    commands.insert_resource(TextureHandles {
        actor: (
            asset_server.load("sprites/blank.png"),
            texture_atlas_layouts.add(TextureAtlasLayout::from_grid(UVec2{x:32,y:44}, 4, 3, None, None))
        ),
        decorator: (
            asset_server.load("sprites/biomes.png"),
            texture_atlas_layouts.add(TextureAtlasLayout::from_grid(UVec2{x:83,y:136}, 7, 1, None, None))
        ),
    });
}

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins
        .set(ImagePlugin::default_nearest())
        .set(AssetPlugin {
            file_path: "../assets".into(),
            ..default()
        })
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,bevy=warn,naga=warn,polling=warn,winit=warn,".to_owned()
                    +"client=debug,"
                    +"client::common::input=trace,"
                    +"client::client::systems=trace,"
                    ,
            custom_layer: |_| None,
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
    ));

    app.add_event::<Event>();

    app.add_systems(Startup, setup);
    app.add_systems(Update, (
        panic_on_error_system,
        ui_input,
        do_server_events,
        handle_input,
        update_animations,
        update_transforms,
        try_events,
        camera,
    ));

    let (client, transport) = new_renet_client();

    app.init_resource::<EntityMap>();
    app.init_resource::<Map>();
    app.insert_resource(client);
    app.insert_resource(transport);

    app.run();
}
