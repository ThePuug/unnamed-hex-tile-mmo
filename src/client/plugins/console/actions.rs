use bevy::prelude::*;

use crate::{
    client::{
        plugins::diagnostics::{DiagnosticsState, grid::HexGridOverlay, perf_ui::PerfUiRootMarker, network_ui::NetworkUiRootMarker},
        components::Terrain,
    },
    common::resources::map::Map,
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
    mut map: ResMut<Map>,
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay), (Without<PerfUiRootMarker>, Without<NetworkUiRootMarker>)>,
    mut pending_meshes: ResMut<crate::client::resources::PendingChunkMeshes>,
    chunk_mesh_query: Query<(Entity, &Mesh3d, &crate::client::components::ChunkMesh)>,
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

                info!("Grid overlay: {}", if diagnostics_state.grid_visible { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleSlopeRendering => {
                diagnostics_state.slope_rendering_enabled = !diagnostics_state.slope_rendering_enabled;

                // Clear all pending tasks and spawn new async tasks for all chunks
                pending_meshes.tasks.clear();

                let pool = bevy::tasks::AsyncComputeTaskPool::get();
                for (_entity, _mesh_3d, chunk_mesh) in chunk_mesh_query.iter() {
                    let map_clone = map.clone();
                    let apply_slopes = diagnostics_state.slope_rendering_enabled;
                    let chunk_id = chunk_mesh.chunk_id;

                    let task = pool.spawn(async move {
                        map_clone.generate_chunk_mesh(chunk_id, apply_slopes)
                    });

                    pending_meshes.tasks.insert(chunk_id, task);
                }

                // Trigger map change detection to force grid regeneration
                map.set_changed();

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
                    let map_clone = map.clone();
                    let apply_slopes = diagnostics_state.slope_rendering_enabled;
                    let chunk_id = chunk_mesh.chunk_id;

                    let task = pool.spawn(async move {
                        map_clone.generate_chunk_mesh(chunk_id, apply_slopes)
                    });

                    pending_meshes.tasks.insert(chunk_id, task);
                }

                map.set_changed();
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
