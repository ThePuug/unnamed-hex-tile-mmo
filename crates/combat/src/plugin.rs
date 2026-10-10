use bevy::prelude::*;
use common_bevy::message::{Do, Try};

use crate::{self as combat, behaviour, reaction_queue, targeting};

/// Combat as the server runs it, with no networking: damage and threats,
/// NPC targeting, auto-attacks and signature abilities, the reaction queue,
/// resources, combat state, death and respawn.
///
/// Both the live server and the balance arena install it, so the arena
/// fights by the rules players meet. Movement, behaviour (`BehaviourPlugin`)
/// and the removal of the dead (`actor::cleanup_despawned`) are the
/// installer's, since the live server orders removal after the network send.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Do>();
        app.add_message::<Try>();
        app.init_resource::<crate::RunTime>();
        app.init_resource::<combat::dice::Dice>();
        app.init_resource::<common_bevy::tuning::Tuning>();
        app.insert_resource(behaviour::mind::Minds::tuned());
        app.register_required_components::<common_bevy::components::ActorAttributes, combat::dice::Rolls>();

        app.add_observer(combat::process_deal_damage);
        app.add_observer(combat::resolve_threat);
        app.add_observer(combat::resolve_dot_tick);
        app.add_observer(combat::landing::stumble);

        app.add_systems(FixedUpdate, (
            common_bevy::systems::combat::resources::regenerate_resources,
            common_bevy::systems::combat::state::update_combat_state,
            common_bevy::systems::combat::recovery::global_recovery_system,
            reaction_queue::tick_dots,
            combat::track_engagement,
            combat::intimidate,
        ));

        app.add_systems(Update, (
            targeting::update_targets,
            // Every ability, through the one gate, then what lands: a press
            // come due this frame takes its band before any of it lands
            (combat::abilities::use_abilities, reaction_queue::process_expired_threats).chain(),
            common_bevy::systems::combat::resources::check_death,
            common_bevy::systems::combat::resources::process_respawn,
        ));
    }
}

/// How NPCs fight: each chases, holds its place in its engagement and
/// chooses its moves, as the live server and the balance arena both run it.
pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<combat::dice::Dice>();
        app.register_required_components::<behaviour::chase::Chase, combat::dice::Rolls>();
        app.add_systems(
            FixedUpdate,
            (
                common_bevy::components::status::tick_status,
                behaviour::hex_assignment::assign_hexes,
                behaviour::chase::chase,
            )
        );
    }
}
