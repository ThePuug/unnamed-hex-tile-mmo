use bevy::prelude::*;

use crate::{
    plugins::diagnostics::{DateField, DiagnosticsState, grid::HexGridOverlay},
    components::PlayerOriginDebug,
};
use common_bevy::components::Actor;

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
    ToggleCanopyParts,

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
    /// See the world as the target of the actor the client sees as.
    #[cfg(feature = "admin")]
    ViewTarget,
    /// Stop viewing: back as a fresh character.
    #[cfg(feature = "admin")]
    StopViewing,
    /// Stage a party of this archetype ahead of the actor the client sees
    /// as: a den, a party out of its reach, or one engaging it.
    #[cfg(feature = "admin")]
    SpawnParty { archetype: common_bevy::spatial_difficulty::EnemyArchetype, staging: super::state::Staging },
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
    actor_query: Query<Entity, With<Actor>>,
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
            DevConsoleAction::ToggleCanopyParts => {
                diagnostics_state.canopy_parts_off = !diagnostics_state.canopy_parts_off;
                info!("Canopy: {}", if diagnostics_state.canopy_parts_off { "vertices" } else { "parts" });
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
            DevConsoleAction::ViewTarget | DevConsoleAction::StopViewing => {}
            #[cfg(feature = "admin")]
            DevConsoleAction::SpawnParty { .. } => {}
        }
    }
}


/// Asks the server to see the world as the target of the actor the client
/// sees as, or stops the view.
#[cfg(feature = "admin")]
pub fn send_view(
    mut reader: MessageReader<DevConsoleAction>,
    mut writer: MessageWriter<common_bevy::message::Try>,
    seer: Query<&common_bevy::components::target::Target, With<crate::components::Viewed>>,
    mut next: ResMut<NextState<crate::plugins::shell::Stage>>,
    mut entered: ResMut<crate::plugins::shell::Entered>,
    mut rejoin: ResMut<crate::plugins::shell::view::Rejoin>,
    mut viewing: ResMut<crate::plugins::shell::view::Viewing>,
) {
    use crate::plugins::shell::view;
    for action in reader.read() {
        match action {
            DevConsoleAction::ViewTarget => {
                let Some(ent) = seer.single().ok().and_then(|target| target.entity.or(target.last_target)) else {
                    info!("View: nothing targeted");
                    continue;
                };
                writer.write(common_bevy::message::Try { event: common_bevy::message::Event::View { ent } });
                info!("View: {ent}");
            }
            DevConsoleAction::StopViewing if viewing.0.is_some() => {
                view::stop(&mut writer, &mut next, &mut entered, &mut rejoin, &mut viewing);
            }
            _ => {}
        }
    }
}

/// The level and size of a party the console stages: one fighter at the
/// balance arena's level.
#[cfg(feature = "admin")]
const PARTY: (u8, u8) = (10, 1);

/// Levels below an actor each of a den's pair stands: the level rule puts
/// a pair this far below about even with one of the actor's level.
#[cfg(feature = "admin")]
const PAIR_DEFICIT: u8 = 3;

/// A den one actor at `level` can beat alone, as `(level, size)`: one NPC
/// at its level, or with `pair` two `PAIR_DEFICIT` levels below it.
#[cfg(feature = "admin")]
fn den_for(level: u8, pair: bool) -> (u8, u8) {
    if pair { (level.saturating_sub(PAIR_DEFICIT), 2) } else { (level, 1) }
}

/// Asks the server for the party the console picked, ahead of the actor
/// the client sees as.
#[cfg(feature = "admin")]
pub fn send_spawn_party(
    mut reader: MessageReader<DevConsoleAction>,
    mut writer: MessageWriter<common_bevy::message::Try>,
    seer: Query<(Entity, &common_bevy::components::ActorAttributes), With<crate::components::Viewed>>,
) {
    use super::state::Staging;
    for action in reader.read() {
        let DevConsoleAction::SpawnParty { archetype, staging } = *action else { continue };
        let Ok((ent, attrs)) = seer.single() else { continue };
        let (level, size) = match staging {
            Staging::Den => den_for(attrs.total_level().min(u8::MAX as u32) as u8, rand::Rng::random_bool(&mut rand::rng(), 0.5)),
            Staging::Party | Staging::Opposition => PARTY,
        };
        let engage = staging == Staging::Opposition;
        writer.write(common_bevy::message::Try { event: common_bevy::message::Event::SpawnParty { ent, archetype, level, size, engage } });
        info!("Stage: {size}x{archetype:?}@{level}{}", if engage { ", engaging" } else { "" });
    }
}

#[cfg(all(test, feature = "admin"))]
mod tests {
    use super::*;

    #[test]
    fn a_den_is_one_at_the_actors_level_or_a_pair_below_it() {
        for actor in [1, 3, 10, 20] {
            assert_eq!(den_for(actor, false), (actor, 1));
            let (level, size) = den_for(actor, true);
            assert_eq!(size, 2);
            assert!(level < actor, "a pair stands below a level-{actor} actor");
        }
    }
}
