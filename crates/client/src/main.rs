// AsBindGroup builds a SystemParam tuple that nests past the default 128.
#![recursion_limit = "256"]

mod components;
pub mod network;
mod plugins;
mod resources;
mod systems;

use bevy::{
    log::LogPlugin,
    pbr::ExtendedMaterial,
    prelude::*,
    render::error_handler::{ErrorType, RenderErrorHandler, RenderErrorPolicy},
};
use bevy_easings::*;
use common_bevy::{
    components::*,
    message::*,
    plugins::nntree,
    resources::*,
};
use crate::{
    plugins::{
        console::DevConsolePlugin,
        diagnostics::DiagnosticsPlugin,
        ink::InkPlugin,
        settings::SettingsPlugin,
        shell::ShellPlugin,
        ui::UiPlugin,
        vignette::VignettePlugin,
        water::WaterPlugin,
    },
    resources::*,
    systems::{actor, actor_dead_visibility, animator, camera, combat, equipment, gathering, hiding, input, movement, renet, struck, targeting, world}
};
#[cfg(feature = "admin")]
use crate::plugins::recorder;

/// The game's assets, the assets repo's checkout: what the asset server
/// reads, and the music's bank.
pub const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

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
            file_path: ASSETS.to_string(),
            ..default()
        })
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            // world=warn keeps the terrain pipeline's per-tile tracing spans
            // disabled — at debug they cost real time on the hot path. ureq
            // and rustls trace every chunk of the music banks' download.
            filter:
                "wgpu=error,naga=warn,polling=warn,winit=warn,offset_allocator=warn,gilrs=warn,\
                 cosmic_text=warn,renetcode=warn,renet=warn,client=trace,world=warn,bevy=warn,\
                 ureq=warn,ureq_proto=warn,rustls=warn".to_string(),
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
        crate::plugins::cover::CoverPlugin,
        crate::plugins::canopy::CanopyPartsPlugin,
        MaterialPlugin::<ExtendedMaterial<StandardMaterial, crate::resources::TerrainExtension>>::default(),
        MaterialPlugin::<world::DiscMaterial>::default(),
        MaterialPlugin::<world::SkyMaterial>::default(),
    ));
    app.add_plugins((SettingsPlugin, ShellPlugin, InkPlugin, crate::plugins::cel::CelPlugin, crate::plugins::music::MusicPlugin));

    // wgpu reports a validation error where a draw and its pass disagree,
    // and the default handler quits. A custom draw that misses a frame's
    // change of sample count says so this way, and the frame after it is
    // right: losing the client to one is worse than drawing it wrong.
    // A lost device is the one worth acting on.
    app.insert_resource(RenderErrorHandler(|error, _main, _render| match error.ty {
        ErrorType::DeviceLost => RenderErrorPolicy::Recover(default()),
        ErrorType::OutOfMemory => RenderErrorPolicy::StopRendering,
        ErrorType::Validation => RenderErrorPolicy::Ignore,
        ErrorType::Internal => RenderErrorPolicy::StopRendering,
    }));

    app.add_message::<Do>();
    app.add_message::<Try>();

    app.add_systems(Startup, (
        setup,
        camera::setup,
        world::setup,
    ));


    // Keys move the character only while the world is played: loading, it
    // stands where the server put it.
    let playing = in_state(crate::plugins::shell::Stage::Playing);
    app.add_systems(PreUpdate, input::update_keybits.run_if(playing));

    app.add_systems(FixedUpdate, (
        input::tick,
        input::do_confirm,
        movement::simulate_remote,
        common_bevy::components::status::tick_status,
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
        crate::resources::rebase_origin,
    ));

    app.add_systems(Update, (
        actor::do_spawn,
        movement::apply_intent,
        movement::apply_displace,
        movement::end_displace,
        actor::update,
        actor_dead_visibility::update_dead_visibility,
        actor_dead_visibility::cleanup_dead_entities,
        actor_dead_visibility::respawn,
        animator::play_abilities,
        animator::play_held,
        animator::update,
        targeting::update_targets, // Update hostile targets every frame (detects when targets move)
        targeting::update_ally_targets, // Update ally targets every frame (detects when allies move)
    ));

    // Camera: in admin builds, while the recorder runs no camera path
    #[cfg(feature = "admin")]
    app.add_systems(Update, camera::update.run_if(not(recorder::camera_free)));
    #[cfg(not(feature = "admin"))]
    app.add_systems(Update, camera::update);

    // The server starts every recovery; the client only counts it down
    app.add_systems(Update, (
        common_bevy::systems::combat::recovery::global_recovery_system,
    ));

    app.add_systems(Update, (
        combat::handle_insert_threat,
        (struck::on_contact, struck::on_landing, struck::update_marks, struck::flash).chain(),
        combat::handle_clear_queue,
        combat::land_own,
        combat::answer_own,
        common_bevy::systems::world::try_incremental,
        common_bevy::systems::world::do_incremental,
        // A Loc that arrives with a slide must see the Displacing marker the
        // slide inserted, so the slide handler runs (and its commands apply) first.
        movement::do_loc.after(movement::apply_displace),
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
        renet::track_latency,
        renet::handle_arrived,
        world::update,
    ));

    app.add_systems(Update, (
        gathering::request,
        gathering::mark,
        gathering::do_loot,
        gathering::do_activity,
        (equipment::hold, equipment::bind_held).chain(),
        crate::systems::focus::track,
        crate::systems::loot_window::update,
        common_bevy::systems::movement::update_burden,
    ).run_if(in_state(crate::plugins::shell::Stage::Playing)));

    app.add_systems(PostUpdate, (
        renet::send_try,
        world::follow_camera,
    ));

    app.insert_resource(crate::resources::world_map());

    app.init_resource::<InputQueues>();
    app.init_resource::<combat::Gone>();
    app.init_resource::<common_bevy::tuning::Tuning>();
    app.init_resource::<crate::resources::RenderOrigin>();
    app.init_resource::<gathering::LootWindow>();
    app.init_resource::<crate::systems::focus::NumpadFocus>();
    app.init_resource::<crate::systems::help::Help>();
    app.add_systems(Startup, crate::systems::help::setup);
    app.add_systems(Update, crate::systems::help::show);
    app.init_resource::<hiding::HiddenMeshes>();
    app.init_resource::<EntityMap>();
    app.init_resource::<Server>();

    #[cfg(feature = "admin")]
    app.add_plugins(recorder::RecorderPlugin);


    app.run();
}
