pub mod network;
mod plugins;
mod resources;
mod systems;

use std::time::*;
use bevy::{ log::LogPlugin, prelude::*, time::common_conditions::* };

use common_bevy::{
    plugins::nntree,
    resources::{map::*, *},
};
use crate::{
    resources::*,
    systems::{actor, aoi, engagement_cleanup, engagement_spawner, input, renet, world},
};


/// When the admin console let go of the server: it stops.
static LET_GO: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// When the update that stopped the server ended, whatever stopped it.
static STOPPED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Stops the server once the console that started it lets go, saying how
/// long the word waited for a frame to hear it.
fn stop_when_let_go(mut exit: MessageWriter<AppExit>) {
    if let Some(let_go) = LET_GO.get() {
        warn!("the admin console let go of the server {:.1?} ago; it stops", let_go.elapsed());
        exit.write(AppExit::Success);
    }
}

/// Marks the frame the server stops on, before the store's last write.
fn mark_stopped(mut exit: MessageReader<AppExit>) {
    if exit.read().count() > 0 {
        let _ = STOPPED.set(Instant::now());
    }
}

/// Runs the server as the admin console's child, and as nothing else.
/// The console holds the other end of its standard input: when that
/// closes, by the console's word or because the console is gone, the
/// server writes what it holds and stops.
fn main() {
    if std::env::var_os(common::settings::CONSOLE_ENV).is_none() {
        eprintln!("The server is started from the admin console, which holds and stops it: cargo run --bin console");
        std::process::exit(2);
    }
    std::thread::spawn(|| {
        let mut heard = String::new();
        while std::io::stdin().read_line(&mut heard).is_ok_and(|read| read > 0) {
            heard.clear();
        }
        let _ = LET_GO.set(Instant::now());
    });

    let mut app = App::new();
    app.add_systems(Update, stop_when_let_go);
    app.add_systems(PostUpdate, mark_stopped);
    app.add_plugins((
        // Without a wait the runner spins a core. Physics is FixedUpdate, so
        // the wait only bounds how long a Try or Do sits before Update runs.
        // Half the cores build terrain for the far view and the chunks, each
        // task on its own thread, where the ladder builds separate areas at
        // once; the frame keeps a quarter.
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))).set(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions {
                async_compute: bevy::app::TaskPoolThreadAssignmentPolicy { min_threads: 1, max_threads: usize::MAX, percent: 0.5, on_thread_spawn: None, on_thread_destroy: None },
                ..default()
            },
        }),
        LogPlugin {
            level: bevy::log::Level::TRACE,
            // world=warn keeps the terrain pipeline's per-tile tracing spans
            // disabled — at debug they cost real time on the hot path.
            filter:  "wgpu=error,bevy=warn,renetcode=warn,renet=warn,sqlx=warn,polling=warn,async_io=warn,".to_owned()
                    +"server=trace,world=warn,"
                    ,
            custom_layer: |_| None,
            ..default()
        },
        // First, so the settings it reads from the store are there for
        // whatever is built with a cold one
        crate::plugins::persistence::PersistencePlugin,
        crate::network::NetworkPlugin,
        nntree::NNTreePlugin,
        combat::BehaviourPlugin,
        combat::CombatPlugin,
        crate::plugins::metrics::MetricsPlugin,
        crate::plugins::world_streaming::WorldStreamingPlugin,
        // Ctrl-C stops the server through `AppExit`, so what it holds is
        // written before it goes.
        bevy::app::TerminalCtrlCHandlerPlugin,
    ));


    app.add_observer(renet::do_manage_connections);
    app.add_observer(renet::do_presence);

    app.add_systems(FixedUpdate, (
        input::apply,
    ));

    app.add_systems(FixedPostUpdate, (
        input::broadcast_movement_intent,
        actor::broadcast_heading_changes,
    ));

    // A client is told what its character wears before it can enter
    app.add_systems(PreUpdate, (
        renet::show_characters,
        renet::write_try,
        renet::admit,
    ).chain());

    // Actor systems
    app.add_systems(Update, (
        actor::do_incremental,
        combat::actor::update,
    ));

    // World, network, and spawner systems
    app.add_systems(Update, (
        common_bevy::systems::world::try_incremental,
        common_bevy::systems::world::do_incremental,
        input::try_input,
        (actor::try_teleport, (world::follow_wall, world::try_change_calendar).chain()),
        engagement_spawner::try_spawn_party,
        (input::try_respec_attributes, input::try_set_loadout),
        (crate::systems::equipment::try_wear, crate::systems::mount::try_mount, crate::systems::mount::dismount),
        crate::systems::gathering::try_gather,
        crate::systems::gathering::try_take,
        crate::systems::gathering::finish_work,
        crate::systems::gathering::interrupt_work,
        crate::systems::gathering::close_windows,
        (
            crate::systems::gathering::try_drop,
            crate::systems::building::try_build,
            crate::systems::building::try_site,
            crate::systems::building::finish_building,
            crate::systems::building::interrupt_building,
            crate::systems::building::try_use,
            crate::systems::building::house,
            crate::systems::building::shelter.run_if(on_timer(Duration::from_millis(500))),
            crate::systems::settlers::tend_camps,
            crate::systems::settlers::try_hail,
            crate::systems::settlers::try_load,
            crate::systems::settlers::try_store,
            crate::systems::settlers::try_plan,
            crate::systems::settlers::try_layout,
            crate::systems::settlers::try_send,
            crate::systems::settlers::heed_settings,
            crate::systems::charts::chart,
            crate::systems::charts::tell,
            crate::systems::charts::try_draw,
            crate::systems::charts::relay,
        ),
        common_bevy::systems::movement::update_burden,
        engagement_cleanup::update_engagement_proximity.run_if(on_timer(Duration::from_secs(1))),
        engagement_cleanup::cleanup_engagements.run_if(on_timer(Duration::from_secs(5))),
        crate::systems::dens::tend_dens,
        crate::systems::dens::show_dens.run_if(on_timer(Duration::from_secs(1))),
        world::do_spawn,
        world::try_spawn,
    ));

    app.add_systems(PostUpdate, (
        aoi::update_area_of_interest,
        renet::send_do.after(aoi::update_area_of_interest),
        combat::actor::cleanup_despawned.after(renet::send_do),
        // What an NPC leaves is laid where it stood, which is read off it
        // before it is gone
        crate::systems::loot::leave_loot.before(combat::actor::cleanup_despawned),
    ));


    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(Map::new(qrz::Map::new(common::grid::HEX_RADIUS, common::grid::RISE)));

    app.init_resource::<Lobby>();
    app.init_resource::<renet::Shown>();
    app.init_resource::<crate::systems::characters::Characters>();
    app.init_resource::<crate::systems::gathering::Piles>();
    app.init_resource::<InputQueues>();
    app.init_resource::<input::InputGuards>();
    // One definition of where the world starts: the difficulty origin and the
    // spawn are the same place, and the z comes from the terrain there.
    let haven = common_bevy::haven::launch();
    info!("the haven stands at ({}, {})", haven.q, haven.r);
    app.init_resource::<crate::systems::building::Constructs>();
    app.init_resource::<crate::systems::settlers::Camps>();
    app.init_resource::<crate::systems::charts::Charts>();
    app.init_resource::<crate::systems::haul::Depots>();
    app.init_resource::<crate::systems::settlers::Settling>();
    app.add_systems(FixedUpdate, crate::systems::settlers::settle);
    app.add_systems(Update, crate::systems::settlers::tell_stopped.run_if(on_timer(Duration::from_secs(1))));
    let registry = crate::resources::event_registry::EventRegistry::new(::world::WORLD_SEED);
    app.insert_resource(common_bevy::hubs::Hubs::new((haven.q, haven.r)));
    // `CLEARING=<radius>[@<q>,<r>][~]` fells and mines every tile that far
    // round the tile named, or the haven, before anything is served: a
    // fixture for seeing changes from afar. With `~` it thins out toward its
    // edge instead of stopping there.
    let changes = match std::env::var("CLEARING") {
        Ok(spec) => {
            let (spec, fades) = spec.strip_suffix('~').map_or((spec.as_str(), false), |s| (s, true));
            let (radius, at) = spec.split_once('@').unwrap_or((spec, ""));
            let radius: i32 = radius.parse().expect("CLEARING starts with a radius in tiles");
            let at = at.split_once(',').map_or((haven.q, haven.r), |(q, r)| {
                (q.parse().expect("CLEARING's q is a whole number"), r.parse().expect("CLEARING's r is a whole number"))
            });
            info!("clearing {radius} tiles round ({}, {}){}", at.0, at.1, if fades { ", fading" } else { "" });
            crate::systems::gathering::WorldChanges::clearing(|q, r| registry.cover_at(q, r), at, radius, fades)
        }
        Err(_) => default(),
    };
    app.insert_resource(changes);
    app.insert_resource(renet::Admission::reading(haven, registry.clone()));
    app.insert_resource(registry);
    app.init_resource::<crate::resources::summary_cache::SummaryCache>();
    app.init_resource::<engagement_spawner::Parties>();
    app.init_resource::<crate::systems::dens::Dens>();
    app.add_message::<crate::systems::dens::EngagementEnded>();
    app.add_message::<crate::systems::gathering::TileChanged>();
    app.add_message::<crate::systems::economy::Moved>();

    // The schedule runner's loop, and then the world freed a part at a
    // time, saying which part took the time
    app.set_runner(|mut app: App| {
        while app.plugins_state() == bevy::app::PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        let wait = Duration::from_secs_f64(1.0 / 60.0);
        let exit = loop {
            let began = Instant::now();
            app.update();
            if let Some(exit) = app.should_exit() {
                break exit;
            }
            if let Some(left) = wait.checked_sub(began.elapsed()) {
                std::thread::sleep(left);
            }
        };
        // Resources first: each is an entity's component, so freeing the
        // entities first frees them all unnamed
        let world = app.world_mut();
        let held: Vec<_> = world.iter_resources().map(|(id, info, _)| (id, info.name().to_string())).collect();
        for (id, name) in held {
            let began = Instant::now();
            world.remove_resource_by_id(id);
            if began.elapsed() > Duration::from_millis(50) {
                info!("teardown: {name} freed in {:.1?}", began.elapsed());
            }
        }
        let began = Instant::now();
        let entities = world.entities().len();
        world.clear_entities();
        info!("teardown: {entities} entities freed in {:.1?}", began.elapsed());
        let began = Instant::now();
        drop(app);
        info!("teardown: the rest of the app freed in {:.1?}", began.elapsed());
        exit
    });
    app.run();
    if let Some(stopped) = STOPPED.get() {
        info!("the server came down {:.1?} after the update that stopped it", stopped.elapsed());
    }
}
