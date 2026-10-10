//! # Engagement Cleanup System

//! Removes completed or abandoned engagements.
//! Runs periodically to keep world state clean.

use bevy::prelude::*;
use std::time::Duration;

use combat::engagement::{Engagement, EngagementMember, LastPlayerProximity};
use common_bevy::{
    components::Loc,
    message::{Do, Event},
    moment::Moment,
};

use crate::{resources::Lobby, systems::dens::EngagementEnded};

/// Abandonment timeout (30 seconds with no one watching)
const ABANDONMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// How far a client watches an engagement from. An engagement outlives a
/// player's area of interest by a margin: abandoned inside it, its NPCs
/// would despawn on a client that still holds them.
const PROXIMITY_RANGE: i32 = crate::systems::aoi::EXIT_RADIUS + 8;

/// System that cleans up completed or abandoned engagements

/// Cleanup triggers:
/// 1. All NPCs dead (all child entities despawned)
/// 2. Abandoned (no client watching within `PROXIMITY_RANGE` for 30 seconds)

/// Actions on cleanup:
/// - Despawn engagement entity
/// - Say which way it ended ([`EngagementEnded`]), so a den is cleared only
///   when its pack died
pub fn cleanup_engagements(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut ended: MessageWriter<EngagementEnded>,
    time: Res<Time>,
    engagement_query: Query<(Entity, &Engagement, &Loc, &LastPlayerProximity)>,
    npc_query: Query<&EngagementMember>,
    lobby: Res<Lobby>,
    locs: Query<&Loc, Without<Engagement>>,
) {
    for (engagement_entity, engagement, engagement_loc, last_proximity) in engagement_query.iter() {
        let all_npcs_dead = !engagement.spawned_npcs.iter().any(|&npc_entity| npc_query.get(npc_entity).is_ok());
        let should_cleanup = all_npcs_dead
            || (last_proximity.is_abandoned(Moment::ZERO + time.elapsed(), ABANDONMENT_TIMEOUT) && !watched(engagement_loc, &lobby, &locs));

        if should_cleanup {
            // Emit Despawn events for all NPCs — send_do routes via LoadedBy,
            // cleanup_despawned handles actual entity removal
            for &npc_entity in &engagement.spawned_npcs {
                if npc_query.get(npc_entity).is_ok() {
                    writer.write(Do { event: Event::Despawn { ent: npc_entity } });
                }
            }

            // Despawn engagement entity directly (no network component, clients don't know about it)
            commands.entity(engagement_entity).despawn();
            ended.write(EngagementEnded { engagement: engagement_entity, cleared: all_npcs_dead });
        }
    }
}

/// Whether a client sees the world from within `PROXIMITY_RANGE` of `at`:
/// through its character, or through an actor it views, which the lobby
/// holds in its character's place.
fn watched(at: &Loc, lobby: &Lobby, locs: &Query<&Loc, Without<Engagement>>) -> bool {
    lobby.right_values()
        .filter_map(|ent| locs.get(*ent).ok())
        .any(|loc| at.flat_distance(&**loc) < PROXIMITY_RANGE)
}

/// Refreshes each engagement's proximity timestamp while a client watches it.
pub fn update_engagement_proximity(
    mut engagement_query: Query<(&Loc, &mut LastPlayerProximity), With<Engagement>>,
    lobby: Res<Lobby>,
    locs: Query<&Loc, Without<Engagement>>,
    time: Res<Time>,
) {
    for (engagement_loc, mut last_proximity) in engagement_query.iter_mut() {
        if watched(engagement_loc, &lobby, &locs) {
            last_proximity.update(Moment::ZERO + time.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fight_seen_through_a_viewed_actor_is_watched() {
        use bevy::ecs::system::RunSystemOnce;
        use qrz::Qrz;

        let mut world = World::new();
        let at = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        // No character in the world: the client sees as a fighter of the fight
        let fighter = world.spawn(Loc::new(Qrz { q: 2, r: 0, z: 0 })).id();
        world.insert_resource(Lobby::default());

        let seen = |world: &mut World| world
            .run_system_once(move |lobby: Res<Lobby>, locs: Query<&Loc, Without<Engagement>>| watched(&at, &lobby, &locs))
            .unwrap();
        assert!(!seen(&mut world), "an actor no client sees as watches nothing");
        world.resource_mut::<Lobby>().insert(7, fighter);
        assert!(seen(&mut world), "the viewed fighter keeps its fight");
    }
}
