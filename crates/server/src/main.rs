mod arena;
mod components;
pub mod network;
mod plugins;
mod resources;
mod systems;

use std::time::*;
use bevy::{ log::LogPlugin, prelude::*, time::common_conditions::* };
use bevy_easings::*;
use serde::{Deserialize, Serialize};

use common_bevy::{
    components::{behaviour::*, entity_type::*},
    plugins::nntree,
    resources::{map::*, *},
};
use crate::{
    resources::*,
    systems::{actor, aoi, engagement_cleanup, engagement_spawner, input, renet, world},
};

#[derive(Clone, Copy, Debug, Deserialize, Event, Message, Serialize)]
pub struct Tick {
    pub ent: Entity,
    pub behaviour: Behaviour,
}


fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "arena") {
        return arena::run(&args[1..]);
    }

    let mut app = App::new();
    app.add_plugins((
        // Without a wait the runner spins a core. Physics is FixedUpdate, so
        // the wait only bounds how long a Try or Do sits before Update runs.
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))),
        LogPlugin {
            level: bevy::log::Level::TRACE,
            // world=warn keeps the terrain pipeline's per-tile tracing spans
            // disabled — at debug they cost real time on the hot path.
            filter:  "wgpu=error,bevy=warn,renetcode=warn,renet=warn,".to_owned()
                    +"server=trace,world=warn,"
                    ,
            custom_layer: |_| None,
            ..default()
        },
        TransformPlugin,
        crate::network::NetworkPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        crate::plugins::behaviour::BehaviourPlugin,
        crate::plugins::combat::CombatPlugin,
        crate::plugins::metrics::MetricsPlugin::default(),
        crate::plugins::world_streaming::WorldStreamingPlugin,
    ));

    app.add_message::<Tick>();

    // Add observers for triggered events
    app.add_observer(renet::do_manage_connections);
    app.add_observer(renet::do_presence);

    app.add_systems(FixedUpdate, (
        input::apply,
    ));

    app.add_systems(FixedPostUpdate, (
        input::broadcast_movement_intent,
        actor::broadcast_heading_changes, // Broadcast heading changes to clients
    ));

    app.add_systems(PreUpdate, (
        renet::write_try,
    ));

    // Actor systems
    app.add_systems(Update, (
        actor::do_incremental,
        actor::update,
    ));

    // World, network, and spawner systems
    app.add_systems(Update, (
        common_bevy::systems::world::try_incremental,
        common_bevy::systems::world::do_incremental,
        input::try_input,
        actor::try_teleport,
        engagement_spawner::try_spawn_den,
        input::try_set_tier_lock, // Tier lock targeting
        input::try_respec_attributes, // Attribute respec system
        crate::systems::equipment::try_wear,
        crate::systems::gathering::try_gather,
        crate::systems::gathering::try_take,
        crate::systems::gathering::finish_work,
        crate::systems::gathering::interrupt_work,
        crate::systems::gathering::close_windows,
        crate::systems::gathering::try_drop,
        common_bevy::systems::movement::update_burden,
        engagement_cleanup::update_engagement_proximity.run_if(on_timer(Duration::from_secs(1))), // Update proximity tracking
        engagement_cleanup::cleanup_engagements.run_if(on_timer(Duration::from_secs(5))), // Clean up dead/abandoned engagements
        world::do_spawn,
        world::try_spawn,
    ));

    app.add_systems(PostUpdate, (
        aoi::update_area_of_interest,
        renet::send_do.after(aoi::update_area_of_interest),
        renet::cleanup_despawned.after(renet::send_do),
    ));


    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(Map::new(qrz::Map::<EntityType>::new(
        common::camera::HEX_RADIUS,
        common::camera::RISE,
        qrz::HexOrientation::FlatTop,
    )));

    app.init_resource::<Lobby>();
    app.init_resource::<crate::systems::gathering::Piles>();
    app.init_resource::<InputQueues>();
    app.init_resource::<input::InputGuards>();
    let registry = crate::resources::event_registry::EventRegistry::new(::world::WORLD_SEED);
    // One definition of where the world starts: the difficulty origin and the
    // spawn are the same place, and the z comes from the terrain there.
    let haven = common_bevy::spatial_difficulty::HAVEN_LOCATION;
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
    let spawn_z = registry.elevation_at(haven.q, haven.r) + 1;
    app.insert_resource(common_bevy::components::resources::SpawnPoint(
        qrz::Qrz { q: haven.q, r: haven.r, z: spawn_z }));
    app.insert_resource(registry);
    app.init_resource::<crate::resources::summary_cache::SummaryCache>();
    app.init_resource::<engagement_spawner::ActiveSpawners>();

    app.run();
}
