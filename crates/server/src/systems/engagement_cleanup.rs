//! # Engagement Cleanup System (ADR-014)
//!
//! Removes completed or abandoned engagements and unregisters from budget.
//! Runs periodically to keep world state clean.

use bevy::prelude::*;
use std::time::Duration;

use common_bevy::{
    components::{
        behaviour::Behaviour,
        engagement::{Engagement, EngagementMember, LastPlayerProximity},
        Loc,
    },
    message::{Do, Event},
};
use crate::resources::engagement_budget::EngagementBudget;

/// Abandonment timeout (30 seconds with no players nearby)
const ABANDONMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// Proximity range for abandonment check.
/// Must exceed the minimum client eviction distance to prevent ghost NPCs.
/// With FOV_CHUNK_RADIUS=5 and CHUNK_SPACING=19, eviction distance is ~123 tiles.
/// Set to 150 to exceed AOI EXIT_RADIUS (142) with safety margin.
const PROXIMITY_RANGE: i32 = 150;

/// System that cleans up completed or abandoned engagements
///
/// Cleanup triggers:
/// 1. All NPCs dead (all child entities despawned)
/// 2. Abandoned (no players within 60 tiles for 30 seconds)
///
/// Actions on cleanup:
/// - Despawn engagement entity
/// - Unregister from budget
pub fn cleanup_engagements(
    mut commands: Commands,
    mut budget: ResMut<EngagementBudget>,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    engagement_query: Query<(Entity, &Engagement, &Loc, &LastPlayerProximity)>,
    npc_query: Query<&EngagementMember>,
    player_query: Query<(&Loc, &Behaviour), Without<Engagement>>,
) {
    for (engagement_entity, engagement, engagement_loc, last_proximity) in engagement_query.iter() {
        let mut should_cleanup = false;

        // Check 1: All NPCs dead?
        let alive_count = engagement.spawned_npcs.iter().filter(|&npc_entity| {
            npc_query.get(*npc_entity).is_ok()
        }).count();
        let all_npcs_dead = alive_count == 0;

        if all_npcs_dead {
            should_cleanup = true;
        } else {
            // Check 2: Abandoned? (no players nearby for 30s)
            let is_abandoned = last_proximity.is_abandoned(time.elapsed(), ABANDONMENT_TIMEOUT);

            if is_abandoned {
                // Double-check: are there really no players nearby?
                let closest_player_dist = player_query.iter()
                    .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
                    .map(|(player_loc, _)| engagement_loc.flat_distance(&**player_loc))
                    .min()
                    .unwrap_or(i32::MAX);

                let any_player_nearby = closest_player_dist < PROXIMITY_RANGE;

                if !any_player_nearby {
                    should_cleanup = true;
                }
            }
        }

        if should_cleanup {
            // Unregister from budget
            budget.unregister_engagement(engagement.zone_id);

            // Emit Despawn events for all NPCs — send_do routes via LoadedBy,
            // cleanup_despawned handles actual entity removal
            for &npc_entity in &engagement.spawned_npcs {
                if npc_query.get(npc_entity).is_ok() {
                    writer.write(Do { event: Event::Despawn { ent: npc_entity } });
                }
            }

            // Despawn engagement entity directly (no network component, clients don't know about it)
            commands.entity(engagement_entity).despawn();
        }
    }
}

