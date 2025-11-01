//! Target Indicator System
//!
//! Shows visual indicators on entities that will be targeted by abilities.
//! This is THE MOST CRITICAL system for player feedback (per ADR-004-player-feedback.md).
//!
//! # Design Requirements (from player feedback)
//!
//! - Updates EVERY FRAME (zero lag, instant feedback)
//! - Position matches target exactly
//! - No flickering or ghost indicators
//! - Clear visual distinction (red = hostile, green = ally)
//!
//! # How it works
//!
//! 1. Query local player's location, heading
//! 2. Call `select_target()` to get current hostile target
//! 3. Update indicator position to match target's location
//! 4. Show/hide indicator based on target availability

use bevy::{
    prelude::*,
    pbr::NotShadowCaster,
    render::primitives::Aabb,
};

use crate::{
    client::{components::TargetIndicator, plugins::diagnostics::DiagnosticsState},
    common::{
        components::{behaviour::Behaviour, entity_type::*, heading::*, *},
        plugins::nntree::*,
        resources::map::Map,
        systems::targeting::{select_target, select_ally_target},
    },
};

use super::world::TILE_SIZE;

/// Setup the target indicator visual
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create a hex ring mesh slightly larger than tiles
    let indicator_mesh = meshes.add(Extrusion::new(RegularPolygon::new(TILE_SIZE * 1.1, 6), 0.08));

    // Red material for hostile targets
    let hostile_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.0, 0.0, 0.7),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    // Spawn the hostile indicator (hidden by default)
    commands.spawn((
        Mesh3d(indicator_mesh.clone()),
        MeshMaterial3d(hostile_material),
        Transform::from_xyz(0.0, -1000.0, 0.0), // Start hidden below world
        Visibility::Hidden,
        Aabb::default(),
        NotShadowCaster,
        TargetIndicator {
            indicator_type: IndicatorType::Hostile,
        },
    ));

    // Green material for ally targets
    let ally_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 1.0, 0.0, 0.7),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    // Spawn the ally indicator (hidden by default)
    commands.spawn((
        Mesh3d(indicator_mesh),
        MeshMaterial3d(ally_material),
        Transform::from_xyz(0.0, -1000.0, 0.0), // Start hidden below world
        Visibility::Hidden,
        Aabb::default(),
        NotShadowCaster,
        TargetIndicator {
            indicator_type: IndicatorType::Ally,
        },
    ));
}

