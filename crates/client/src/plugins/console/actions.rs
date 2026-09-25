use bevy::prelude::*;

use crate::{
    plugins::diagnostics::{DateField, DiagnosticsState, grid::HexGridOverlay},
    components::PlayerOriginDebug,
};
use common_bevy::components::behaviour::Behaviour;

/// Events that can be triggered from the developer console
#[derive(Event, Message, Debug)]
pub enum DevConsoleAction {
    // Terrain actions
    ToggleGrid,
    /// Hold the lighting clock at an hour of the day, in ms.
    SetLightingTime(u128),
    /// Move the held lighting clock by this many ms, either way.
    ScrubLightingClock(i128),
    /// Step a field of the held lighting clock's date this many times.
    StepLightingDate(DateField, i32),
    SyncLightingClock,
    ToggleCameraEnvelope,
    ToggleTerrainHidden,
    ToggleCoverHidden,
    ToggleCameraCloseup,

    // Top-level toggles
    ToggleMetricsOverlay,
    WriteMetricsSnapshot,

    // Admin actions
    #[cfg(feature = "admin")]
    ToggleFlyover,
    #[cfg(feature = "admin")]
    GotoWorldUnits(f64, f64),
    #[cfg(feature = "admin")]
    GotoQR(i32, i32),
    #[cfg(feature = "admin")]
    SetForcedSummaryRadius(Option<u32>),
    #[cfg(feature = "admin")]
    ReportTerrain,
    /// Place a den of this archetype ahead of the player.
    #[cfg(feature = "admin")]
    SpawnDen(common_bevy::spatial_difficulty::EnemyArchetype),
}

/// System that executes console actions
/// Everything the console switches, gathered into one parameter: a
/// system takes at most sixteen, and the actions already read most of
/// the scene in order to act on it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Switches<'w> {
    state: ResMut<'w, DiagnosticsState>,
    dump: ResMut<'w, crate::plugins::diagnostics::MetricsDump>,
}

pub fn execute_console_actions(
    mut commands: Commands,
    mut switches: Switches,
    mut reader: MessageReader<DevConsoleAction>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grid_query: Query<(&mut Visibility, &mut HexGridOverlay)>,
    actor_query: Query<Entity, With<Behaviour>>,
    debug_sphere_query: Query<Entity, With<PlayerOriginDebug>>,
    mut terrain: Query<&mut Visibility, (With<crate::resources::SummaryMesh>, Without<HexGridOverlay>)>,
    mut cover: Query<
        &mut Visibility,
        (
            Or<(
                With<crate::plugins::cover::draw::ModelStand>,
                With<crate::plugins::cover::draw::CardStand>,
            )>,
            Without<crate::resources::SummaryMesh>,
            Without<HexGridOverlay>,
        ),
    >,
    time: Res<Time>,
    server: Res<crate::resources::Server>,
) {
    let game = server.current_time(time.elapsed().as_millis());
    let diagnostics_state = &mut *switches.state;
    let metrics_dump = &mut *switches.dump;
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
            DevConsoleAction::SetLightingTime(ms_of_day) => {
                diagnostics_state.lighting.hold(game, *ms_of_day);
                info!("Lighting clock: held at {}", diagnostics_state.lighting.held_at().unwrap_or_default());
            }
            DevConsoleAction::ScrubLightingClock(delta) => {
                diagnostics_state.lighting.scrub(game, *delta);
            }
            DevConsoleAction::StepLightingDate(field, steps) => {
                diagnostics_state.lighting.step(game, *field, *steps);
                let at = diagnostics_state.lighting.at(game);
                info!("Lighting clock: held on {}", common_bevy::systems::Date::of(at));
            }
            DevConsoleAction::SyncLightingClock => {
                diagnostics_state.lighting.sync();
                info!("Lighting clock: game time");
            }
            DevConsoleAction::ToggleCameraEnvelope => {
                diagnostics_state.camera_envelope_off = !diagnostics_state.camera_envelope_off;
                info!("Camera envelope: {}", if diagnostics_state.camera_envelope_off { "LIFTED" } else { "ON" });
            }
            DevConsoleAction::ToggleTerrainHidden => {
                diagnostics_state.terrain_hidden = !diagnostics_state.terrain_hidden;
                let shown = if diagnostics_state.terrain_hidden { Visibility::Hidden } else { Visibility::Inherited };
                for mut visibility in terrain.iter_mut() {
                    *visibility = shown;
                }
                info!("Terrain: {}", if diagnostics_state.terrain_hidden { "HIDDEN" } else { "shown" });
            }
            DevConsoleAction::ToggleCoverHidden => {
                diagnostics_state.cover_hidden = !diagnostics_state.cover_hidden;
                let shown = if diagnostics_state.cover_hidden { Visibility::Hidden } else { Visibility::Inherited };
                for mut visibility in cover.iter_mut() {
                    *visibility = shown;
                }
                info!("Cover: {}", if diagnostics_state.cover_hidden { "HIDDEN" } else { "shown" });
            }
            DevConsoleAction::ToggleCameraCloseup => {
                diagnostics_state.camera_closeup = !diagnostics_state.camera_closeup;
                info!("Camera close-up: {}", if diagnostics_state.camera_closeup { "ON" } else { "off" });
            }
            DevConsoleAction::WriteMetricsSnapshot => {
                metrics_dump.asked = true;
                info!("Metrics: a snapshot appended to proofs/client/metrics.log");
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
            #[cfg(feature = "admin")]
            DevConsoleAction::SetForcedSummaryRadius(_) => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::ReportTerrain => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::SpawnDen(_) => {}
        }
    }
}


/// Asks the server for the den the console picked, ahead of the player.
#[cfg(feature = "admin")]
pub fn send_spawn_den(
    mut reader: MessageReader<DevConsoleAction>,
    mut writer: MessageWriter<common_bevy::message::Try>,
    player: Query<Entity, (With<common_bevy::components::Actor>, With<common_bevy::components::behaviour::PlayerControlled>)>,
) {
    for action in reader.read() {
        let DevConsoleAction::SpawnDen(archetype) = *action else { continue };
        let Ok(ent) = player.single() else { continue };
        writer.write(common_bevy::message::Try { event: common_bevy::message::Event::SpawnDen { ent, archetype } });
        info!("Spawn den: {archetype:?}");
    }
}