/// System that updates player proximity tracking for engagements
///
/// Run frequently to keep proximity timestamps fresh
pub fn update_engagement_proximity(
    mut engagement_query: Query<(&Loc, &mut LastPlayerProximity), With<Engagement>>,
    player_query: Query<(&Loc, &Behaviour), Without<Engagement>>,
    time: Res<Time>,
) {
    for (engagement_loc, mut last_proximity) in engagement_query.iter_mut() {
        // Check if any ACTUAL player (not NPCs) is within proximity range
        let any_player_nearby = player_query.iter()
            .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
            .any(|(player_loc, _)| {
                engagement_loc.flat_distance(&**player_loc) < PROXIMITY_RANGE
            });

        if any_player_nearby {
            // Update timestamp - player is nearby
            last_proximity.update(time.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{
        chunk::{CHUNK_SPACING, FOV_CHUNK_RADIUS, ChunkId, chunk_tiles},
        components::Loc,
    };

    /// CRITICAL INVARIANT TEST (ADR-014)
    ///
    /// Validates that PROXIMITY_RANGE is large enough to guarantee clients have evicted
    /// chunks before engagement abandonment triggers.
    ///
    /// Architecture:
    /// - Clients evict chunks outside hex distance FOV_CHUNK_RADIUS + 1
    /// - Server abandons engagements when no players within PROXIMITY_RANGE for 30s
    /// - Engagements spawn at chunk centers
    ///
    /// For this to work correctly:
    /// PROXIMITY_RANGE must be > minimum distance at which clients evict chunks
    ///
    /// This test calculates the minimum eviction distance and ensures PROXIMITY_RANGE
    /// provides a safety buffer.
    #[test]
    fn test_proximity_range_exceeds_minimum_client_eviction_distance() {
        use common_bevy::chunk::chunk_hex_distance;

        // Engagement spawns at chunk center
        let engagement_chunk = ChunkId(0, 0);
        let engagement_loc = Loc::new(engagement_chunk.center());

        // Client keeps chunks within hex distance FOV_CHUNK_RADIUS + 1
        // Minimum eviction occurs when player moves to chunk just outside this hex ring.
        let eviction_radius = (FOV_CHUNK_RADIUS + 1) as i32;

        // Test chunks at hex distance exactly eviction_radius + 1 (just evicted)
        let mut min_eviction_distance = i32::MAX;
        let r = eviction_radius + 1;

        for dq in -r..=r {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            for dr in dr_min..=dr_max {
                let chunk_dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
                // Only chunks at exactly eviction_radius + 1 (just evicted boundary)
                if chunk_dist != eviction_radius + 1 {
                    continue;
                }

                let player_chunk = ChunkId(engagement_chunk.0 + dq, engagement_chunk.1 + dr);

                // Check all tiles in the player's hex chunk
                for (q, r) in chunk_tiles(player_chunk) {
                    let player_loc = Loc::new(qrz::Qrz { q, r, z: 0 });
                    let distance = engagement_loc.flat_distance(&player_loc);
                    min_eviction_distance = min_eviction_distance.min(distance);
                }
            }
        }

        // PROXIMITY_RANGE must be larger than the minimum eviction distance
        assert!(
            PROXIMITY_RANGE > min_eviction_distance,
            "PROXIMITY_RANGE ({}) must exceed minimum client eviction distance ({}) to guarantee \
             chunks are evicted before abandonment. Current margin: {} tiles.\n\
             \n\
             This ensures no 'ghost NPCs' remain on clients when engagements are abandoned.\n\
             If this test fails, either:\n\
             1. Increase PROXIMITY_RANGE, or\n\
             2. Decrease FOV_CHUNK_RADIUS + 1 (eviction buffer), or\n\
             3. Decrease CHUNK_SPACING (not recommended - affects entire chunk system)",
            PROXIMITY_RANGE,
            min_eviction_distance,
            PROXIMITY_RANGE - min_eviction_distance
        );

        println!("Abandonment safety margin: {} tiles", PROXIMITY_RANGE - min_eviction_distance);
        println!("  PROXIMITY_RANGE: {}", PROXIMITY_RANGE);
        println!("  Min eviction distance: {}", min_eviction_distance);
        println!("  CHUNK_SPACING: {}", CHUNK_SPACING);
        println!("  FOV_CHUNK_RADIUS + 1: {}", FOV_CHUNK_RADIUS + 1);
    }

    /// Test that validates the abandonment distance provides adequate safety margin
    /// even in worst-case scenarios
    #[test]
    fn test_abandonment_has_adequate_safety_margin() {
        // Calculate minimum eviction distance (from previous test logic)
        let engagement_chunk = ChunkId(0, 0);
        let engagement_loc = Loc::new(engagement_chunk.center());
        let eviction_radius = (FOV_CHUNK_RADIUS + 1) as i32;

        let mut min_eviction_distance = i32::MAX;
        let r = eviction_radius + 1;
        for dq in -r..=r {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            for dr in dr_min..=dr_max {
                let chunk_dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
                if chunk_dist != eviction_radius + 1 {
                    continue;
                }

                let player_chunk = ChunkId(engagement_chunk.0 + dq, engagement_chunk.1 + dr);
                for (q, r) in chunk_tiles(player_chunk) {
                    let player_loc = Loc::new(qrz::Qrz { q, r, z: 0 });
                    let distance = engagement_loc.flat_distance(&player_loc);
                    min_eviction_distance = min_eviction_distance.min(distance);
                }
            }
        }

        let safety_margin = PROXIMITY_RANGE - min_eviction_distance;

        // Require at least 2 tiles of safety margin
        assert!(
            safety_margin >= 2,
            "Safety margin ({} tiles) should be at least 2 tiles to handle edge cases and timing",
            safety_margin
        );
    }
}
