#![feature(let_chains)]
#![feature(more_float_constants)]
#![feature(extend_one)]

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
    components::{entity_type::*, *}, 
    message::*, 
    plugins::nntree, 
    resources::{ map::*,  * },
    systems::physics,
};
use client::{
    resources::*,
    systems::{actor, animator, camera, debug_grid, debug_toggles, input, renet, target_cursor, *}
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
        FrameTimeDiagnosticsPlugin::default(),
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
        debug_grid::setup,
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
        physics::update,
        common::systems::actor::update,
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
        debug_grid::toggle_grid,
        debug_grid::update_grid,
        debug_toggles::toggle_slopes,
        debug_toggles::toggle_fixed_lighting,
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
    app.init_resource::<debug_grid::GridVisible>();
    app.init_resource::<debug_grid::GridNeedsRegen>();
    app.init_resource::<debug_toggles::SlopeRenderingEnabled>();
    app.init_resource::<debug_toggles::FixedLightingEnabled>();
    app.init_resource::<Server>();

    app.run();
}
