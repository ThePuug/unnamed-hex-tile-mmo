//! # Area of Interest (AOI) System

//! Manages entity visibility for networked clients. Each entity has a `LoadedBy`
//! component tracking which players can see it. When entities move, this system
//! updates membership and sends Spawn/Despawn events directly.
//!
//! A foe lurking beyond its reach of a player (`targeting::unseen`) is held
//! by no client of that player's: nothing about it is sent there, so no
//! client, however altered, learns where it lurks.

use std::collections::HashSet;

use bevy::prelude::*;
use renet::DefaultChannel;
use crate::network::ServerNet;

use common_bevy::{
    chunk::{FOV_CHUNK_RADIUS, CHUNK_SPACING, CHUNK_RADIUS},
    components::{
        behaviour::Side,
        loaded_by::LoadedBy,
        resources::RespawnTimer,
        status::Status,
        AttackRange, Loc,
    },
    message::{Do, Event},
    plugins::nntree::{NNTree, NearestNeighbor},
    systems::targeting::unseen,
};
use crate::{
    plugins::metrics::SystemTimings,
    resources::Lobby,
    systems::world::Synced,
};

/// AOI radius: entities within this distance are visible to players.
/// Covers FOV_CHUNK_RADIUS + 1 buffer chunk in all directions (hex chunks).
const AOI_RADIUS: i32 = (FOV_CHUNK_RADIUS as i32 + 1) * CHUNK_SPACING as i32 + CHUNK_RADIUS as i32;

/// Exit radius: hysteresis buffer to prevent enter/exit flicker at the boundary.
pub(crate) const EXIT_RADIUS: i32 = AOI_RADIUS + CHUNK_SPACING as i32;

/// What a player's sight of an actor turns on beside distance: who is
/// hostile to whom, and whether it lurks beyond its reach.
type Sight = (&'static Loc, Option<&'static Status>, Option<&'static AttackRange>, Option<&'static Side>);

/// Whether `seer`'s client is kept from `seen`: a foe of its lurking beyond
/// its own reach of it, as no foe on the server sees it (`targeting::unseen`).
fn withheld(sights: &Query<Sight>, seen: Entity, seer: Entity) -> bool {
    let (Ok((at, status, reach, side)), Ok((from, _, _, own))) = (sights.get(seen), sights.get(seer)) else { return false };
    own.zip(side).is_some_and(|(own, theirs)| own.is_hostile_to(*theirs))
        && unseen(status, reach.copied().unwrap_or_default().0, at.flat_distance(from))
}

/// Puts `ent` on `player`'s client, or takes it off, as `shown` says,
/// wherever its `LoadedBy` holds otherwise.
fn load(
    loaded_by: &mut LoadedBy,
    ent: Entity,
    player: Entity,
    client_id: renet::ClientId,
    shown: bool,
    actors: &Query<Synced, Without<RespawnTimer>>,
    conn: &mut ServerNet,
) {
    if shown && loaded_by.players.insert(player) {
        if let Ok(synced) = actors.get(ent) {
            for event in synced.events(ent) {
                conn.send(client_id, DefaultChannel::ReliableOrdered, &event);
            }
        }
    } else if !shown && loaded_by.players.remove(&player) {
        conn.send(client_id, DefaultChannel::ReliableOrdered, &Do { event: Event::Despawn { ent, died: false } });
    }
}

/// Updates LoadedBy membership when entities move, or begin or end a lurk.

