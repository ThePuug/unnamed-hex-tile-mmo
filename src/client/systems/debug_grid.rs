use bevy::{
    prelude::*,
    pbr::NotShadowCaster,
    render::{
        mesh::PrimitiveTopology,
        render_asset::RenderAssetUsages,
        primitives::Aabb,
    },
};

use crate::{
    client::components::Terrain,
    common::resources::map::Map,
};

#[derive(Resource, Default)]
pub struct GridVisible(pub bool);

#[derive(Component)]
pub struct DebugGridMesh;

/// Setup grid mesh entity
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create initial mesh with minimal dummy data (prevents divide by zero)
    let mut initial_mesh = Mesh::new(
        PrimitiveTopology::LineList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    // Add dummy line at origin (won't be visible anyway since mesh starts hidden)
    initial_mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
    );
    let mesh = meshes.add(initial_mesh);
    
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.3, 0.3),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Aabb::default(),
        NotShadowCaster,
        Visibility::Hidden,
        DebugGridMesh,
    ));
}

/// Toggle grid visibility with 'G' key
pub fn toggle_grid(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut grid_visible: ResMut<GridVisible>,
    mut query: Query<&mut Visibility, With<DebugGridMesh>>,
) {
    if keyboard.just_pressed(KeyCode::KeyG) {
        grid_visible.0 = !grid_visible.0;
        
        if let Ok(mut visibility) = query.single_mut() {
            *visibility = if grid_visible.0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        
        info!("Grid {}", if grid_visible.0 { "enabled" } else { "disabled" });
    }
}

/// Update grid mesh when terrain changes
pub fn update_grid(
    terrain_query: Query<&Terrain, Changed<Terrain>>,
    mut grid_query: Query<(&mut Mesh3d, &mut Aabb), With<DebugGridMesh>>,
    map: Res<Map>,
    grid_visible: Res<GridVisible>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Only update if terrain changed or grid is visible
    if terrain_query.is_empty() || !grid_visible.0 {
        return;
    }
    
    let Ok((mut grid_mesh_handle, mut aabb)) = grid_query.single_mut() else { return };
    
    // Build unified line mesh for all tiles
    let mut positions = Vec::new();
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    
    for (qrz, _) in map.iter_tiles() {
        let verts = map.vertices_with_slopes(qrz);
        
        // Add the 6 edges of each hex tile (each edge = 2 vertices)
        for i in 0..6 {
            let v1 = verts[i];
            let v2 = verts[(i + 1) % 6];
            positions.push([v1.x, v1.y, v1.z]);
            positions.push([v2.x, v2.y, v2.z]);
            min = min.min(v1).min(v2);
            max = max.max(v1).max(v2);
        }
        
        // Add lines from center to each vertex
        let center = verts[6];
        for i in 0..6 {
            positions.push([center.x, center.y, center.z]);
            positions.push([verts[i].x, verts[i].y, verts[i].z]);
            min = min.min(center).min(verts[i]);
            max = max.max(center).max(verts[i]);
        }
    }
    
    // Don't create empty mesh - causes divide by zero in renderer
    if positions.is_empty() {
        return;
    }
    
    // Create new mesh with all lines
    let mut new_mesh = Mesh::new(
        PrimitiveTopology::LineList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    new_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    
    grid_mesh_handle.0 = meshes.add(new_mesh);
    
    // Update AABB to prevent culling when far from origin
    *aabb = Aabb::from_min_max(min, max);
}
