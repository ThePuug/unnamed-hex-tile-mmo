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
use ::renet::DefaultChannel;

use common::{
    message::*, 
    plugins::nntree, 
    resources::map::*, 
    systems::physics,
};
use server::{
    resources::{ *, terrain::* },
    systems::{ world, actor, input, renet },
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
        nntree::NNTreePlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        world::setup,
    ));

    app.add_systems(PreUpdate, (
        renet::write_try,
    ));

    app.add_systems(FixedUpdate, (
        input::generate_input,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        actor::try_discover,
        actor::try_incremental,
        input::do_input,
        input::try_gcd,
        input::try_input,
        input::update_qrz,
        physics::do_incremental,
        physics::update_heading,
        renet::do_manage_connections,
        world::do_spawn,
    ));

    app.add_systems(PostUpdate, (
        renet::send_do,
    ));

    let (server, transport) = renet::new_renet_server();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(Map::new(qrz::Map::<Entity>::new(1., 0.8)));

    app.init_resource::<Lobby>();
    app.init_resource::<InputQueues>();
    app.init_resource::<Terrain>();
    app.init_resource::<RunTime>();

    app.run();
}
