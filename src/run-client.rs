#![feature(let_chains)]
#![feature(more_float_constants)]

mod common;
mod client;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{
    diagnostic::*,
    log::LogPlugin, 
    prelude::*,
    render::diagnostic::*,
};
use bevy_easings::*;
use bevy_renet::{
    renet::*,
    netcode::*,
    *,
};
use iyes_perf_ui::PerfUiPlugin;

use common::{
    components::{ keybits::*, * }, 
    message::{ Event, * }, 
    plugins::nntree, 
    resources::{ map::*,  * },
    systems::physics
};
use client::{
    resources::*, 
    systems::{ actor, animator, camera, input, renet, * }
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
        FrameTimeDiagnosticsPlugin,
        EntityCountDiagnosticsPlugin,
        SystemInformationDiagnosticsPlugin,
        RenderDiagnosticsPlugin,
        PerfUiPlugin
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        setup,
        actor::setup,
        camera::setup,
        renet::setup,
        ui::setup.after(camera::setup),
        world::setup,
    ));

    app.add_systems(PreUpdate, (
        renet::write_try,
        input::update_keybits,
        input::generate_input.after(input::update_keybits),
        ui::update,
        world::do_init,
    ));

    app.add_systems(FixedUpdate, (
        actor::do_spawn,
        input::do_input,
        physics::do_incremental,
        world::do_spawn,
        world::update,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        actor::try_gcd,
        actor::update,
        animator::update,
        camera::update,
        input::try_input,
        physics::update_heading,
    ));

    app.add_systems(PostUpdate, (
        renet::send_do,
    ));

    let mut buffer = InputQueue::default();
    buffer.queue.push_front(Event::Input { ent: Entity::PLACEHOLDER, key_bits: KeyBits::default(), dt: 0, seq: 1 });
    app.insert_resource(buffer);

    app.insert_resource(Map::new(qrz::Map::<Entity>::new(1., 0.8)));

    app.init_resource::<EntityMap>();
    app.init_resource::<Server>();

    app.run();
}
