//! # Engagement Cleanup System (ADR-014)
//!
//! Removes completed or abandoned engagements and unregisters from budget.
//! Runs periodically to keep world state clean.

use bevy::prelude::*;
use std::time::Duration;

use crate::{
    common::components::{
        behaviour::Behaviour,
        engagement::{Engagement, EngagementMember, LastPlayerProximity},
        Loc,
    },
    server::resources::engagement_budget::EngagementBudget,
};

/// Abandonment timeout (30 seconds with no players nearby)
const ABANDONMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// Proximity range for abandonment check (60 tiles)
const PROXIMITY_RANGE: i16 = 60;

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
                // Find closest ACTUAL player (not NPCs)
                let closest_player_dist = player_query.iter()
                    .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
                    .map(|(player_loc, _)| engagement_loc.flat_distance(&**player_loc))
                    .min()
                    .unwrap_or(i16::MAX);

                let any_player_nearby = closest_player_dist < PROXIMITY_RANGE;

                if !any_player_nearby {
                    should_cleanup = true;
                }
            }
        }

        if should_cleanup {
            // Unregister from budget
            budget.unregister_engagement(engagement.zone_id);

            // Despawn all NPCs (server-side cleanup, no broadcast needed since clients evict chunks first)
            for &npc_entity in &engagement.spawned_npcs {
                if npc_query.get(npc_entity).is_ok() {
                    commands.entity(npc_entity).despawn_recursive();
                }
            }

            // Despawn engagement entity
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
    use crate::common::{
        chunk::{CHUNK_SIZE, FOV_CHUNK_RADIUS, ChunkId, chunk_to_tile},
        components::Loc,
    };

    /// CRITICAL INVARIANT TEST (ADR-014)
    ///
    /// Validates that PROXIMITY_RANGE is large enough to guarantee clients have evicted
    /// chunks before engagement abandonment triggers.
    ///
    /// Architecture:
    /// - Clients evict chunks outside FOV_CHUNK_RADIUS + 1 (square of radius 3)
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
        // Engagement spawns at chunk center
        let engagement_chunk = ChunkId(0, 0);
        let engagement_loc = Loc::new(engagement_chunk.center());

        // Find the minimum distance from engagement to a player whose client has just
        // evicted the engagement's chunk.
        //
        // Client keeps chunks in square of radius (FOV_CHUNK_RADIUS + 1)
        // Minimum eviction occurs when player moves to chunk just outside this square.
        let eviction_radius = (FOV_CHUNK_RADIUS + 1) as i16;

        // Test all edges of the eviction boundary (just outside the square)
        let mut min_eviction_distance = i16::MAX;

        // Test chunks just outside each edge of the square
        for dq in -(eviction_radius + 1)..=(eviction_radius + 1) {
            for dr in -(eviction_radius + 1)..=(eviction_radius + 1) {
                // Skip chunks inside the square
                if dq.abs() <= eviction_radius && dr.abs() <= eviction_radius {
                    continue;
                }

                // Skip chunks too far from the boundary (we want the minimum distance)
                if dq.abs() > eviction_radius + 1 || dr.abs() > eviction_radius + 1 {
                    continue;
                }

                // This is a chunk just outside the eviction boundary
                let player_chunk = ChunkId(engagement_chunk.0 + dq, engagement_chunk.1 + dr);

                // Player could be at any position within their chunk
                // For minimum distance, they're at the corner nearest to the engagement
                for player_offset_q in 0..CHUNK_SIZE as u8 {
                    for player_offset_r in 0..CHUNK_SIZE as u8 {
                        let player_tile = chunk_to_tile(player_chunk, player_offset_q, player_offset_r);
                        let player_loc = Loc::new(player_tile);

                        let distance = engagement_loc.flat_distance(&player_loc);
                        min_eviction_distance = min_eviction_distance.min(distance);
                    }
                }
            }
        }

        // PROXIMITY_RANGE must be larger than the minimum eviction distance
        // to ensure clients have evicted before abandonment triggers
        assert!(
            PROXIMITY_RANGE > min_eviction_distance,
            "PROXIMITY_RANGE ({}) must exceed minimum client eviction distance ({}) to guarantee \
             chunks are evicted before abandonment. Current margin: {} tiles.\n\
             \n\
             This ensures no 'ghost NPCs' remain on clients when engagements are abandoned.\n\
             If this test fails, either:\n\
             1. Increase PROXIMITY_RANGE, or\n\
             2. Decrease FOV_CHUNK_RADIUS + 1 (eviction buffer), or\n\
             3. Decrease CHUNK_SIZE (not recommended - affects entire chunk system)",
            PROXIMITY_RANGE,
            min_eviction_distance,
            PROXIMITY_RANGE - min_eviction_distance
        );

        // Log the safety margin for visibility
        println!("✓ Abandonment safety margin: {} tiles", PROXIMITY_RANGE - min_eviction_distance);
        println!("  PROXIMITY_RANGE: {}", PROXIMITY_RANGE);
        println!("  Min eviction distance: {}", min_eviction_distance);
        println!("  CHUNK_SIZE: {}", CHUNK_SIZE);
        println!("  FOV_CHUNK_RADIUS + 1: {}", FOV_CHUNK_RADIUS + 1);
    }

    /// Test that validates the abandonment distance provides adequate safety margin
    /// even in worst-case scenarios
    #[test]
    fn test_abandonment_has_adequate_safety_margin() {
        // Calculate minimum eviction distance (from previous test logic)
        let engagement_chunk = ChunkId(0, 0);
        let engagement_loc = Loc::new(engagement_chunk.center());
        let eviction_radius = (FOV_CHUNK_RADIUS + 1) as i16;

        let mut min_eviction_distance = i16::MAX;
        for dq in -(eviction_radius + 1)..=(eviction_radius + 1) {
            for dr in -(eviction_radius + 1)..=(eviction_radius + 1) {
                if dq.abs() <= eviction_radius && dr.abs() <= eviction_radius {
                    continue;
                }
                if dq.abs() > eviction_radius + 1 || dr.abs() > eviction_radius + 1 {
                    continue;
                }

                let player_chunk = ChunkId(engagement_chunk.0 + dq, engagement_chunk.1 + dr);
                for player_offset_q in 0..CHUNK_SIZE as u8 {
                    for player_offset_r in 0..CHUNK_SIZE as u8 {
                        let player_tile = chunk_to_tile(player_chunk, player_offset_q, player_offset_r);
                        let player_loc = Loc::new(player_tile);
                        let distance = engagement_loc.flat_distance(&player_loc);
                        min_eviction_distance = min_eviction_distance.min(distance);
                    }
                }
            }
        }

        let safety_margin = PROXIMITY_RANGE - min_eviction_distance;

        // Require at least 2 tiles of safety margin
        // This accounts for:
        // - Timing uncertainties (client eviction runs every 5s)
        // - Network lag between client movement and server updates
        // - Edge cases in distance calculations
        assert!(
            safety_margin >= 2,
            "Safety margin ({} tiles) should be at least 2 tiles to handle edge cases and timing",
            safety_margin
        );
    }
}
