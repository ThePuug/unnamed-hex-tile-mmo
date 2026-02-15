use bevy::prelude::*;

use crate::{
    client::{
        plugins::diagnostics::{DiagnosticsState, grid::HexGridOverlay, perf_ui::PerfUiRootMarker, network_ui::NetworkUiRootMarker},
        components::PlayerOriginDebug,
    },
    common::components::behaviour::Behaviour,
};

/// Events that can be triggered from the developer console
#[derive(Event, Message, Debug)]
pub enum DevConsoleAction {
    // Terrain actions
    ToggleGrid,
    ToggleSlopeRendering,
    ToggleFixedLighting,
    RegenerateMesh,

    // Performance actions
    TogglePerfUI,
    ToggleNetworkUI,
}

/// System that executes console actions
pub fn execute_console_actions(
    mut commands: Commands,
    mut diagnostics_state: ResMut<DiagnosticsState>,
    mut reader: MessageReader<DevConsoleAction>,
    map_state: Res<crate::common::resources::map::MapState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay), (Without<PerfUiRootMarker>, Without<NetworkUiRootMarker>)>,
    mut pending_meshes: ResMut<crate::client::resources::PendingChunkMeshes>,
    chunk_mesh_query: Query<(Entity, &Mesh3d, &crate::client::components::ChunkMesh)>,
    actor_query: Query<Entity, With<Behaviour>>,
    debug_sphere_query: Query<Entity, With<PlayerOriginDebug>>,
    mut perf_ui_query: Query<&mut Node, (With<PerfUiRootMarker>, Without<HexGridOverlay>, Without<NetworkUiRootMarker>)>,
    mut network_ui_query: Query<&mut Node, (With<NetworkUiRootMarker>, Without<PerfUiRootMarker>, Without<HexGridOverlay>)>,
) {
    for action in reader.read() {
        match action {
            // Terrain actions
            DevConsoleAction::ToggleGrid => {
                diagnostics_state.grid_visible = !diagnostics_state.grid_visible;

                // Update grid overlay visibility and regeneration flag (same as toggle_grid_visibility)
                if let Ok((mut visibility, mut overlay)) = grid_query.single_mut() {
                    *visibility = if diagnostics_state.grid_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };

                    // Request mesh regeneration when toggling on
                    if diagnostics_state.grid_visible {
                        overlay.needs_regeneration = true;
                    }
                }

                // Spawn or despawn debug spheres for all actors
                if diagnostics_state.grid_visible {
                    for actor_entity in actor_query.iter() {
                        crate::client::systems::actor::spawn_debug_sphere(
                            &mut commands, &mut meshes, &mut materials, actor_entity,
                        );
                    }
                } else {
                    for entity in debug_sphere_query.iter() {
                        commands.entity(entity).despawn();
                    }
                }

                info!("Grid overlay: {}", if diagnostics_state.grid_visible { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleSlopeRendering => {
                diagnostics_state.slope_rendering_enabled = !diagnostics_state.slope_rendering_enabled;

                // Clear all pending tasks and spawn new async tasks for all chunks
                pending_meshes.tasks.clear();

                let pool = bevy::tasks::AsyncComputeTaskPool::get();
                for (_entity, _mesh_3d, chunk_mesh) in chunk_mesh_query.iter() {
                    let map_arc = map_state.map.clone();
                    let apply_slopes = diagnostics_state.slope_rendering_enabled;
                    let chunk_id = chunk_mesh.chunk_id;

                    let task = pool.spawn(async move {
                        // Acquire read lock (blocks if drain task has write lock)
                        let map_lock = map_arc.read().unwrap();

                        // Generate mesh using temporary Map wrapper
                        let map_wrapper = crate::common::resources::map::Map::from_inner(map_lock.clone());
                        map_wrapper.generate_chunk_mesh(chunk_id, apply_slopes)
                    });

                    pending_meshes.tasks.insert(chunk_id, task);
                }

                info!("Slope rendering: {} (regenerating {} chunks async)",
                    if diagnostics_state.slope_rendering_enabled { "ON" } else { "OFF" },
                    pending_meshes.tasks.len());
            }
            DevConsoleAction::ToggleFixedLighting => {
                diagnostics_state.fixed_lighting_enabled = !diagnostics_state.fixed_lighting_enabled;
                info!("Fixed lighting: {}", if diagnostics_state.fixed_lighting_enabled { "ON" } else { "OFF" });
            }
            DevConsoleAction::RegenerateMesh => {
                // Clear all pending tasks and spawn new async tasks for all chunks
                pending_meshes.tasks.clear();

                let pool = bevy::tasks::AsyncComputeTaskPool::get();
                for (_entity, _mesh_3d, chunk_mesh) in chunk_mesh_query.iter() {
                    let map_arc = map_state.map.clone();
                    let apply_slopes = diagnostics_state.slope_rendering_enabled;
                    let chunk_id = chunk_mesh.chunk_id;

                    let task = pool.spawn(async move {
                        // Acquire read lock (blocks if drain task has write lock)
                        let map_lock = map_arc.read().unwrap();

                        // Generate mesh using temporary Map wrapper
                        let map_wrapper = crate::common::resources::map::Map::from_inner(map_lock.clone());
                        map_wrapper.generate_chunk_mesh(chunk_id, apply_slopes)
                    });

                    pending_meshes.tasks.insert(chunk_id, task);
                }

                info!("Mesh regeneration requested ({} chunks async)", pending_meshes.tasks.len());
            }

            // Performance actions
            DevConsoleAction::TogglePerfUI => {
                diagnostics_state.perf_ui_visible = !diagnostics_state.perf_ui_visible;

                if let Ok(mut node) = perf_ui_query.single_mut() {
                    node.display = if diagnostics_state.perf_ui_visible {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }

                info!("Performance UI: {}", if diagnostics_state.perf_ui_visible { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleNetworkUI => {
                diagnostics_state.network_ui_visible = !diagnostics_state.network_ui_visible;

                if let Ok(mut node) = network_ui_query.single_mut() {
                    node.display = if diagnostics_state.network_ui_visible {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }

                info!("Network UI: {}", if diagnostics_state.network_ui_visible { "ON" } else { "OFF" });
            }
        }
    }
}
