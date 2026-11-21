use bevy::prelude::*;
use common_bevy::message::{Do, Try};

use crate::{self as combat, behaviour, targeting};

/// Combat as the server runs it, with no networking: every ability through
/// one gate, activations, recharges and what lands, NPC targeting,
/// auto-attacks, runes, resources, combat state, death and respawn.
///
/// Both the live server and the balance arena install it, so the arena
/// fights by the rules players meet. Movement, behaviour (`BehaviourPlugin`)
/// and the removal of the dead (`actor::cleanup_despawned`) are the
/// installer's, since the live server orders removal after the network send.
/// A player's own step or turn mid-activation is the installer's to tell
/// (`combat::Cancel`).
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

        app.add_observer(combat::abilities::cancel);

        app.add_systems(FixedUpdate, (
            common_bevy::systems::combat::resources::regenerate_resources,
            common_bevy::systems::combat::state::update_combat_state,
            combat::landing::tick_wounds,
            combat::track_engagement,
        ));

        app.add_systems(Update, (
            targeting::update_targets,
            combat::abilities::stance::count,
            // Every ability through the one gate, the ended activations
            // landing first; then each rune's pulse
            (combat::abilities::use_abilities, combat::runes::control).chain(),
            common_bevy::systems::combat::resources::check_death,
            common_bevy::systems::combat::resources::process_respawn,
            combat::runes::lapse,
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
