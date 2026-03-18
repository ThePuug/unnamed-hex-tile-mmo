#![feature(more_float_constants)]
#![feature(extend_one)]

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
    chunk::WorldDiscoveryCache,
    components::{behaviour::*, entity_type::*},
    message::*,
    plugins::nntree,
    resources::{map::*, *},
};
use crate::{
    resources::{engagement_budget::EngagementBudget, terrain::*, *},
    systems::{actor, aoi, combat, engagement_cleanup, engagement_spawner, input, npc_ability_usage, reaction_queue, renet, targeting, world},
};

#[derive(Clone, Copy, Debug, Deserialize, Event, Message, Serialize)]
pub struct Tick {
    pub ent: Entity,
    pub behaviour: Behaviour,
}


fn main() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,bevy=warn,renetcode=warn,renet=warn,".to_owned()
                    +"server=trace,"
                    ,
            custom_layer: |_| None,
            ..default()
        },
        TransformPlugin,
        crate::network::NetworkPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
        common_bevy::plugins::controlled::ControlledPlugin,
        crate::plugins::behaviour::BehaviourPlugin,
        crate::plugins::metrics::MetricsPlugin::default(),
    ));

    app.add_message::<Do>();
    app.add_message::<Try>();
    app.add_message::<Tick>();

    // Add observers for triggered events
    app.add_observer(renet::do_manage_connections);
    app.add_observer(combat::process_deal_damage);
    app.add_observer(combat::resolve_threat);

    app.add_systems(Startup, (
        world::setup,
    ));

    app.add_systems(FixedUpdate, (
        common_bevy::systems::combat::resources::regenerate_resources, // Handles all resource regen including leash health regen (100 HP/sec for Returning NPCs)
        common_bevy::systems::combat::state::update_combat_state,
        common_bevy::systems::combat::recovery::global_recovery_system, // ADR-012: Tick down recovery lockout
        common_bevy::systems::combat::synergies::synergy_cleanup_system, // ADR-012: Clean up expired synergies
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
        actor::do_incremental,
        actor::update,
        targeting::update_targets, // Update targets every frame (detects when targets move)
        combat::do_nothing, // CRITICAL: needed because of some magic number of systems
        combat::process_passive_auto_attack.run_if(on_timer(Duration::from_millis(500))), // ADR-009: Auto-attack passive for NPCs only (check every 0.5s) - DIAGNOSTIC: runtime resource commented out
        npc_ability_usage::npc_ability_usage.run_if(on_timer(Duration::from_millis(500))), // ADR-014 Phase 3B: NPCs use signature abilities (check every 0.5s for responsive Defender counters)
        combat::validate_ability_prerequisites,
        combat::abilities::auto_attack::handle_auto_attack,
        combat::abilities::overpower::handle_overpower,
        combat::abilities::lunge::handle_lunge,
        combat::abilities::counter::handle_counter,  // ADR-014: Counter ability
        combat::abilities::kick::handle_kick,        // Kick: reactive knockback
        combat::abilities::deflect::handle_deflect,
        combat::abilities::volley::handle_volley,
        // Note: reset_tier_lock_on_ability_use not needed - tier lock persists while held
        reaction_queue::process_dismiss, // ADR-022: Dismiss front queue threat (no GCD/lockout)
        common_bevy::systems::combat::resources::check_death, // Check for death from ANY source
    ));

    // World, network, and spawner systems
    app.add_systems(Update, (
        common_bevy::systems::world::try_incremental,
        common_bevy::systems::world::do_incremental,
        input::send_input,
        input::try_gcd,
        input::try_input,
        input::try_set_tier_lock, // ADR-010 Phase 1: Tier lock targeting
        input::try_respec_attributes, // Attribute respec system
        common_bevy::systems::combat::queue::sync_queue_window_size, // Sync queue window size when attributes change
        engagement_cleanup::update_engagement_proximity.run_if(on_timer(Duration::from_secs(1))), // ADR-014: Update proximity tracking
        engagement_cleanup::cleanup_engagements.run_if(on_timer(Duration::from_secs(5))), // ADR-014: Clean up dead/abandoned engagements
        world::do_spawn,
        world::try_spawn,
    ));

    app.add_systems(Update, (
        actor::do_spawn_discover,   // Discover initial chunks after spawn
        actor::try_discover_chunk,  // Generates chunks, sends ChunkData for all rings
        engagement_spawner::try_spawn_engagement.after(actor::try_discover_chunk), // ADR-014: Validate and request engagement spawns
        engagement_spawner::do_spawn_engagement, // ADR-014: Create engagements from validated requests
        actor::try_discover,        // Legacy tile discovery (for compatibility)
        common_bevy::systems::combat::resources::process_respawn, // Process respawn timers, teleport to origin
    ));

    app.add_systems(PostUpdate, (
        aoi::update_area_of_interest,
        renet::send_do.after(aoi::update_area_of_interest),
        renet::cleanup_despawned.after(renet::send_do),
    ));


    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8, qrz::HexOrientation::FlatTop)));

    app.init_resource::<Lobby>();
    app.init_resource::<InputQueues>();
    let terrain = Terrain::default();
    let spawn_z = terrain.get(0, 0) + 1;
    app.insert_resource(common_bevy::components::resources::SpawnPoint(qrz::Qrz { q: 0, r: 0, z: spawn_z }));
    app.insert_resource(terrain);
    app.init_resource::<RunTime>();
    app.init_resource::<WorldDiscoveryCache>();
    app.init_resource::<EngagementBudget>(); // ADR-014: Track engagement budget per zone

    app.run();
}
