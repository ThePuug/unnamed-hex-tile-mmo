use std::f32::consts::PI;

use bevy::{
    prelude::*,
    pbr::NotShadowCaster,
};
use qrz::Convert;

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

use super::world::{TILE_RISE, TILE_SIZE};

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create a translucent red hex mesh for the target cursor
    let cursor_mesh = meshes.add(Extrusion::new(RegularPolygon::new(TILE_SIZE * 0.95, 6), TILE_RISE * 0.1));
    let cursor_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.0, 0.0, 0.3),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.spawn((
        Mesh3d(cursor_mesh),
        MeshMaterial3d(cursor_material),
        Transform::from_xyz(0.0, 0.01, 0.0)
            .with_rotation(Quat::from_rotation_x(PI / 2.0)), // Rotate to lie flat on ground
        NotShadowCaster,
        TargetCursor,
    ));
}

pub fn update(
    mut cursor_query: Query<&mut Transform, With<TargetCursor>>,
    player_query: Query<(&Loc, &Heading), With<Actor>>,
    map: Res<Map>,
) {
    if let Ok(mut cursor_transform) = cursor_query.single_mut() {
        if let Ok((loc, heading)) = player_query.single() {
            // Calculate the hex direction from the player's heading
            let target_direction = **loc + **heading;
            
            // Find the actual terrain tile in that direction, searching vertically
            if let Some((actual_tile, _)) = map.find(target_direction, -5) {
                let target_pos = map.convert(actual_tile);
                
                // Position at the top of the tile surface (base + TILE_RISE) plus small offset
                cursor_transform.translation = Vec3::new(
                    target_pos.x,
                    target_pos.y + TILE_RISE + 0.01,
                    target_pos.z
                );
                // Maintain flat orientation
                cursor_transform.rotation = Quat::from_rotation_x(PI / 2.0);
            }
        }
    }
}
