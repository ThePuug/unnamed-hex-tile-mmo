use bevy::{light::ShadowFilteringMethod, prelude::*};

use crate::{
    plugins::diagnostics::{DateField, DiagnosticsState, Shadows, grid::HexGridOverlay},
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
    ToggleMsaa,
    ToggleShadowFilter,
    ToggleSightline,
    ToggleStandMasking,
    ToggleTerrainHidden,
    ToggleForestHidden,
    ToggleCameraCloseup,

    // Top-level toggles
    ToggleMetricsOverlay,
    ToggleMetricsDump,

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
}

/// System that executes console actions
/// Everything the console switches, gathered into one parameter: a
/// system takes at most sixteen, and the actions already read most of
/// the scene in order to act on it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Switches<'w> {
    state: ResMut<'w, DiagnosticsState>,
    dump: ResMut<'w, crate::plugins::diagnostics::MetricsDump>,
    mask_every: ResMut<'w, crate::resources::MaskEveryStand>,
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
    mut camera_msaa: Query<&mut Msaa, With<Camera>>,
    world_camera: Query<Entity, (With<Camera3d>, Without<crate::systems::closeup::CloseupCamera>)>,
    mut sun: Query<&mut DirectionalLight, With<common_bevy::components::Sun>>,
    mut terrain: Query<&mut Visibility, (With<crate::resources::SummaryMesh>, Without<HexGridOverlay>)>,
    mut forest: Query<
        &mut Visibility,
        (
            Or<(
                With<crate::plugins::forest::draw::TreeBatch>,
                With<crate::plugins::forest::draw::CardBatch>,
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
    let mask_every = &mut *switches.mask_every;
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
            DevConsoleAction::ToggleShadowFilter => {
                let shadows = diagnostics_state.shadows.next();
                diagnostics_state.shadows = shadows;
                let method = match shadows {
                    Shadows::Hard => ShadowFilteringMethod::Hardware2x2,
                    _ => ShadowFilteringMethod::Gaussian,
                };
                for camera in world_camera.iter() {
                    commands.entity(camera).insert(method);
                }
                for mut light in sun.iter_mut() {
                    light.shadow_maps_enabled = shadows != Shadows::Off;
                }
                info!("Shadows: {}", shadows.label());
            }
            DevConsoleAction::ToggleSightline => {
                diagnostics_state.sightline_off = !diagnostics_state.sightline_off;
                info!("Sightline tunnel: {}", if diagnostics_state.sightline_off { "OFF" } else { "on" });
            }
            DevConsoleAction::ToggleStandMasking => {
                diagnostics_state.mask_every_stand = !diagnostics_state.mask_every_stand;
                mask_every.0 = diagnostics_state.mask_every_stand;
                info!("Stand masking: {}", if diagnostics_state.mask_every_stand { "EVERY" } else { "only what is reached" });
            }
            DevConsoleAction::ToggleTerrainHidden => {
                diagnostics_state.terrain_hidden = !diagnostics_state.terrain_hidden;
                let shown = if diagnostics_state.terrain_hidden { Visibility::Hidden } else { Visibility::Inherited };
                for mut visibility in terrain.iter_mut() {
                    *visibility = shown;
                }
                info!("Terrain: {}", if diagnostics_state.terrain_hidden { "HIDDEN" } else { "shown" });
            }
            DevConsoleAction::ToggleForestHidden => {
                diagnostics_state.forest_hidden = !diagnostics_state.forest_hidden;
                let shown = if diagnostics_state.forest_hidden { Visibility::Hidden } else { Visibility::Inherited };
                for mut visibility in forest.iter_mut() {
                    *visibility = shown;
                }
                info!("Forest: {}", if diagnostics_state.forest_hidden { "HIDDEN" } else { "shown" });
            }
            DevConsoleAction::ToggleCameraCloseup => {
                diagnostics_state.camera_closeup = !diagnostics_state.camera_closeup;
                info!("Camera close-up: {}", if diagnostics_state.camera_closeup { "ON" } else { "off" });
            }
            DevConsoleAction::ToggleMsaa => {
                let samples = diagnostics_state.samples.next();
                diagnostics_state.samples = samples;
                // Every camera on the window, or the ones left behind stop
                // sharing its main texture and draw the UI a second time.
                for mut msaa in camera_msaa.iter_mut() {
                    *msaa = match samples {
                        crate::plugins::diagnostics::Samples::Four => Msaa::Sample4,
                        crate::plugins::diagnostics::Samples::Two => Msaa::Sample2,
                        crate::plugins::diagnostics::Samples::Off => Msaa::Off,
                    };
                }
                info!("MSAA: {}", samples.label());
            }
            DevConsoleAction::ToggleMetricsDump => {
                metrics_dump.on = !metrics_dump.on;
                info!("Metrics dump: {}", if metrics_dump.on { "ON (proofs/client/metrics.txt)" } else { "off" });
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
        }
    }
}

