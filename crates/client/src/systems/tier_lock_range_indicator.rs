//! Tier Lock Range Indicator System

//! Displays translucent yellow hexes showing targetable tiles when tier locked.

//! # Design Requirements

//! - Shows yellow translucent indicators for tiles in the locked tier range
//! - Updates immediately when tier lock changes (zero lag)
//! - Follows terrain elevation (terrain-following meshes)
//! - Clears indicators when tier lock is removed or player is unavailable
//! - Tier ranges are `RangeTier::bounds`; the far tier is drawn
//!   `FAR_RINGS` deep from where it starts

//! # Implementation

//! Uses a SINGLE entity with a combined mesh for all hex tiles to avoid entity churn.
//! Only recreates the mesh when the tier actually changes.

use bevy::prelude::*;
use bevy_camera::primitives::Aabb;
use bevy_light::NotShadowCaster;

use common_bevy::{
    components::{tier_lock::*, *},
    resources::map::Map,
    systems::targeting::RangeTier,
};

use qrz::Qrz;

/// Component marking the single tier lock range indicator entity
/// Stores the current tier and player location to avoid unnecessary mesh rebuilds
#[derive(Component)]
pub struct TierLockRangeIndicator {
    current_tier: Option<RangeTier>,
    current_player_loc: Option<Loc>,
}

/// Rings of the far tier drawn, from where it starts: it runs on to the
/// edge of the target search, and one mesh of all of it would be large.
const FAR_RINGS: u32 = 4;

/// The distances drawn for a tier, inclusive.
fn drawn_range(tier: RangeTier) -> (u32, u32) {
    let (min, max) = tier.bounds();
    match tier {
        RangeTier::Far => (min, min + FAR_RINGS - 1),
        _ => (min, max),
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

/// Only rebuilds the mesh when the tier actually changes.
pub fn update(
    mut commands: Commands,
    local_player_query: Query<(Entity, &Loc, &TierLock), With<Actor>>,
    mut indicator_query: Query<(Entity, &mut TierLockRangeIndicator, &mut Visibility, Option<&MeshMaterial3d<StandardMaterial>>)>,
    input_queues: Res<common_bevy::resources::InputQueues>,
    map: Res<Map>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Get the single indicator entity
    let Ok((indicator_ent, mut indicator, mut visibility, maybe_material)) = indicator_query.single_mut() else {
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

    let (min_dist, max_dist) = drawn_range(tier);
    let tiles_in_range: Vec<Qrz> = (min_dist..=max_dist).flat_map(|radius| player_loc.ring(radius)).collect();

    // Build a single combined mesh for all tiles
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    let mut min_bounds = Vec3::splat(f32::MAX);
    let mut max_bounds = Vec3::splat(f32::MIN);

    for tile_qrz in tiles_in_range {
        // Find the actual terrain tile at this location (handles elevation)
        if let Some((actual_tile, _)) = map.get_by_qr(tile_qrz.q, tile_qrz.r) {
            // Get the vertices for this tile (respecting slope toggle)
            let sloped_verts = map.vertices_with_slopes(actual_tile, true);

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
        bevy_asset::RenderAssetUsages::default()
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(bevy_mesh::Indices::U32(indices));

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
    fn every_tier_is_drawn_from_where_it_starts_and_the_far_one_capped() {
        for tier in [RangeTier::Close, RangeTier::Mid, RangeTier::Far] {
            let (drawn_min, drawn_max) = drawn_range(tier);
            assert_eq!(drawn_min, tier.bounds().0);
            assert!(drawn_max <= tier.bounds().1);
        }
        assert_eq!(drawn_range(RangeTier::Far).1 - drawn_range(RangeTier::Far).0 + 1, FAR_RINGS);
    }
}
