#![feature(let_chains)]
#![feature(more_float_constants)]
#![feature(extend_one)]

mod common;
mod client;

use std::time::{Duration, SystemTime};
use std::net::UdpSocket;

use bevy::{
    log::LogPlugin,
    prelude::*,
    time::common_conditions::on_timer,
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
    systems::{physics, targeting},
};
use client::{
    plugins::{
        console::DevConsolePlugin,
        diagnostics::DiagnosticsPlugin,
        ui::UiPlugin,
    },
    resources::*,
    systems::{actor, actor_dead_visibility, animator, camera, combat, input, projectile, renet, world}
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
        common::plugins::controlled::ControlledPlugin,
        DevConsolePlugin,
        DiagnosticsPlugin,
        UiPlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        setup,
        actor::setup,
        camera::setup,
        renet::setup,
        world::setup,
    ));


    app.add_systems(PreUpdate, (
        // Ensure proper ordering: update_keybits -> tick -> do_input
        input::update_keybits,
    ));

    app.add_systems(FixedUpdate, (
        input::do_input.after(common::systems::behaviour::controlled::tick),
        physics::update,
        projectile::update_projectiles, // Client-side projectile movement simulation
        common::systems::combat::resources::regenerate_resources,
    ));

    app.add_systems(PreUpdate, (
        renet::write_do,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        actor::do_spawn,
        actor::try_gcd,
        actor::update,
        actor_dead_visibility::update_dead_visibility,
        animator::update,
        camera::update,
        targeting::update_targets_on_change, // Reactive targeting: updates Target when heading/loc changes
        projectile::spawn_hit_flash, // Spawn flash effects when projectiles hit
        projectile::update_hit_flashes, // Update and fade out flash effects
        combat::player_auto_attack.run_if(on_timer(Duration::from_millis(500))), // Check for auto-attack opportunities every 0.5s
        combat::apply_gcd,
        // REMOVED: Client-side attack prediction - all combat is server-authoritative
        combat::handle_insert_threat,
        combat::handle_apply_damage,
        combat::handle_clear_queue,
        combat::handle_ability_failed,
        common::systems::world::try_incremental,
        common::systems::world::do_incremental,
    ));

    app.add_systems(Update, (
        world::async_spawn,
        world::async_ready,
        world::do_init,
        renet::handle_pong,
        renet::periodic_ping,
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
    app.init_resource::<LoadedChunks>();

    // Add chunk eviction system (runs periodically to cleanup distant chunks)
    // Runs every 5 seconds with a +1 chunk buffer to prevent aggressive eviction
    // Server mirrors client eviction logic in do_incremental to track which chunks
    // the client has evicted, allowing them to be re-sent when player returns
    app.add_systems(Update, world::evict_distant_chunks.run_if(on_timer(Duration::from_secs(5))));

    app.run();
}
