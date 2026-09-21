#![feature(more_float_constants)]
#![feature(extend_one)]

mod components;
pub mod network;
mod plugins;
mod resources;
mod systems;

use std::time::Duration;

use bevy::{
    log::LogPlugin,
    pbr::ExtendedMaterial,
    prelude::*,
    time::common_conditions::on_timer,
};
use bevy_easings::*;
use common_bevy::{
    components::{entity_type::*, *},
    message::*,
    plugins::nntree,
    resources::*,
};
use crate::{
    plugins::{
        console::DevConsolePlugin,
        diagnostics::DiagnosticsPlugin,
        ui::UiPlugin,
        vignette::VignettePlugin,
        water::WaterPlugin,
    },
    resources::*,
    systems::{ability_prediction, actor, actor_dead_visibility, animator, attack_telegraph, camera, combat, equipment, hiding, input, movement, renet, targeting, world}
};
#[cfg(feature = "admin")]
use crate::plugins::flyover;

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
        .set(hiding::gltf_plugin())
        .set(AssetPlugin {
            file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_string(),
            ..default()
        })
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            // world=warn keeps the terrain pipeline's per-tile tracing spans
            // disabled — at debug they cost real time on the hot path.
            filter:
                "wgpu=error,naga=warn,polling=warn,winit=warn,offset_allocator=warn,gilrs=warn,\
                 cosmic_text=warn,renetcode=warn,renet=warn,egui=warn,epaint=warn,client=trace,world=warn,bevy=warn".to_string(),
            custom_layer: |_| None,
            ..default()
        }),
        crate::network::NetworkPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        DevConsolePlugin,
        DiagnosticsPlugin,
        crate::plugins::world_streaming::WorldStreamingPlugin,
        UiPlugin,
        VignettePlugin,
        WaterPlugin,
        MaterialPlugin::<ExtendedMaterial<StandardMaterial, crate::resources::TerrainExtension>>::default(),
        MaterialPlugin::<world::DiscMaterial>::default(),
        MaterialPlugin::<world::SkyMaterial>::default(),
    ));

    app.add_message::<Do>();
    app.add_message::<Try>();

    app.add_systems(Startup, (
        setup,
        actor::setup,
        camera::setup,
        world::setup,
    ));


    #[cfg(feature = "admin")]
    app.add_systems(PreUpdate, input::update_keybits.run_if(flyover::not_in_flyover));
    #[cfg(not(feature = "admin"))]
    app.add_systems(PreUpdate, input::update_keybits);

    app.add_systems(FixedUpdate, (
        input::tick,
        input::do_confirm,
        movement::simulate_remote,
        common_bevy::systems::combat::resources::regenerate_resources,
    ));

    app.add_systems(FixedPostUpdate, (
        movement::predict_local_player,
    ));

    // The visual advances once per frame here, so every Update reader —
    // the actor's transform, the camera — sees the same current() whatever
    // their order. A fixed tick between re-targets it from current(), which
    // does not move it.
    app.add_systems(PreUpdate, (
        renet::write_do,
        movement::advance_interpolation,
    ));

    app.add_systems(Update, (
        actor::do_spawn,
        movement::apply_intent,
        movement::apply_displace,
        actor::try_gcd,
        actor::update,
        actor_dead_visibility::update_dead_visibility,
        actor_dead_visibility::cleanup_dead_entities,
        animator::play_abilities,
        animator::update,
        targeting::update_targets, // Update hostile targets every frame (detects when targets move)
        targeting::update_ally_targets, // Update ally targets every frame (detects when allies move)
        combat::player_auto_attack.run_if(on_timer(Duration::from_millis(500))), // Check for auto-attack opportunities every 0.5s
        combat::apply_gcd,
    ));

    // Camera: conditional on flyover state in admin builds
    #[cfg(feature = "admin")]
    app.add_systems(Update, (
        camera::update.run_if(flyover::not_in_flyover),
    ));
    #[cfg(not(feature = "admin"))]
    app.add_systems(Update, camera::update);

    // Client-side recovery (authoritative server, no prediction)
    app.add_systems(Update, (
        ability_prediction::handle_ability_used, // Apply recovery/synergies when server confirms ability use
        common_bevy::systems::combat::recovery::global_recovery_system, // Tick down recovery timer
        common_bevy::systems::combat::synergies::synergy_cleanup_system, // Clean up expired synergies
        common_bevy::systems::combat::queue::sync_queue_window_size, // Sync queue window size when attributes change
    ));

    app.add_systems(Update, (
        combat::handle_insert_threat,
        combat::handle_apply_damage,
        combat::handle_clear_queue,
        combat::handle_ability_failed,
        common_bevy::systems::world::try_incremental,
        common_bevy::systems::world::do_incremental,
        // A Loc that ends a slide must see the Displacing marker the slide
        // inserted, so the slide handler runs (and its commands apply) first.
        movement::do_loc.after(movement::apply_displace),
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
        world::do_init,
        (
            equipment::do_inventory,
            equipment::dress,
            hiding::parse_extras,
            equipment::bind_worn,
            hiding::hide_under,
        ).chain(),
        renet::handle_pong,
        renet::periodic_ping,
        world::update,
    ));

    app.add_systems(PostUpdate, (
        renet::send_try,
        world::follow_camera,
    ));

    app.insert_resource(common_bevy::resources::map::Map::new(
        qrz::Map::<EntityType>::new(
            common::camera::HEX_RADIUS,
            common::camera::RISE,
            qrz::HexOrientation::FlatTop,
        ),
    ));

    app.init_resource::<InputQueues>();
    app.init_resource::<hiding::HiddenMeshes>();
    app.init_resource::<EntityMap>();
    app.init_resource::<Server>();
    app.init_resource::<crate::resources::SkipNeighborRegen>();
    app.init_resource::<crate::resources::ClientTimers>();

    #[cfg(feature = "admin")]
    app.add_plugins(flyover::FlyoverPlugin);


    app.run();
}
