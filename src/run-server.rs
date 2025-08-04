#![feature(more_float_constants)]
#![feature(extend_one)]

mod common;
mod server;

use std::{ net::UdpSocket, time::* };
use bevy::{ log::LogPlugin, prelude::*, time::common_conditions::* };
use bevy_easings::*;
use bevy_renet::{
    renet::{ConnectionConfig, RenetServer},
    netcode::{NetcodeServerTransport, NetcodeTransportError, NetcodeServerPlugin},
    RenetServerPlugin,
};
use ::renet::DefaultChannel;

use crate::{
    common::{
        components::entity_type::*, 
        message::*, 
        plugins::nntree, 
        resources::{map::*, *}, 
        systems::physics
    },
    server::{ 
        resources::{terrain::*, *}, 
        systems::{actor, input, renet, world}, 
        *
    }
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
    app.add_event::<Tick>();

    app.add_systems(Startup, (
        world::setup,
    ));

    app.add_systems(PreUpdate, (
        renet::write_try,
    ));

    app.add_systems(FixedUpdate, (
        common::systems::behaviour::controlled::apply,
        common::systems::behaviour::controlled::tick,
        server::systems::behaviour::follow::tick.run_if(on_timer(Duration::from_millis(1000))),
        server::systems::behaviour::follow::apply, //.run_if(on_timer(Duration::from_millis(125))),
        physics::update,
        common::systems::actor::update,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        actor::try_discover,
        actor::update,
        common::systems::world::try_incremental,
        common::systems::world::do_incremental,
        input::send_input,
        input::try_gcd,
        input::try_input,
        renet::do_manage_connections,
        world::do_spawn,
        world::try_spawn,
    ));

    app.add_systems(PostUpdate, (
        renet::send_do,
    ));

    let (server, transport) = renet::new_renet_server();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8)));

    app.init_resource::<Lobby>();
    app.init_resource::<InputQueues>();
    app.init_resource::<Terrain>();
    app.init_resource::<RunTime>();

    app.run();
}
