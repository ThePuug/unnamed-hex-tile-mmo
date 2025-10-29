use bevy::prelude::*;
use std::time::Duration;

use crate::common::{
    components::{*, behaviour::*, resources::*},
    message::{Event, *},
    plugins::nntree::*,
};

/// Update combat state for all entities
/// Runs in FixedUpdate on server only
/// Checks for combat exit conditions:
/// - 5 seconds since last_action AND
/// - No hostile entities within 20 hexes
pub fn update_combat_state(
    mut writer: EventWriter<Do>,
    mut query: Query<(Entity, &Loc, &mut CombatState, Option<&Behaviour>)>,
    entity_query: Query<(Entity, &Loc, Option<&Behaviour>)>,
    nntree: Res<NNTree>,
    time: Res<Time>,
) {
    let current_time = time.elapsed();
    let combat_exit_timeout = Duration::from_secs(5);

    for (ent, loc, mut combat_state, behaviour) in &mut query {
        if !combat_state.in_combat {
            continue; // Already out of combat
        }

        // Check if 5 seconds have passed since last combat action
        let time_since_action = current_time.saturating_sub(combat_state.last_action);
        if time_since_action < combat_exit_timeout {
            continue; // Still within 5 second window
        }

        // Check for hostile entities within 20 hexes
        let has_nearby_hostile = has_hostile_within_radius(
            ent,
            loc,
            behaviour,
            &entity_query,
            &nntree,
            20,
        );

        if has_nearby_hostile {
            continue; // Hostile nearby, stay in combat
        }

        // Exit combat: timeout elapsed AND no hostiles nearby
        combat_state.in_combat = false;

        // Broadcast combat state change to clients
        writer.write(Do {
            event: Event::CombatState {
                ent,
                in_combat: false,
            },
        });
    }
}

/// Check if there are any hostile entities within the specified radius
/// MVP logic: All NPCs (non-Controlled Behaviour) are hostile to players and vice versa
fn has_hostile_within_radius(
    self_ent: Entity,
    self_loc: &Loc,
    self_behaviour: Option<&Behaviour>,
    entity_query: &Query<(Entity, &Loc, Option<&Behaviour>)>,
    nntree: &NNTree,
    radius: i16,
) -> bool {
    // Query NNTree for entities within radius (squared distance)
    let nearby = nntree.locate_within_distance(*self_loc, radius * radius);

    for other in nearby {
        if other.ent == self_ent {
            continue; // Skip self
        }

        // Check if this entity exists in our query
        let Ok((_, _, other_behaviour)) = entity_query.get(other.ent) else {
            continue; // Entity doesn't have required components
        };

        // MVP hostile detection logic:
        // - Players (Controlled) are hostile to NPCs (non-Controlled)
        // - NPCs are hostile to Players
        // - Players are NOT hostile to other players (no PvP in MVP)
        // - NPCs are NOT hostile to other NPCs
        let is_hostile = match (self_behaviour, other_behaviour) {
            // Player checking for hostiles
            (Some(Behaviour::Controlled), Some(Behaviour::Controlled)) => false, // Player vs Player = not hostile (no PvP)
            (Some(Behaviour::Controlled), _) => true, // Player vs NPC = hostile

            // NPC checking for hostiles
            (_, Some(Behaviour::Controlled)) => true, // NPC vs Player = hostile
            _ => false, // NPC vs NPC = not hostile
        };

        if is_hostile {
            return true;
        }
    }

    false
}

/// Helper function to enter combat state
/// Call this when dealing damage, taking damage, or using offensive abilities
pub fn enter_combat(
    ent: Entity,
    combat_state: &mut CombatState,
    time: &Time,
    writer: &mut EventWriter<Do>,
) {
    let was_in_combat = combat_state.in_combat;

    combat_state.in_combat = true;
    combat_state.last_action = time.elapsed();

    // Only broadcast if state changed
    if !was_in_combat {
        writer.write(Do {
            event: Event::CombatState {
                ent,
                in_combat: true,
            },
        });
    }
}