/// When entity E's Loc changes:
/// 1. Query NNTree for nearby entities within AOI_RADIUS
/// 2. For each nearby entity X:
///    - If X is a player not in E.LoadedBy → add, send Spawn(E) to X
///    - If E is a player not in X.LoadedBy → add, send Spawn(X) to E
///    - Either way, where the one is a foe lurking beyond its reach of the
///      other, remove instead, sending Despawn
/// 3. For each player P in E.LoadedBy not in nearby set → remove, send Despawn(E) to P
/// 4. If E is a player: check entities at EXIT_RADIUS that have E in LoadedBy but are beyond AOI → clean up
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_area_of_interest(
    changed_query: Query<
        (Entity, Ref<Loc>, Option<&Status>),
        (Or<(Changed<Loc>, Changed<Status>)>, Without<RespawnTimer>, With<NearestNeighbor>),
    >,
    mut loaded_by_query: Query<&mut LoadedBy>,
    actor_query: Query<Synced, Without<RespawnTimer>>,
    sights: Query<Sight>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
    mut conn: ResMut<ServerNet>,
    timings: Res<SystemTimings>,
    mut buf_nearby: Local<Vec<Entity>>,
    mut buf_exited: Local<Vec<Entity>>,
    mut buf_nearby_set: Local<HashSet<Entity>>,
) {
    if changed_query.is_empty() { return; }
    let _t = timings.scope("aoi");
    for (ent, loc, status) in &changed_query {
        // A status changes each tick its effects count down; of those
        // changes only a lurk begun or ended changes who sees it
        let lurking = status.is_some_and(Status::is_lurking);
        let flipped = loaded_by_query.get_mut(ent).is_ok_and(|mut loaded_by| std::mem::replace(&mut loaded_by.lurking, lurking) != lurking);
        if !loc.is_changed() && !flipped {
            continue;
        }
        let loc = *loc;
        // A client sees the world as it: its character, or an actor it views
        let client_id = lobby.get_by_right(&ent);

        // Step 1+2: Find nearby entities and handle enters
        buf_nearby.clear();
        buf_nearby.extend(
            nntree
                .within_tiles(loc, AOI_RADIUS as i64)
                .filter(|nn| nn.ent != ent)
                .map(|nn| nn.ent),
        );

        for &other_ent in &*buf_nearby {
            // If other is a player → check if other should have E loaded
            if let Some(other_client_id) = lobby.get_by_right(&other_ent) {
                if let Ok(mut e_loaded_by) = loaded_by_query.get_mut(ent) {
                    let shown = !withheld(&sights, ent, other_ent);
                    load(&mut e_loaded_by, ent, other_ent, *other_client_id, shown, &actor_query, &mut conn);
                }
            }

            // If E is a player → check if E should have other loaded
            if let Some(client_id) = client_id {
                if let Ok(mut other_loaded_by) = loaded_by_query.get_mut(other_ent) {
                    let shown = !withheld(&sights, other_ent, ent);
                    load(&mut other_loaded_by, other_ent, ent, *client_id, shown, &actor_query, &mut conn);
                }
            }
        }

        // Step 3: Check exits from E.LoadedBy — players that are no longer nearby
        buf_nearby_set.clear();
        buf_nearby_set.extend(buf_nearby.iter().copied());

        if let Ok(mut e_loaded_by) = loaded_by_query.get_mut(ent) {
            buf_exited.clear();
            buf_exited.extend(
                e_loaded_by
                    .players
                    .iter()
                    .filter(|p| !buf_nearby_set.contains(p))
                    .copied(),
            );

            for &player_ent in &*buf_exited {
                e_loaded_by.players.remove(&player_ent);
                // Send Despawn(E) to player
                if let Some(client_id) = lobby.get_by_right(&player_ent) {
                    conn.send(*client_id, DefaultChannel::ReliableOrdered, &Do { event: Event::Despawn { ent, died: false } });
                }
            }
        }

        // Step 4: If E is a player, handle "player walks away from static entities"
        // Query at EXIT_RADIUS to find entities that still have E in their LoadedBy
        // but are now beyond AOI_RADIUS
        if let Some(client_id) = client_id {
            buf_nearby.clear();
            buf_nearby.extend(
                nntree
                    .within_tiles(loc, EXIT_RADIUS as i64)
                    .filter(|nn| nn.ent != ent)
                    .map(|nn| nn.ent),
            );

            for &other_ent in &*buf_nearby {
                // Skip entities already in nearby (within AOI) — they're fine
                if buf_nearby_set.contains(&other_ent) {
                    continue;
                }

                // other_ent is beyond AOI but within EXIT — check if E is in other's LoadedBy
                if let Ok(mut other_loaded_by) = loaded_by_query.get_mut(other_ent) {
                    if other_loaded_by.players.remove(&ent) {
                        // Send Despawn(other) to E
                        conn.send(*client_id, DefaultChannel::ReliableOrdered, &Do { event: Event::Despawn { ent: other_ent, died: false } });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use common_bevy::components::status::Timed;
    use qrz::Qrz;

    #[test]
    fn a_players_client_is_kept_from_a_foe_lurking_beyond_its_reach() {
        let mut world = World::new();
        let at = |q| Loc::new(Qrz { q, r: 0, z: 0 });
        let lurking = || Status { lurk: Some(Timed { pace: 1.0, remaining: 5.0 }), ..default() };
        let player = world.spawn((at(0), Side::PLAYERS)).id();
        let far = world.spawn((at(5), Side::WILD, lurking())).id();
        let near = world.spawn((at(2), Side::WILD, lurking())).id();
        let reaching = world.spawn((at(5), Side::WILD, lurking(), AttackRange(5))).id();
        let ally = world.spawn((at(5), Side::PLAYERS, lurking())).id();
        let standing = world.spawn((at(5), Side::WILD)).id();

        let kept = world.run_system_once(move |sights: Query<Sight>| {
            [far, near, reaching, ally, standing].map(|seen| withheld(&sights, seen, player))
        }).unwrap();

        assert_eq!(kept, [true, false, false, false, false], "only the foe lurking beyond its own reach is kept away");
    }
}
