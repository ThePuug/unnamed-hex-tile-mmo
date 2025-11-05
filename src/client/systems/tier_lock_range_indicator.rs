//! Tier Lock Range Indicator System
//!
//! Displays translucent yellow hexes showing targetable tiles when tier locked.
//!
//! # Design Requirements
//!
//! - Shows yellow translucent indicators for tiles in the locked tier range
//! - Updates immediately when tier lock changes (zero lag)
//! - Follows terrain elevation (terrain-following meshes)
//! - Clears indicators when tier lock is removed or player is unavailable
//! - Tier ranges (matching targeting.rs):
//!   - Tier 1 (Close): 1-2 hexes (melee range)
//!   - Tier 2 (Mid): 3-6 hexes (mid range)
//!   - Tier 3 (Far): 7-10 hexes (far range)
//!
//! # Implementation
//!
//! Uses a SINGLE entity with a combined mesh for all hex tiles to avoid entity churn.
//! Only recreates the mesh when the tier actually changes.

use bevy::{
    prelude::*,
    pbr::NotShadowCaster,
    render::primitives::Aabb,
};

use crate::{
    client::plugins::diagnostics::DiagnosticsState,
    common::{
        components::{tier_lock::*, *},
        resources::map::Map,
        systems::targeting::RangeTier,
    },
};

use qrz::Qrz;

/// Component marking the single tier lock range indicator entity
/// Stores the current tier and player location to avoid unnecessary mesh rebuilds
#[derive(Component)]
pub struct TierLockRangeIndicator {
    current_tier: Option<RangeTier>,
    current_player_loc: Option<Loc>,
}

/// Get the distance range for a tier (inclusive)
/// Must match the ranges defined in targeting.rs::get_range_tier()
fn get_tier_range(tier: RangeTier) -> (u32, u32) {
    match tier {
        RangeTier::Close => (1, 2),  // Tier 1: 1-2 hexes (melee range)
        RangeTier::Mid => (3, 6),    // Tier 2: 3-6 hexes (mid range)
        RangeTier::Far => (7, 10),   // Tier 3: 7+ hexes (far range, capped at 10 for performance)
    }
}

/// Setup system (runs once on startup)
pub fn setup(mut commands: Commands) {
    // Spawn the single indicator entity (initially hidden)
    commands.spawn((
        TierLockRangeIndicator {
            current_tier: None,
            current_player_loc: None,
        },
        Visibility::Hidden,
    ));
}

