use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};
use common_bevy::message::{Do, Try};

use crate::systems::{combat, npc_ability_usage, reaction_queue, targeting};

/// Combat as the server runs it, with no networking: damage and threats,
/// NPC targeting, auto-attacks and signature abilities, the reaction queue,
/// resources, combat state, death and respawn.
///
/// Both the live server and the balance arena install it, so the arena
/// fights by the rules players meet. Movement, behaviour (`BehaviourPlugin`)
/// and the removal of the dead (`renet::cleanup_despawned`) are the
/// installer's, since the live server orders removal after the network send.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Do>();
        app.add_message::<Try>();
        app.init_resource::<crate::resources::RunTime>();

        app.add_observer(combat::process_deal_damage);
        app.add_observer(combat::resolve_threat);

        app.add_systems(FixedUpdate, (
            common_bevy::systems::combat::resources::regenerate_resources, // Handles all resource regen including leash health regen (100 HP/sec for Returning NPCs)
            common_bevy::systems::combat::state::update_combat_state,
            common_bevy::systems::combat::recovery::global_recovery_system, // Tick down recovery lockout
            common_bevy::systems::combat::synergies::synergy_cleanup_system, // Clean up expired synergies
            reaction_queue::process_expired_threats,
        ));

        app.add_systems(Update, (
            targeting::update_targets, // Update targets every frame (detects when targets move)
            combat::process_passive_auto_attack.run_if(on_timer(Duration::from_millis(500))), // Auto-attack passive for NPCs only (check every 0.5s)
            npc_ability_usage::npc_ability_usage.run_if(on_timer(Duration::from_millis(500))), // NPCs use signature abilities (check every 0.5s for responsive Defender counters)
            combat::validate_ability_prerequisites,
            combat::abilities::auto_attack::handle_auto_attack,
            combat::abilities::overpower::handle_overpower,
            combat::abilities::lunge::handle_lunge,
            combat::abilities::counter::handle_counter,  // Counter ability
            combat::abilities::kick::handle_kick,        // Kick: reactive knockback
            combat::abilities::deflect::handle_deflect,
            reaction_queue::process_dismiss, // Dismiss front queue threat (no GCD/lockout)
            common_bevy::systems::combat::resources::check_death, // Check for death from ANY source
            common_bevy::systems::combat::resources::process_respawn,
            common_bevy::systems::combat::queue::sync_queue_window_size, // Sync queue window size when attributes change
        ));
    }
}
