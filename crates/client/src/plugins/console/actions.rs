use bevy::prelude::*;

use crate::{
    plugins::diagnostics::{DiagnosticsState, grid::HexGridOverlay},
    components::PlayerOriginDebug,
};
use common_bevy::components::behaviour::Behaviour;

/// Events that can be triggered from the developer console
#[derive(Event, Message, Debug)]
pub enum DevConsoleAction {
    // Terrain actions
    ToggleGrid,
    ToggleFixedLighting,

    // Top-level toggles
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
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay)>,
    actor_query: Query<Entity, With<Behaviour>>,
    debug_sphere_query: Query<Entity, With<PlayerOriginDebug>>,
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::ToggleGrid => {
                diagnostics_state.grid_visible = !diagnostics_state.grid_visible;

                if let Ok((mut visibility, mut overlay)) = grid_query.single_mut() {
                    *visibility = if diagnostics_state.grid_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                    if diagnostics_state.grid_visible {
                        overlay.needs_regeneration = true;
                    }
                }

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
            DevConsoleAction::ToggleFixedLighting => {
                diagnostics_state.fixed_lighting_enabled = !diagnostics_state.fixed_lighting_enabled;
                info!("Fixed lighting: {}", if diagnostics_state.fixed_lighting_enabled { "ON" } else { "OFF" });
            }
            DevConsoleAction::ToggleMetricsOverlay => {
                diagnostics_state.metrics_overlay_visible = !diagnostics_state.metrics_overlay_visible;
                info!("Metrics overlay: {}", if diagnostics_state.metrics_overlay_visible { "ON" } else { "OFF" });
            }

            #[cfg(feature = "admin")]
            DevConsoleAction::ToggleFlyover => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::GotoWorldUnits(_, _) => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::GotoQR(_, _) => {}
        }
    }
}
