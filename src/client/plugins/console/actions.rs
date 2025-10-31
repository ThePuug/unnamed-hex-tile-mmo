use bevy::prelude::*;

use crate::{
    client::{
        plugins::diagnostics::{DiagnosticsState, grid::HexGridOverlay, perf_ui::PerfUiRootMarker},
        components::Terrain,
    },
    common::{
        components::{
            resources::*,
            behaviour::Behaviour,
            reaction_queue::{ReactionQueue, QueuedThreat, DamageType},
            ActorAttributes,
        },
        resources::map::Map,
        systems::combat::queue::calculate_timer_duration,
    },
};

/// Events that can be triggered from the developer console
#[derive(Event, Debug)]
pub enum DevConsoleAction {
    // Terrain actions
    ToggleGrid,
    ToggleSlopeRendering,
    ToggleFixedLighting,
    RegenerateMesh,

    // Combat actions
    QueueDamageThreat,
    DrainStamina,
    DrainMana,
    ClearReactionQueue,
    RefillResources,

    // Performance actions
    TogglePerfUI,
    ToggleFPSCounter,
    ToggleDetailedStats,
    LogFrameReport,

    // Tools actions (future)
    TeleportToCursor,
    SpawnNPCAtCursor,
    ClearAllEntities,
    PlaceTestSpawner,
}

/// System that executes console actions
pub fn execute_console_actions(
    mut commands: Commands,
    mut reader: EventReader<DevConsoleAction>,
    mut diagnostics_state: ResMut<DiagnosticsState>,
    mut map: ResMut<Map>,
    time: Res<Time>,
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay), Without<PerfUiRootMarker>>,
    mut terrain_query: Query<&mut Terrain>,
    mut perf_ui_query: Query<&mut Visibility, (With<PerfUiRootMarker>, Without<HexGridOverlay>)>,
    mut player_query: Query<
        (Entity, &mut Health, &mut Stamina, &mut Mana, &mut ReactionQueue, &ActorAttributes),
        With<Behaviour>
    >,
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

            // Combat actions
            DevConsoleAction::QueueDamageThreat => {
                if let Ok((entity, mut _health, _, _, mut queue, attrs)) = player_query.single_mut() {
                    let now = time.elapsed();
                    let timer_duration = calculate_timer_duration(attrs);

                    let threat = QueuedThreat {
                        source: entity,
                        damage: 20.0,
                        damage_type: DamageType::Physical,
                        inserted_at: now,
                        timer_duration,
                    };

                    queue.threats.push_back(threat);
                    info!("Queued 20 damage threat");
                }
            }
            DevConsoleAction::DrainStamina => {
                if let Ok((_, _, mut stamina, _, _, _)) = player_query.single_mut() {
                    stamina.step = (stamina.step - 30.0).max(0.0);
                    stamina.state = stamina.step;
                    info!("Drained 30 stamina");
                }
            }
            DevConsoleAction::DrainMana => {
                if let Ok((_, _, _, mut mana, _, _)) = player_query.single_mut() {
                    mana.step = (mana.step - 25.0).max(0.0);
                    mana.state = mana.step;
                    info!("Drained 25 mana");
                }
            }
            DevConsoleAction::ClearReactionQueue => {
                if let Ok((_, _, _, _, mut queue, _)) = player_query.single_mut() {
                    queue.threats.clear();
                    info!("Cleared reaction queue");
                }
            }
            DevConsoleAction::RefillResources => {
                if let Ok((_, mut health, mut stamina, mut mana, _, _)) = player_query.single_mut() {
                    health.step = health.max;
                    health.state = health.max;
                    stamina.step = stamina.max;
                    stamina.state = stamina.max;
                    mana.step = mana.max;
                    mana.state = mana.max;
                    info!("Refilled all resources to maximum");
                }
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
            DevConsoleAction::ToggleFPSCounter => {
                info!("FPS counter toggle (future: separate from perf UI)");
            }
            DevConsoleAction::ToggleDetailedStats => {
                info!("Detailed stats toggle (not yet implemented)");
            }
            DevConsoleAction::LogFrameReport => {
                info!("Frame report logged (not yet implemented)");
            }

            // Tools actions (future)
            DevConsoleAction::TeleportToCursor => {
                info!("Teleport to cursor (not yet implemented)");
            }
            DevConsoleAction::SpawnNPCAtCursor => {
                info!("Spawn NPC at cursor (not yet implemented)");
            }
            DevConsoleAction::ClearAllEntities => {
                info!("Clear all entities (not yet implemented)");
            }
            DevConsoleAction::PlaceTestSpawner => {
                info!("Place test spawner (not yet implemented)");
            }
        }
    }
}
