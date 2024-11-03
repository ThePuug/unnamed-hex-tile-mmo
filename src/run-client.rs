#![feature(let_chains)]
#![feature(more_float_constants)]

mod common;
mod client;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_easings::*;
use bevy_renet::{
    renet::{
        transport::ClientAuthentication,
        ConnectionConfig,
    },
    transport::NetcodeClientPlugin,
    RenetClientPlugin,
};
use renet::transport::{NetcodeClientTransport, NetcodeTransportError};

use common::{
    message::*,
    components::{ *,
    },
    resources::{ *, 
        map::*,
    },
    systems::{
        input::*,
        physics::*,
    },
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
                    +"client=trace,"
                    ,
            custom_layer: |_| None,
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
        EasingsPlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, setup);
    app.add_systems(Update, (
        panic_on_error_system,
        generate_input,
        write_do,
        do_input,
        do_move,
        send_try,
        update_animations,
        update_camera,
        update_headings,
        update_offsets,
        update_transforms,
        update_keybits,
    ));

    let (client, transport) = new_renet_client();

    app.init_resource::<EntityMap>();
    app.init_resource::<Map>();
    app.init_resource::<InputQueue>();
    app.insert_resource(client);
    app.insert_resource(transport);

    app.run();
}
