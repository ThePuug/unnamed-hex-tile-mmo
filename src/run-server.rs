#![feature(more_float_constants)]

mod common;
mod server;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_easings::*;
use bevy_renet::{
    renet::{ConnectionConfig, RenetServer},
    netcode::{NetcodeServerTransport, NetcodeTransportError, NetcodeServerPlugin},
    RenetServerPlugin,
};
use kiddo::fixed::kdtree::KdTree;
use renet::DefaultChannel;

use common::{
    components::*, 
    message::*, 
    plugins::nntree, 
    resources::map::*, 
    systems::physics::*
};
use server::{
    resources::{ *, terrain::* },
    systems::{
        actor::*,
        input::*,
        renet::*,
    },
};

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(mut renet_error: EventReader<NetcodeTransportError>) {
    if let Some(e) = renet_error.read().next() {
        panic!("{:?}", e);
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
                    ,
            custom_layer: |_| None,
        },
        TransformPlugin,
        RenetServerPlugin,
        NetcodeServerPlugin,
        EasingsPlugin::default(),
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Update, (
        panic_on_error_system,
        send_do,
        do_gcd,
        do_incremental,
        do_input,
        do_manage_connections,
        try_discover,
        try_gcd,
        try_incremental,
        try_input,
        update_headings,
        update_offsets,
        write_try,
        nntree::update,
    ));

    app.add_systems(FixedUpdate, (
        generate_input,
    ));

    let (server, transport) = new_renet_server();
    app.insert_resource(server);
    app.insert_resource(transport);

    let kdtree = nntree::NNTree(KdTree::with_capacity(1_000_000));
    app.insert_resource(kdtree);

    app.init_resource::<Lobby>();
    app.init_resource::<InputQueues>();
    app.init_resource::<Map>();
    app.init_resource::<Terrain>();

    app.run();
}
