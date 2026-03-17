//! # Area of Interest (AOI) System
//!
//! Manages entity visibility for networked clients. Each entity has a `LoadedBy`
//! component tracking which players can see it. When entities move, this system
//! updates membership and sends Spawn/Despawn events directly.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use ::renet::DefaultChannel;

use common_bevy::{
    chunk::{FOV_CHUNK_RADIUS, CHUNK_SPACING, CHUNK_RADIUS},
    components::{
        behaviour::PlayerControlled,
        entity_type::EntityType,
        heading::Heading,
        loaded_by::LoadedBy,
        resources::{CombatState, Health, Mana, RespawnTimer, Stamina},
        ActorAttributes, Loc,
    },
    plugins::nntree::{NNTree, NearestNeighbor},
};
use crate::{
    resources::Lobby,
    systems::world::generate_actor_spawn_events,
};

/// AOI radius: entities within this distance are visible to players.
/// Covers FOV_CHUNK_RADIUS + 1 buffer chunk in all directions (hex chunks).
pub const AOI_RADIUS: i32 = (FOV_CHUNK_RADIUS as i32 + 1) * CHUNK_SPACING as i32 + CHUNK_RADIUS as i32;
const AOI_RADIUS_SQ: i64 = AOI_RADIUS as i64 * AOI_RADIUS as i64;

/// Exit radius: hysteresis buffer to prevent enter/exit flicker at the boundary.
const EXIT_RADIUS: i32 = AOI_RADIUS + CHUNK_SPACING as i32;
const EXIT_RADIUS_SQ: i64 = EXIT_RADIUS as i64 * EXIT_RADIUS as i64;

/// Updates LoadedBy membership when entities move.
///
/// When entity E's Loc changes:
/// 1. Query NNTree for nearby entities within AOI_RADIUS
/// 2. For each nearby entity X:
///    - If X is a player not in E.LoadedBy → add, send Spawn(E) to X
///    - If E is a player not in X.LoadedBy → add, send Spawn(X) to E
/// 3. For each player P in E.LoadedBy not in nearby set → remove, send Despawn(E) to P
/// 4. If E is a player: check entities at EXIT_RADIUS that have E in LoadedBy but are beyond AOI → clean up
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_area_of_interest(
    changed_query: Query<
        (Entity, &Loc, &NearestNeighbor, Option<&PlayerControlled>),
        (Changed<Loc>, Without<RespawnTimer>),
    >,
    mut loaded_by_query: Query<&mut LoadedBy>,
    actor_query: Query<(
        &Loc,
        &EntityType,
        Option<&ActorAttributes>,
        Option<&PlayerControlled>,
        Option<&Heading>,
        Option<&Health>,
        Option<&Stamina>,
        Option<&Mana>,
        Option<&CombatState>,
    ), Without<RespawnTimer>>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
    mut conn: ResMut<RenetServer>,
) {
    for (ent, loc, _nn, player_controlled) in &changed_query {
        let is_player = player_controlled.is_some();

        // Step 1+2: Find nearby entities and handle enters
        let nearby: Vec<Entity> = nntree
            .locate_within_distance(*loc, AOI_RADIUS_SQ)
            .filter(|nn| nn.ent != ent)
            .map(|nn| nn.ent)
            .collect();

        for &other_ent in &nearby {
            // If other is a player → check if other should have E loaded
            if let Some(other_client_id) = lobby.get_by_right(&other_ent) {
                // other_ent is a player: ensure E is in E.LoadedBy with other_ent
                if let Ok(mut e_loaded_by) = loaded_by_query.get_mut(ent) {
                    if !e_loaded_by.players.contains(&other_ent) {
                        e_loaded_by.players.insert(other_ent);
                        // Send Spawn(E) to other player
                        if let Ok((_, &typ, attrs, pc, heading, health, stamina, mana, combat_state)) = actor_query.get(ent) {
                            let spawn_events = generate_actor_spawn_events(
                                ent, typ, **loc,
                                attrs.copied(), pc, heading, health, stamina, mana, combat_state,
                            );
                            for event in spawn_events {
                                let message = bincode::serde::encode_to_vec(event, bincode::config::legacy()).unwrap();
                                conn.send_message(*other_client_id, DefaultChannel::ReliableOrdered, message);
                            }
                        }
                    }
                }
            }

            // If E is a player → check if E should have other loaded
            if is_player {
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    if let Ok(mut other_loaded_by) = loaded_by_query.get_mut(other_ent) {
                        if !other_loaded_by.players.contains(&ent) {
                            other_loaded_by.players.insert(ent);
                            // Send Spawn(other) to E
                            if let Ok((&other_loc, &other_typ, other_attrs, other_pc, other_heading, other_health, other_stamina, other_mana, other_combat_state)) = actor_query.get(other_ent) {
                                let spawn_events = generate_actor_spawn_events(
                                    other_ent, other_typ, *other_loc,
                                    other_attrs.copied(), other_pc, other_heading, other_health, other_stamina, other_mana, other_combat_state,
                                );
                                for event in spawn_events {
                                    let message = bincode::serde::encode_to_vec(event, bincode::config::legacy()).unwrap();
                                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Step 3: Check exits from E.LoadedBy — players that are no longer nearby
        let nearby_set: std::collections::HashSet<Entity> = nearby.iter().copied().collect();

        if let Ok(mut e_loaded_by) = loaded_by_query.get_mut(ent) {
            let exited: Vec<Entity> = e_loaded_by
                .players
                .iter()
                .filter(|p| !nearby_set.contains(p))
                .copied()
                .collect();

            for player_ent in exited {
                e_loaded_by.players.remove(&player_ent);
                // Send Despawn(E) to player
                if let Some(client_id) = lobby.get_by_right(&player_ent) {
                    let message = bincode::serde::encode_to_vec(
                        common_bevy::message::Do { event: common_bevy::message::Event::Despawn { ent } },
                        bincode::config::legacy(),
                    ).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
        }

        // Step 4: If E is a player, handle "player walks away from static entities"
        // Query at EXIT_RADIUS to find entities that still have E in their LoadedBy
        // but are now beyond AOI_RADIUS
        if is_player {
            let exit_nearby: Vec<Entity> = nntree
                .locate_within_distance(*loc, EXIT_RADIUS_SQ)
                .filter(|nn| nn.ent != ent)
                .map(|nn| nn.ent)
                .collect();

            let client_id = lobby.get_by_right(&ent);

            for other_ent in exit_nearby {
                // Skip entities already in nearby (within AOI) — they're fine
                if nearby_set.contains(&other_ent) {
                    continue;
                }

                // other_ent is beyond AOI but within EXIT — check if E is in other's LoadedBy
                if let Ok(mut other_loaded_by) = loaded_by_query.get_mut(other_ent) {
                    if other_loaded_by.players.remove(&ent) {
                        // Send Despawn(other) to E
                        if let Some(client_id) = client_id {
                            let message = bincode::serde::encode_to_vec(
                                common_bevy::message::Do { event: common_bevy::message::Event::Despawn { ent: other_ent } },
                                bincode::config::legacy(),
                            ).unwrap();
                            conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                        }
                    }
                }
            }
        }
    }
}