/// Update tier lock range indicators based on player's targeting state
///
/// Only rebuilds the mesh when the tier actually changes.
pub fn update(
    mut commands: Commands,
    local_player_query: Query<(Entity, &Loc, &TierLock), With<Actor>>,
    mut indicator_query: Query<(Entity, &mut TierLockRangeIndicator, &mut Visibility, Option<&MeshMaterial3d<StandardMaterial>>)>,
    input_queues: Res<crate::common::resources::InputQueues>,
    map: Res<Map>,
    diagnostics_state: Res<DiagnosticsState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Get the single indicator entity
    let Ok((indicator_ent, mut indicator, mut visibility, maybe_material)) = indicator_query.get_single_mut() else {
        return;
    };

    // Get local player (entity with InputQueue in the resource)
    let mut local_player_data = None;
    for (player_ent, player_loc, targeting_state) in &local_player_query {
        if input_queues.get(&player_ent).is_some() {
            local_player_data = Some((player_ent, player_loc, targeting_state));
            break;
        }
    }

    let Some((_player_ent, player_loc, targeting_state)) = local_player_data else {
        // No local player - hide indicator
        *visibility = Visibility::Hidden;
        indicator.current_tier = None;
        return;
    };

    // Check if tier locked
    let tier_lock = targeting_state.get();

    if tier_lock.is_none() {
        // Not tier locked - hide indicator
        *visibility = Visibility::Hidden;
        indicator.current_tier = None;
        return;
    }

    let tier = tier_lock.unwrap();

    // Check if tier or player location changed - if not, no need to rebuild mesh
    if indicator.current_tier == Some(tier) && indicator.current_player_loc == Some(*player_loc) {
        return; // No change, keep existing mesh
    }

    // Tier or player location changed - rebuild mesh
    indicator.current_tier = Some(tier);
    indicator.current_player_loc = Some(*player_loc);
    *visibility = Visibility::Visible;

    let (min_dist, max_dist) = get_tier_range(tier);

    // Find all tiles within tier range
    let player_qrz = **player_loc;
    let mut tiles_in_range = Vec::new();

    // Search in a square bounding box (max_dist * 2 + 1) around player
    let search_radius = max_dist as i16;
    for dq in -search_radius..=search_radius {
        for dr in -search_radius..=search_radius {
            let tile_qrz = Qrz {
                q: player_qrz.q + dq,
                r: player_qrz.r + dr,
                z: player_qrz.z,
            };

            // Calculate flat distance (ignoring Z)
            let distance = player_loc.flat_distance(&Loc::new(tile_qrz)) as u32;

            // Check if in tier range
            if distance >= min_dist && distance <= max_dist {
                tiles_in_range.push(tile_qrz);
            }
        }
    }

    // Build a single combined mesh for all tiles
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    let mut min_bounds = Vec3::splat(f32::MAX);
    let mut max_bounds = Vec3::splat(f32::MIN);

    for tile_qrz in tiles_in_range {
        // Find the actual terrain tile at this location (handles elevation)
        if let Some((actual_tile, _)) = map.find(tile_qrz, -60) {
            // Get the vertices for this tile (respecting slope toggle)
            let (sloped_verts, _) = map.vertices_and_colors_with_slopes(actual_tile, diagnostics_state.slope_rendering_enabled);

            let base_index = positions.len() as u32;

            // Add the 6 perimeter vertices + center, slightly above terrain
            for i in 0..6 {
                let v = sloped_verts[i];
                let pos = Vec3::new(v.x, v.y + 0.06, v.z); // Raise 0.06 above terrain
                positions.push([pos.x, pos.y, pos.z]);
                normals.push([0.0, 1.0, 0.0]);
                min_bounds = min_bounds.min(pos);
                max_bounds = max_bounds.max(pos);
            }
            // Center vertex
            let center = sloped_verts[6];
            let center_pos = Vec3::new(center.x, center.y + 0.06, center.z);
            positions.push([center_pos.x, center_pos.y, center_pos.z]);
            normals.push([0.0, 1.0, 0.0]);
            min_bounds = min_bounds.min(center_pos);
            max_bounds = max_bounds.max(center_pos);

            // Create triangles from center to each edge (fan pattern)
            for i in 0..6 {
                let next = (i + 1) % 6;
                indices.extend_from_slice(&[
                    base_index + 6,
                    base_index + i,
                    base_index + next,
                ]);
            }
        }
    }

    // Create the combined mesh
    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::render::render_asset::RenderAssetUsages::default()
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

    let mesh_handle = meshes.add(mesh);

    // Create or reuse material
    let material = if let Some(mat) = maybe_material {
        mat.0.clone()
    } else {
        materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 0.0, 0.3), // Translucent yellow
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        })
    };

    // Update the entity with new mesh
    commands.entity(indicator_ent).insert((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, 0.0, 0.0), // Vertices are in world space
        Aabb::from_min_max(min_bounds, max_bounds),
        NotShadowCaster,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tier_range_close() {
        let (min, max) = get_tier_range(RangeTier::Close);
        assert_eq!(min, 1, "Tier 1 (Close) should start at 1 hex");
        assert_eq!(max, 2, "Tier 1 (Close) should end at 2 hexes");
    }

    #[test]
    fn test_get_tier_range_mid() {
        let (min, max) = get_tier_range(RangeTier::Mid);
        assert_eq!(min, 3, "Tier 2 (Mid) should start at 3 hexes");
        assert_eq!(max, 6, "Tier 2 (Mid) should end at 6 hexes");
    }

    #[test]
    fn test_get_tier_range_far() {
        let (min, max) = get_tier_range(RangeTier::Far);
        assert_eq!(min, 7, "Tier 3 (Far) should start at 7 hexes");
        assert_eq!(max, 10, "Tier 3 (Far) should end at 10 hexes");
    }

    #[test]
    fn test_tier_ranges_are_contiguous() {
        let (_, close_max) = get_tier_range(RangeTier::Close);
        let (mid_min, mid_max) = get_tier_range(RangeTier::Mid);
        let (far_min, _) = get_tier_range(RangeTier::Far);

        assert_eq!(close_max + 1, mid_min, "Tier 1 and Tier 2 should be contiguous");
        assert_eq!(mid_max + 1, far_min, "Tier 2 and Tier 3 should be contiguous");
    }

    #[test]
    fn test_tier_ranges_do_not_overlap() {
        let (close_min, close_max) = get_tier_range(RangeTier::Close);
        let (mid_min, mid_max) = get_tier_range(RangeTier::Mid);
        let (far_min, far_max) = get_tier_range(RangeTier::Far);

        // Close and Mid should not overlap
        assert!(close_max < mid_min, "Tier 1 should not overlap with Tier 2");

        // Mid and Far should not overlap
        assert!(mid_max < far_min, "Tier 2 should not overlap with Tier 3");
    }

    #[test]
    fn test_tier_1_starts_at_melee_range() {
        let (min, _) = get_tier_range(RangeTier::Close);
        assert_eq!(min, 1, "Tier 1 should start at distance 1 (melee range)");
    }

    #[test]
    fn test_tier_ranges_cover_expected_distances() {
        // Test specific distances fall into correct tiers (matching targeting.rs)
        let distance_1_tier = get_tier_for_distance(1);
        assert_eq!(distance_1_tier, RangeTier::Close, "Distance 1 should be Tier 1 (Close)");

        let distance_2_tier = get_tier_for_distance(2);
        assert_eq!(distance_2_tier, RangeTier::Close, "Distance 2 should be Tier 1 (Close)");

        let distance_3_tier = get_tier_for_distance(3);
        assert_eq!(distance_3_tier, RangeTier::Mid, "Distance 3 should be Tier 2 (Mid)");

        let distance_6_tier = get_tier_for_distance(6);
        assert_eq!(distance_6_tier, RangeTier::Mid, "Distance 6 should be Tier 2 (Mid)");

        let distance_7_tier = get_tier_for_distance(7);
        assert_eq!(distance_7_tier, RangeTier::Far, "Distance 7 should be Tier 3 (Far)");

        let distance_10_tier = get_tier_for_distance(10);
        assert_eq!(distance_10_tier, RangeTier::Far, "Distance 10 should be Tier 3 (Far)");
    }

    // Helper function for testing (matches targeting.rs::get_range_tier logic)
    fn get_tier_for_distance(distance: u32) -> RangeTier {
        if distance <= 2 {
            RangeTier::Close
        } else if distance <= 6 {
            RangeTier::Mid
        } else {
            RangeTier::Far
        }
    }
}