/// Update target indicator position every frame
///
/// This runs in Update schedule for instant feedback (60fps)
pub fn update(
    mut indicator_query: Query<(&mut Mesh3d, &mut Transform, &mut Visibility, &mut Aabb, &TargetIndicator)>,
    local_player_query: Query<(Entity, &Loc, &Heading, &crate::common::components::resources::Health), With<Actor>>,
    entity_query: Query<(&EntityType, &Loc, Option<&crate::common::components::behaviour::PlayerControlled>)>,
    nntree: Res<NNTree>,
    map: Res<Map>,
    diagnostics_state: Res<DiagnosticsState>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Get local player
    let Ok((player_ent, player_loc, player_heading, health)) = local_player_query.get_single() else {
        return;
    };

    // Don't show target indicator while dead (health <= 0)
    if health.state <= 0.0 {
        // Hide all indicators
        for (_, _, mut visibility, _, _) in &mut indicator_query {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Select the current hostile target using Phase 2 targeting system
    // Filter out allies (PlayerControlled) from hostile targeting
    let hostile_target = select_target(
        player_ent,
        *player_loc,
        *player_heading,
        None, // No tier lock in MVP
        &nntree,
        |ent| {
            entity_query.get(ent).ok().and_then(|(et, _, player_controlled_opt)| {
                // Exclude allies (PlayerControlled) from hostile targeting
                if player_controlled_opt.is_some() {
                    return None;
                }
                Some(*et)
            })
        },
    );

    // Select the current ally target using directional targeting
    let ally_target = select_ally_target(
        player_ent,
        *player_loc,
        *player_heading,
        &nntree,
        |ent| entity_query.get(ent).ok().map(|(_, _, player_controlled_opt)| player_controlled_opt.is_some()).unwrap_or(false),
    );

    // Update both hostile and ally indicators
    for (mut mesh_handle, mut transform, mut visibility, mut aabb, indicator) in &mut indicator_query {
        if matches!(indicator.indicator_type, IndicatorType::Hostile) {
            if let Some(target_ent) = hostile_target {
                // Get target's location
                if let Ok((_, target_loc, _)) = entity_query.get(target_ent) {
                    // Find the actual terrain tile at target location (handles elevation)
                    if let Some((actual_tile, _)) = map.find(**target_loc, -60) {
                        // Get the vertices for this tile (respecting slope toggle)
                        let (sloped_verts, _) = map.vertices_and_colors_with_slopes(actual_tile, diagnostics_state.slope_rendering_enabled);

                        // Create a filled hex mesh matching the sloped terrain
                        let mut positions = Vec::new();
                        let mut normals = Vec::new();
                        let mut indices = Vec::new();

                        // Track min/max for AABB
                        let mut min = Vec3::splat(f32::MAX);
                        let mut max = Vec3::splat(f32::MIN);

                        // Add the 6 perimeter vertices + center, slightly above terrain
                        for i in 0..6 {
                            let v = sloped_verts[i];
                            let pos = Vec3::new(v.x, v.y + 0.05, v.z); // Raise 0.05 above terrain
                            positions.push([pos.x, pos.y, pos.z]);
                            min = min.min(pos);
                            max = max.max(pos);
                        }
                        // Center vertex
                        let center = sloped_verts[6];
                        let center_pos = Vec3::new(center.x, center.y + 0.05, center.z);
                        positions.push([center_pos.x, center_pos.y, center_pos.z]);
                        min = min.min(center_pos);
                        max = max.max(center_pos);

                        // Add normals (all pointing up)
                        for _ in 0..7 {
                            normals.push([0.0, 1.0, 0.0]);
                        }

                        // Create triangles from center to each edge (fan pattern)
                        for i in 0..6 {
                            let next = (i + 1) % 6;
                            indices.extend_from_slice(&[
                                6, i as u32, next as u32,  // Center, current vertex, next vertex
                            ]);
                        }

                        // Create new mesh
                        let mut new_mesh = Mesh::new(
                            bevy::render::render_resource::PrimitiveTopology::TriangleList,
                            bevy::render::render_asset::RenderAssetUsages::default()
                        );
                        new_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                        new_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                        new_mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

                        // Replace the mesh
                        mesh_handle.0 = meshes.add(new_mesh);

                        // Update AABB to prevent culling
                        *aabb = Aabb::from_min_max(min, max);

                        // Position at origin since vertices are in world space
                        transform.translation = Vec3::ZERO;
                        transform.rotation = Quat::IDENTITY;

                        *visibility = Visibility::Visible;
                    } else {
                        // Can't find terrain at target location, hide indicator
                        *visibility = Visibility::Hidden;
                    }
                } else {
                    // Target entity doesn't have location, hide indicator
                    *visibility = Visibility::Hidden;
                }
            } else {
                // No target, hide indicator
                *visibility = Visibility::Hidden;
            }
        } else if matches!(indicator.indicator_type, IndicatorType::Ally) {
            if let Some(ally_ent) = ally_target {
                // Get ally's location
                if let Ok((_, ally_loc, _)) = entity_query.get(ally_ent) {
                    // Find the actual terrain tile at ally location (handles elevation)
                    if let Some((actual_tile, _)) = map.find(**ally_loc, -60) {
                        // Get the vertices for this tile (respecting slope toggle)
                        let (sloped_verts, _) = map.vertices_and_colors_with_slopes(actual_tile, diagnostics_state.slope_rendering_enabled);

                        // Create a filled hex mesh matching the sloped terrain
                        let mut positions = Vec::new();
                        let mut normals = Vec::new();
                        let mut indices = Vec::new();

                        // Track min/max for AABB
                        let mut min = Vec3::splat(f32::MAX);
                        let mut max = Vec3::splat(f32::MIN);

                        // Add the 6 perimeter vertices + center, slightly above terrain
                        for i in 0..6 {
                            let v = sloped_verts[i];
                            let pos = Vec3::new(v.x, v.y + 0.05, v.z); // Raise 0.05 above terrain
                            positions.push([pos.x, pos.y, pos.z]);
                            min = min.min(pos);
                            max = max.max(pos);
                        }
                        // Center vertex
                        let center = sloped_verts[6];
                        let center_pos = Vec3::new(center.x, center.y + 0.05, center.z);
                        positions.push([center_pos.x, center_pos.y, center_pos.z]);
                        min = min.min(center_pos);
                        max = max.max(center_pos);

                        // Add normals (all pointing up)
                        for _ in 0..7 {
                            normals.push([0.0, 1.0, 0.0]);
                        }

                        // Create triangles from center to each edge (fan pattern)
                        for i in 0..6 {
                            let next = (i + 1) % 6;
                            indices.extend_from_slice(&[
                                6, i as u32, next as u32,  // Center, current vertex, next vertex
                            ]);
                        }

                        // Create new mesh
                        let mut new_mesh = Mesh::new(
                            bevy::render::render_resource::PrimitiveTopology::TriangleList,
                            bevy::render::render_asset::RenderAssetUsages::default()
                        );
                        new_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                        new_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                        new_mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

                        // Replace the mesh
                        mesh_handle.0 = meshes.add(new_mesh);

                        // Update AABB to prevent culling
                        *aabb = Aabb::from_min_max(min, max);

                        // Position at origin since vertices are in world space
                        transform.translation = Vec3::ZERO;
                        transform.rotation = Quat::IDENTITY;

                        *visibility = Visibility::Visible;
                    } else {
                        // Can't find terrain at ally location, hide indicator
                        *visibility = Visibility::Hidden;
                    }
                } else {
                    // Ally entity doesn't have location, hide indicator
                    *visibility = Visibility::Hidden;
                }
            } else {
                // No ally target, hide indicator
                *visibility = Visibility::Hidden;
            }
        }
    }
}

/// Indicator types for different targeting modes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorType {
    /// Red indicator for hostile targets
    Hostile,
    /// Green indicator for ally targets (future)
    #[allow(dead_code)]
    Ally,
}
