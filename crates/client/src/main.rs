#![feature(more_float_constants)]
#![feature(extend_one)]

mod components;
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
use bevy_renet::{
    netcode::{NetcodeClientPlugin, NetcodeErrorEvent},
    RenetClientPlugin,
};

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
    },
    resources::*,
    systems::{ability_prediction, actor, actor_dead_visibility, animator, attack_telegraph, camera, combat, input, prediction, renet, targeting, world}
};
#[cfg(feature = "admin")]
use crate::systems::admin;

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(trigger: On<NetcodeErrorEvent>) {
    panic!("{:?}", trigger.event());
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
        .set(AssetPlugin {
            file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_string(),
            ..default()
        })
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,naga=warn,polling=warn,winit=warn,offset_allocator=warn,gilrs=warn,".to_owned()
                    +"bevy=warn,cosmic_text=warn,renetcode=warn,renet=warn,client=trace,"
                    ,
            custom_layer: |_| None,
            ..default()
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        common_bevy::plugins::controlled::ControlledPlugin,
        DevConsolePlugin,
        DiagnosticsPlugin,
        UiPlugin,
        VignettePlugin,
        MaterialPlugin::<ExtendedMaterial<StandardMaterial, crate::resources::TerrainExtension>>::default(),
    ));

    app.add_message::<Do>();
    app.add_message::<Try>();

    app.add_observer(panic_on_error_system);

    app.add_systems(Startup, (
        setup,
        actor::setup,
        camera::setup,
        renet::setup,
        world::setup,
    ));


    // Ensure proper ordering: update_keybits -> tick -> do_input
    #[cfg(feature = "admin")]
    app.add_systems(PreUpdate, input::update_keybits.run_if(admin::not_in_flyover));
    #[cfg(not(feature = "admin"))]
    app.add_systems(PreUpdate, input::update_keybits);
    app.add_systems(PreUpdate, common_bevy::resources::map::refresh_map);

    app.add_systems(FixedUpdate, (
        input::do_input.after(common_bevy::systems::behaviour::controlled::tick),
        common_bevy::systems::combat::resources::regenerate_resources,
    ));

    // ADR-019: Predict local player position by replaying InputQueue from confirmed state
    app.add_systems(FixedPostUpdate, (
        prediction::predict_local_player,
    ));

    app.add_systems(PreUpdate, (
        renet::write_do,
    ));

    app.add_systems(Update, (
        actor::do_spawn,
        actor::apply_movement_intent, // ADR-011: Apply movement intent predictions
        actor::try_gcd,
        prediction::advance_interpolation.before(actor::update), // ADR-019: Advance VisualPosition before rendering
        actor::update,
        actor_dead_visibility::update_dead_visibility,
        actor_dead_visibility::cleanup_dead_entities,
        animator::update,
        targeting::update_targets, // Update hostile targets every frame (detects when targets move)
        targeting::update_ally_targets, // Update ally targets every frame (detects when allies move)
        combat::player_auto_attack.run_if(on_timer(Duration::from_millis(500))), // Check for auto-attack opportunities every 0.5s
        combat::apply_gcd,
    ));

    // Camera: conditional on flyover state in admin builds
    #[cfg(feature = "admin")]
    app.add_systems(Update, (
        camera::update.run_if(admin::not_in_flyover),
        admin::flyover_camera_update.run_if(admin::flyover_active),
    ));
    #[cfg(not(feature = "admin"))]
    app.add_systems(Update, camera::update);

    // ADR-012: Client-side recovery (authoritative server, no prediction)
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
        // do_incremental must run AFTER apply_movement_intent so that
        // MovementPrediction exists when Loc updates arrive. Otherwise the
        // no-prediction fallback fires every frame, setting wrong visual targets.
        common_bevy::systems::world::do_incremental
            .after(actor::apply_movement_intent),
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
        renet::handle_pong,
        renet::periodic_ping,
        world::do_spawn,
        world::dispatch_lod_tasks.after(world::do_spawn),
        world::poll_and_swap_lod.after(world::dispatch_lod_tasks),
        world::update,
    ));

    app.add_systems(PostUpdate, (
        renet::send_try,
    ));

    let map_state = common_bevy::resources::map::MapState::new(qrz::Map::<EntityType>::new(1., 0.8, qrz::HexOrientation::FlatTop));
    let map = map_state.as_map(); // Create Map that shares the same Arc
    app.insert_resource(map_state);
    app.insert_resource(map);

    app.init_resource::<InputQueues>();
    app.init_resource::<EntityMap>();
    app.init_resource::<Server>();
    app.init_resource::<LoadedChunks>();
    app.init_resource::<crate::resources::SkipNeighborRegen>();
    app.init_resource::<crate::resources::ChunkLodMeshes>();
    app.init_resource::<crate::resources::LodTriangleStats>();

    // Admin resources and systems (compile-time feature gate)
    #[cfg(feature = "admin")]
    {
        app.init_resource::<admin::FlyoverState>();
        app.init_resource::<admin::PendingFlyoverTiles>();
        app.insert_resource(admin::AdminTerrain::default());

        app.add_systems(Update, (
            admin::execute_admin_actions,
            admin::flyover_movement.run_if(admin::flyover_active),
            admin::tag_admin_chunks,
            admin::poll_flyover_tile_tasks.run_if(admin::flyover_active),
            admin::flyover_generate_chunks
                .run_if(admin::flyover_active)
                .run_if(on_timer(Duration::from_millis(200))),
            admin::flyover_evict_chunks
                .run_if(admin::flyover_active)
                .run_if(on_timer(Duration::from_secs(1))),
        ));
    }

    // Data eviction: remove tiles/summaries beyond view range (timer, every 5s)
    // Disabled during flyover — admin module handles its own eviction
    #[cfg(feature = "admin")]
    app.add_systems(Update, world::evict_data
        .run_if(on_timer(Duration::from_secs(5)))
        .run_if(admin::not_in_flyover));
    #[cfg(not(feature = "admin"))]
    app.add_systems(Update, world::evict_data.run_if(on_timer(Duration::from_secs(5))));

    app.run();
}
