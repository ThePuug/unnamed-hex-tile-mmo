use bevy::prelude::*;

use crate::{
    plugins::diagnostics::{DiagnosticsState, grid::HexGridOverlay, perf_ui::PerfUiRootMarker, network_ui::NetworkUiRootMarker, terrain_detail::TerrainDetailRootMarker},
    components::PlayerOriginDebug,
};
use common_bevy::components::behaviour::Behaviour;

/// Events that can be triggered from the developer console
#[derive(Event, Message, Debug)]
pub enum DevConsoleAction {
    // Terrain actions
    ToggleGrid,
    ToggleSlopeRendering,
    ToggleFixedLighting,
    RegenerateMesh,
    ToggleTerrainDetail,

    // Performance actions
    TogglePerfUI,
    ToggleNetworkUI,
    ToggleMetricsOverlay,

    // Admin actions
    #[cfg(feature = "admin")]
    ToggleFlyover,
    #[cfg(feature = "admin")]
    GotoWorldUnits(f64, f64),
    #[cfg(feature = "admin")]
    GotoQR(i32, i32),
}

/// System that executes console actions
pub fn execute_console_actions(
    mut commands: Commands,
    mut diagnostics_state: ResMut<DiagnosticsState>,
    mut reader: MessageReader<DevConsoleAction>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay), (Without<PerfUiRootMarker>, Without<NetworkUiRootMarker>)>,
    actor_query: Query<Entity, With<Behaviour>>,
    debug_sphere_query: Query<Entity, With<PlayerOriginDebug>>,
    mut perf_ui_query: Query<&mut Node, (With<PerfUiRootMarker>, Without<HexGridOverlay>, Without<NetworkUiRootMarker>, Without<TerrainDetailRootMarker>)>,
    mut network_ui_query: Query<&mut Node, (With<NetworkUiRootMarker>, Without<PerfUiRootMarker>, Without<HexGridOverlay>, Without<TerrainDetailRootMarker>)>,
    mut terrain_detail_query: Query<&mut Node, (With<TerrainDetailRootMarker>, Without<PerfUiRootMarker>, Without<HexGridOverlay>, Without<NetworkUiRootMarker>)>,
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
                        crate::systems::actor::spawn_debug_sphere(
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
                // Mesh regeneration now handled by LoD pipeline
                info!("Slope rendering: {}",
                    if diagnostics_state.slope_rendering_enabled { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleFixedLighting => {
                diagnostics_state.fixed_lighting_enabled = !diagnostics_state.fixed_lighting_enabled;
                info!("Fixed lighting: {}", if diagnostics_state.fixed_lighting_enabled { "ON" } else { "OFF" });
            }
            DevConsoleAction::RegenerateMesh => {
                // Mesh regeneration now handled by LoD pipeline
                info!("Mesh regeneration: use LoD pipeline (no-op)");
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

            DevConsoleAction::ToggleMetricsOverlay => {
                diagnostics_state.metrics_overlay_visible = !diagnostics_state.metrics_overlay_visible;
                info!("Metrics overlay: {}", if diagnostics_state.metrics_overlay_visible { "ON" } else { "OFF" });
            }

            DevConsoleAction::ToggleTerrainDetail => {
                diagnostics_state.terrain_detail_visible = !diagnostics_state.terrain_detail_visible;

                if let Ok(mut node) = terrain_detail_query.single_mut() {
                    node.display = if diagnostics_state.terrain_detail_visible {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }

                info!("Terrain detail: {}", if diagnostics_state.terrain_detail_visible { "ON" } else { "OFF" });
            }

            // Admin actions (handled by admin module's own system)
            #[cfg(feature = "admin")]
            DevConsoleAction::ToggleFlyover => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::GotoWorldUnits(_, _) => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::GotoQR(_, _) => {}
        }
    }
}
