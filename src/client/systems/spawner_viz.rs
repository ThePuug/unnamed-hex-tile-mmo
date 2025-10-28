use bevy::{
    prelude::*,
    pbr::NotShadowCaster,
};

use crate::common::{
    components::{Loc, heading::Heading, Actor},
    message::{Try, Event},
    systems::gcd::GcdType,
    resources::map::Map,
};
use qrz::Convert;

use super::world::TILE_SIZE;

/// Marker component for spawner visualization entities
#[derive(Component)]
pub struct SpawnerVisualization {
    pub location: Loc,
}

/// Resource to track if spawner visualizations should be shown
#[derive(Resource)]
pub struct SpawnerVizState {
    pub enabled: bool,
}

impl Default for SpawnerVizState {
    fn default() -> Self {
        Self {
            enabled: true, // Show by default for debugging
        }
    }
}

/// System that creates visual markers when spawners are placed
pub fn visualize_spawners(
    mut commands: Commands,
    state: Res<SpawnerVizState>,
    mut try_reader: EventReader<Try>,
    player_query: Query<(&Loc, &Heading), With<Actor>>,
    existing_viz_query: Query<(Entity, &SpawnerVisualization)>,
    map: Res<Map>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.enabled {
        return;
    }

    // Listen for spawner placement events
    for message in try_reader.read().cloned() {
        let Try { event: Event::Gcd { typ: GcdType::PlaceSpawner(_), .. } } = message else { continue };

        // Get player location and heading to calculate spawner position
        let Ok((&player_loc, &player_heading)) = player_query.single() else { continue };
        let spawner_qrz = *player_loc + *player_heading;
        let spawner_loc = Loc::new(spawner_qrz);

        info!("Placing spawner: player_loc={:?}, heading={:?}, spawner_loc={:?}",
              *player_loc, *player_heading, spawner_qrz);

        // Check if we already have a marker at this location
        let already_exists = existing_viz_query.iter().any(|(_, viz)| viz.location == spawner_loc);
        if already_exists {
            continue;
        }

        // Create a vertical cylinder marker
        let marker_mesh = meshes.add(Cylinder::new(TILE_SIZE * 0.3, 2.0));
        let marker_material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.0, 1.0, 1.0, 0.5), // Cyan, semi-transparent
            alpha_mode: AlphaMode::Blend,
            emissive: LinearRgba::new(0.0, 0.5, 0.5, 1.0),
            unlit: false,
            ..default()
        });

        // Calculate world position using Map's convert method
        let mut world_pos = map.convert(spawner_qrz);
        world_pos.y += 1.0; // Raise cylinder 1 unit above terrain

        commands.spawn((
            Mesh3d(marker_mesh),
            MeshMaterial3d(marker_material),
            Transform::from_translation(world_pos),
            NotShadowCaster,
            SpawnerVisualization { location: spawner_loc },
            Name::new("Spawner Visualization"),
        ));

        info!("Created spawner visualization at {:?}", spawner_loc);
    }
}

/// System to toggle spawner visualization on/off
pub fn toggle_spawner_viz(
    mut commands: Commands,
    mut state: ResMut<SpawnerVizState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    existing_viz_query: Query<Entity, With<SpawnerVisualization>>,
) {
    // Toggle with 'V' key
    if keyboard.just_pressed(KeyCode::KeyV) {
        state.enabled = !state.enabled;
        info!("Spawner visualization: {}", if state.enabled { "ON" } else { "OFF" });

        // If disabled, despawn all markers
        if !state.enabled {
            for viz_ent in &existing_viz_query {
                commands.entity(viz_ent).despawn();
            }
        }
    }
}

/// Cleanup system (placeholder for future use)
pub fn cleanup_despawned_spawner_viz() {
    // Placeholder for future cleanup logic if spawners become networkable
}
