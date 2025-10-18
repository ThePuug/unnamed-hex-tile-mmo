use bevy::{
    prelude::*,
    pbr::NotShadowCaster,
    render::primitives::Aabb,
};

use crate::{
    client::components::TargetCursor,
    common::{
        components::{
            heading::Heading,
            Loc, Actor,
        },
        resources::map::Map,
    }
};

use super::world::TILE_SIZE;

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create a simple hex mesh that will be updated to match terrain slopes
    let cursor_mesh = meshes.add(Extrusion::new(RegularPolygon::new(TILE_SIZE * 0.95, 6), 0.05));
    let cursor_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.0, 0.0, 0.3),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,  // Disable backface culling so cursor is visible from all angles
        ..default()
    });

    commands.spawn((
        Mesh3d(cursor_mesh),
        MeshMaterial3d(cursor_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Aabb::default(),
        NotShadowCaster,
        TargetCursor,
    ));
}

pub fn update(
    mut cursor_query: Query<(&mut Mesh3d, &mut Transform, &mut Aabb), With<TargetCursor>>,
    player_query: Query<(&Loc, &Heading), (With<Actor>, Or<(Changed<Loc>, Changed<Heading>)>)>,
    map: Res<Map>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if let Ok((mut mesh_handle, mut cursor_transform, mut aabb)) = cursor_query.single_mut() {
        if let Ok((loc, heading)) = player_query.single() {
            // Calculate the hex direction from the player's heading
            let target_direction = **loc + **heading;
            
            // Find the actual terrain tile in that direction, searching vertically
            if let Some((actual_tile, _)) = map.find(target_direction, -60) {
                // Get the sloped vertices for this tile
                let sloped_verts = map.vertices_with_slopes(actual_tile);
                
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
                    let pos = Vec3::new(v.x, v.y + 0.01, v.z);
                    positions.push([pos.x, pos.y, pos.z]);
                    min = min.min(pos);
                    max = max.max(pos);
                }
                // Center vertex
                let center = sloped_verts[6];
                let center_pos = Vec3::new(center.x, center.y + 0.01, center.z);
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
                let mut new_mesh = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, 
                                            bevy::render::render_asset::RenderAssetUsages::default());
                new_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                new_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                new_mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));
                
                // Replace the mesh
                mesh_handle.0 = meshes.add(new_mesh);
                
                // Update AABB to prevent culling when far from origin
                *aabb = Aabb::from_min_max(min, max);
                
                // Position at origin since vertices are in world space
                cursor_transform.translation = Vec3::ZERO;
                cursor_transform.rotation = Quat::IDENTITY;
            }
        }
    }
}
