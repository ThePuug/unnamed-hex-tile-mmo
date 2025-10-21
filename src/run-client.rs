#![feature(let_chains)]
#![feature(more_float_constants)]
#![feature(extend_one)]

mod common;
mod client;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{
    log::LogPlugin,
    prelude::*,
};
use bevy_easings::*;
use bevy_renet::{
    renet::*,
    netcode::*,
    *,
};

use common::{
    components::{entity_type::*, *}, 
    message::*, 
    plugins::nntree, 
    resources::{ map::*,  * },
    systems::physics,
};
use client::{
    plugins::diagnostics::DiagnosticsPlugin,
    resources::*,
    systems::{actor, animator, camera, input, renet, target_cursor, world, ui}
};

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(
    mut renet_error: EventReader<NetcodeTransportError>
) {
    if let Some(e) = renet_error.read().next() {
        panic!("{:?}", e);
    }
}

fn setup(
    mut config_store: ResMut<GizmoConfigStore>,
) {
    let (_, light_config) = config_store.config_mut::<LightGizmoConfigGroup>();
    light_config.draw_all = false;
    light_config.color = LightGizmoColor::MatchLightColor;
}

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,naga=warn,polling=warn,winit=warn,offset_allocator=warn,gilrs=warn,".to_owned()
                    +"bevy=warn,cosmic_text=warn,client=trace,"
                    ,
            custom_layer: |_| None,
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        DiagnosticsPlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        setup,
        actor::setup,
        camera::setup,
        renet::setup,
        target_cursor::setup,
        ui::setup.after(camera::setup),
        world::setup,
    ));

    app.add_systems(PreUpdate, (
        input::update_keybits,
        renet::write_do,
    ));

    app.add_systems(FixedUpdate, (
        common::systems::behaviour::controlled::apply,
        common::systems::behaviour::controlled::tick,
        common::systems::behaviour::controlled::interpolate_remote,
        physics::update,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        actor::do_spawn,
        actor::try_gcd,
        actor::update,
        animator::update,
        camera::update,
        common::systems::world::try_incremental,
        common::systems::world::do_incremental,
        input::do_input,
        target_cursor::update,
        ui::update,
        world::async_spawn,
        world::async_ready,
        world::do_init,
        world::do_spawn,
        world::update,
    ));

    app.add_systems(PostUpdate, (
        renet::send_try,
    ));

    app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8)));

    app.init_resource::<InputQueues>();
    app.init_resource::<EntityMap>();
    app.init_resource::<Server>();

    app.run();
}
