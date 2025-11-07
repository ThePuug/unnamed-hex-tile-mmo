#![feature(more_float_constants)]
#![feature(extend_one)]

mod common;
mod server;

use std::{ net::UdpSocket, time::* };
use bevy::{ log::LogPlugin, prelude::*, time::common_conditions::* };
use bevy_easings::*;
use bevy_renet::{
    renet::{ConnectionConfig, RenetServer},
    netcode::{NetcodeServerTransport, NetcodeTransportError, NetcodeServerPlugin},
    RenetServerPlugin,
};
use ::renet::DefaultChannel;

use crate::{
    common::{
        chunk::WorldDiscoveryCache,
        components::entity_type::*,
        message::*,
        plugins::nntree,
        resources::{map::*, *},
        systems::physics
    },
    server::{
        resources::{terrain::*, *},
        systems::{actor, combat, input, reaction_queue, renet, spawner, targeting, world},
        *
    }
};

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(mut renet_error: EventReader<NetcodeTransportError>) {
    if let Some(e) = renet_error.read().next() {
        panic!("{:?}", e);
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,bevy=warn,".to_owned()
                    +"unnamed_hex_tile_mmo=trace,"
                    ,
            custom_layer: |_| None,
        },
        TransformPlugin,
        RenetServerPlugin,
        NetcodeServerPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        common::plugins::controlled::ControlledPlugin,
        server::plugins::behaviour::BehaviourPlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();
    app.add_event::<Tick>();

    // Add observers for triggered events
    app.add_observer(combat::process_deal_damage);
    app.add_observer(combat::resolve_threat);

    app.add_systems(Startup, (
        world::setup,
    ));

    app.add_systems(FixedUpdate, (
        physics::update,
        common::systems::combat::resources::regenerate_resources,
        spawner::leash_health_regen, // Rapidly regenerate health for leashing NPCs (100 HP/sec)
        common::systems::combat::state::update_combat_state,
        common::systems::combat::recovery::global_recovery_system, // ADR-012: Tick down recovery lockout
        common::systems::combat::synergies::synergy_cleanup_system, // ADR-012: Clean up expired synergies
        reaction_queue::process_expired_threats,
    ));

    app.add_systems(FixedPostUpdate, (
        input::broadcast_player_movement_intent, // ADR-011: Broadcast player movement intents AFTER physics has processed all inputs
        actor::broadcast_heading_changes, // ADR-011: Broadcast heading changes to clients
    ));

    app.add_systems(PreUpdate, (
        renet::write_try,
    ));

    // Core combat and actor systems
    app.add_systems(Update, (
        panic_on_error_system,
        actor::do_incremental,
        actor::update,
        targeting::update_targets, // Update targets every frame (detects when targets move)
        combat::do_nothing, // CRITICAL: needed because of some magic number of systems
        combat::process_passive_auto_attack.run_if(on_timer(Duration::from_millis(500))), // ADR-009: Auto-attack passive for NPCs only (check every 0.5s) - DIAGNOSTIC: runtime resource commented out
        combat::validate_ability_prerequisites,
        combat::abilities::auto_attack::handle_auto_attack,
        combat::abilities::overpower::handle_overpower,
        combat::abilities::lunge::handle_lunge,
        combat::abilities::knockback::handle_knockback,
        combat::abilities::deflect::handle_deflect,
        combat::abilities::volley::handle_volley,
        // Note: reset_tier_lock_on_ability_use not needed - tier lock persists while held
        common::systems::combat::resources::check_death, // Check for death from ANY source
    ));

    // World, network, and spawner systems
    app.add_systems(Update, (
        common::systems::world::try_incremental,
        common::systems::world::do_incremental,
        input::send_input,
        input::try_gcd,
        input::try_input,
        input::try_set_tier_lock, // ADR-010 Phase 1: Tier lock targeting
        renet::do_manage_connections,
        spawner::tick_spawners.run_if(on_timer(Duration::from_secs(1))),
        spawner::despawn_out_of_range.run_if(on_timer(Duration::from_secs(3))),
        world::do_spawn,
        world::try_spawn,
    ));

    app.add_systems(Update, (
        actor::do_spawn_discover,   // Discover initial chunks after spawn
        actor::try_discover_chunk,  // New chunk-based discovery
        actor::try_discover,        // Legacy tile discovery (for compatibility)
        server::systems::diagnostics::check_duplicate_tiles,
        common::systems::combat::resources::process_respawn, // Process respawn timers, teleport to origin
    ));

    app.add_systems(PostUpdate, (
        renet::send_do,
        renet::cleanup_despawned.after(renet::send_do),
    ));

    let (server, transport) = renet::new_renet_server();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8)));

    app.init_resource::<Lobby>();
    app.init_resource::<InputQueues>();
    app.init_resource::<Terrain>();
    app.init_resource::<RunTime>();
    app.init_resource::<WorldDiscoveryCache>();
    app.init_resource::<server::systems::diagnostics::TerrainTracker>();

    app.run();
}
