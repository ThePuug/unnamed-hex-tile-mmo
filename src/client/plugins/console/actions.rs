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
    mut diagnostics_state: ResMut<DiagnosticsState>,
    mut reader: EventReader<DevConsoleAction>,
    mut map: ResMut<Map>,
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay), (Without<PerfUiRootMarker>, Without<NetworkUiRootMarker>)>,
    mut terrain_query: Query<&mut Terrain>,
    mut perf_ui_query: Query<&mut Visibility, (With<PerfUiRootMarker>, Without<HexGridOverlay>, Without<NetworkUiRootMarker>)>,
    mut network_ui_query: Query<&mut Visibility, (With<NetworkUiRootMarker>, Without<PerfUiRootMarker>, Without<HexGridOverlay>)>,
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

                // Trigger terrain mesh regeneration (same as toggle_slope_rendering)
                if let Ok(mut terrain) = terrain_query.single_mut() {
                    terrain.task_start_regenerate_mesh = true;
                }

                // Trigger map change detection to force grid regeneration
                map.set_changed();

                info!("Slope rendering: {}", if diagnostics_state.slope_rendering_enabled { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleFixedLighting => {
                diagnostics_state.fixed_lighting_enabled = !diagnostics_state.fixed_lighting_enabled;
                info!("Fixed lighting: {}", if diagnostics_state.fixed_lighting_enabled { "ON" } else { "OFF" });
            }
            DevConsoleAction::RegenerateMesh => {
                if let Ok(mut terrain) = terrain_query.single_mut() {
                    terrain.task_start_regenerate_mesh = true;
                }
                map.set_changed();
                info!("Mesh regeneration requested");
            }

            // Performance actions
            DevConsoleAction::TogglePerfUI => {
                diagnostics_state.perf_ui_visible = !diagnostics_state.perf_ui_visible;

                // Update performance UI visibility component (same as toggle_performance_ui)
                if let Ok(mut visibility) = perf_ui_query.single_mut() {
                    *visibility = if diagnostics_state.perf_ui_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                }

                info!("Performance UI: {}", if diagnostics_state.perf_ui_visible { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleNetworkUI => {
                diagnostics_state.network_ui_visible = !diagnostics_state.network_ui_visible;

                // Update network UI visibility component
                if let Ok(mut visibility) = network_ui_query.single_mut() {
                    *visibility = if diagnostics_state.network_ui_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                }

                info!("Network UI: {}", if diagnostics_state.network_ui_visible { "ON" } else { "OFF" });
            }
        }
    }
}
