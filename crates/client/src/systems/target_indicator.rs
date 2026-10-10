//! The rings under the viewed actor's targets: red under the hostile its
//! `Target` names, green under the ally its `AllyTarget` names, each moved
//! onto its tile every frame and hidden while there is none. Unbuilt: a
//! tier badge on the ring.

use bevy::prelude::*;
use bevy_camera::primitives::Aabb;
use bevy_light::NotShadowCaster;

use crate::components::TargetIndicator;
use common_bevy::{
    components::{entity_type::*, *},
    resources::map::Map,
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
        Transform::default(),
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
        Transform::default(),
        Visibility::Hidden,
        Aabb::default(),
        NotShadowCaster,
        TargetIndicator {
            indicator_type: IndicatorType::Ally,
        },
    ));
}

/// Moves each ring onto its target's tile, or hides it, every frame.
pub fn update(
    mut indicator_query: Query<(&mut Mesh3d, &mut Transform, &mut Visibility, &mut Aabb, &TargetIndicator)>,
    local_player_query: Query<(&common_bevy::components::target::Target, Option<&common_bevy::components::ally_target::AllyTarget>, &common_bevy::components::resources::Health), With<crate::components::Viewed>>,
    entity_query: Query<(&EntityType, &Loc)>,
    map: Res<Map>,
    mut meshes: ResMut<Assets<Mesh>>,
    origin: Res<crate::resources::RenderOrigin>,
) {
    // Get local player's targets and health
    let Ok((player_target, player_ally_target, health)) = local_player_query.single() else {
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

    // The current target, never the sticky `last_target`: a ring shows
    // what a press would strike now
    let hostile_target = player_target.entity;
    let ally_target = player_ally_target.and_then(|ally| ally.entity);

    for (mut mesh_handle, mut transform, mut visibility, mut aabb, indicator) in &mut indicator_query {
        let wanted = match indicator.indicator_type {
            IndicatorType::Hostile => hostile_target,
            IndicatorType::Ally => ally_target,
        };
        // No target, a target without a place, or a place with no terrain
        // under it: the ring hides
        let tile = wanted
            .and_then(|ent| entity_query.get(ent).ok())
            .and_then(|(_, loc)| map.get_by_qr(loc.q, loc.r))
            .map(|(tile, _)| tile);
        let Some(tile) = tile else {
            *visibility = Visibility::Hidden;
            continue;
        };

        // A filled hex on the sloped terrain, raised 0.05 above it: the 6
        // perimeter vertices, the centre, and a fan of triangles from it
        let sloped_verts = map.vertices_with_slopes(tile);
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        let positions: Vec<[f32; 3]> = sloped_verts[..7]
            .iter()
            .map(|v| {
                let pos = Vec3::new(v.x, v.y + 0.05, v.z);
                min = min.min(pos);
                max = max.max(pos);
                [pos.x, pos.y, pos.z]
            })
            .collect();
        let normals = vec![[0.0, 1.0, 0.0]; 7];
        let indices: Vec<u32> = (0..6u32).flat_map(|i| [6, i, (i + 1) % 6]).collect();

        let mut new_mesh = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy_asset::RenderAssetUsages::default()
        );
        new_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        new_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        new_mesh.insert_indices(bevy_mesh::Indices::U32(indices));
        mesh_handle.0 = meshes.add(new_mesh);

        // Bounds so it is never culled
        *aabb = Aabb::from_min_max(min, max);

        // The vertices are world coordinates: the transform takes the
        // render origin off them.
        transform.translation = -origin.world_vec();
        transform.rotation = Quat::IDENTITY;
        *visibility = Visibility::Visible;
    }
}

/// Indicator types for different targeting modes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorType {
    /// Red indicator for hostile targets
    Hostile,
    /// Green indicator for ally targets
    Ally,
}
