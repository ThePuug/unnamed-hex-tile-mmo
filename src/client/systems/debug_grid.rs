use bevy::prelude::*;

use crate::{
    client::components::Terrain,
    common::resources::map::Map,
};

#[derive(Resource, Default)]
pub struct GridVisible(pub bool);

/// Toggle grid visibility with 'G' key
pub fn toggle_grid(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut grid_visible: ResMut<GridVisible>,
) {
    if keyboard.just_pressed(KeyCode::KeyG) {
        grid_visible.0 = !grid_visible.0;
        info!("Grid {}", if grid_visible.0 { "enabled" } else { "disabled" });
    }
}

/// Draw grid lines on terrain triangles for debugging
pub fn draw_grid(
    mut gizmos: Gizmos,
    query: Query<&Terrain>,
    map: Res<Map>,
    grid_visible: Res<GridVisible>,
) {
    if !grid_visible.0 || query.is_empty() { return; }

    // Draw hex tile edges for all tiles in the map
    for (qrz, _) in map.iter_tiles() {
        let verts = map.vertices_with_slopes(qrz);
        
        // Draw the 6 edges of each hex tile
        for i in 0..6 {
            let v1 = verts[i];
            let v2 = verts[(i + 1) % 6];
            gizmos.line(v1, v2, Color::srgb(0.3, 0.3, 0.3));
        }
        
        // Draw lines from center to each vertex to show triangle structure
        let center = verts[6];
        for i in 0..6 {
            gizmos.line(center, verts[i], Color::srgb(0.2, 0.2, 0.2));
        }
    }
}
