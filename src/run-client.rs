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
};
use client::{
    plugins::{
        console::DevConsolePlugin,
        diagnostics::DiagnosticsPlugin,
        ui::UiPlugin,
        vignette::VignettePlugin,
    },
    resources::*,
    systems::{ability_prediction, actor, actor_dead_visibility, animator, attack_telegraph, camera, combat, input, prediction, renet, targeting, world}
};

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(
    mut renet_error: MessageReader<NetcodeTransportError>
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
            ..default()
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        common::plugins::controlled::ControlledPlugin,
        DevConsolePlugin,
        DiagnosticsPlugin,
        UiPlugin,
        VignettePlugin,
    ));

    app.add_message::<Do>();
    app.add_message::<Try>();

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
        common::systems::combat::resources::regenerate_resources,
    ));

    // ADR-019: Predict local player position by replaying InputQueue from confirmed state
    app.add_systems(FixedPostUpdate, (
        prediction::predict_local_player,
    ));

    app.add_systems(PreUpdate, (
        renet::write_do,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        actor::do_spawn,
        actor::apply_movement_intent, // ADR-011: Apply movement intent predictions
        actor::try_gcd,
        prediction::advance_interpolation.before(actor::update), // ADR-019: Advance VisualPosition before rendering
        actor::update,
        actor_dead_visibility::update_dead_visibility,
        actor_dead_visibility::cleanup_dead_entities,
        animator::update,
        camera::update,
        targeting::update_targets, // Update hostile targets every frame (detects when targets move)
        targeting::update_ally_targets, // Update ally targets every frame (detects when allies move)
        combat::player_auto_attack.run_if(on_timer(Duration::from_millis(500))), // Check for auto-attack opportunities every 0.5s
        combat::apply_gcd,
    ));

    // ADR-012: Client-side recovery (authoritative server, no prediction)
    app.add_systems(Update, (
        ability_prediction::handle_ability_used, // Apply recovery/synergies when server confirms ability use
        common::systems::combat::recovery::global_recovery_system, // Tick down recovery timer
        common::systems::combat::synergies::synergy_cleanup_system, // Clean up expired synergies
    ));

    app.add_systems(Update, (
        combat::handle_insert_threat,
        combat::handle_apply_damage,
        combat::handle_clear_queue,
        combat::handle_ability_failed,
        common::systems::world::try_incremental,
        common::systems::world::do_incremental,
    ));

    // Attack telegraph systems
    app.add_systems(Update, (
        attack_telegraph::on_insert_threat,
        // CRITICAL: on_apply_damage MUST run before on_clear_queue
        // When damage is applied, server sends both ApplyDamage and ClearQueue events
        // We need to spawn the line before clearing the ball
        attack_telegraph::on_apply_damage.before(attack_telegraph::on_clear_queue),
        attack_telegraph::on_clear_queue,
        attack_telegraph::update_telegraphs,
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
