#![feature(more_float_constants)]

mod common;
mod server;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_renet::{
    renet::{
        transport::{ServerAuthentication, ServerConfig},
        ConnectionConfig, RenetServer,
    },
    transport::NetcodeServerPlugin,
    RenetServerPlugin,
};
use renet::{
    transport::{NetcodeServerTransport, NetcodeTransportError}, 
    DefaultChannel,
};

use common::{
    message::*,
    components::*,
    resources::map::*,
};
use server::{
    resources::{ *,
        map::*,
    },
    systems::{
        discover_tiles::*,
        do_manage_connections::*,
        do_events::*,
        physics::*,
    },
};

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(mut renet_error: EventReader<NetcodeTransportError>) {
    for e in renet_error.read() {
        panic!("{}", e);
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,bevy=warn,".to_owned()
                    +"server=trace,"
                    // +"server::common::input=info,"
                    ,
            custom_layer: |_| None,
        },
        TransformPlugin,
        RenetServerPlugin,
        NetcodeServerPlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Update, (
        panic_on_error_system,
        do_manage_connections,
        try_client_events,
        try_discover,
        try_move,
        update_positions,
        do_events,
    ));

    let (server, transport) = new_renet_server();
    app.init_resource::<Lobby>();
    app.init_resource::<TerrainedMap>();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.run();
}
