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

    // Admin actions
    GotoWorldUnits(f64, f64),
    GotoQR(i32, i32),
    /// See the world as the target of the actor the client sees as.
    ViewTarget,
    /// Stop viewing: back as a fresh character.
    StopViewing,
    /// Add this many ms to the latency the client adds to its traffic,
    /// held between none and `AddedLatency::MOST`.
    AddLatency(i64),
    /// Stage a party of this archetype ahead of the actor the client sees
    /// as: a den, a party out of its reach, or one engaging it.
    SpawnParty { archetype: common_bevy::archetype::EnemyArchetype, staging: super::state::Staging },
}

/// System that executes console actions
#[allow(clippy::too_many_arguments)]
pub fn execute_console_actions(
    mut commands: Commands,
    mut state: ResMut<DiagnosticsState>,
    mut added: ResMut<crate::network::AddedLatency>,
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
    for action in reader.read() {
        match action {
            DevConsoleAction::ToggleGrid => {
                state.grid_visible = !state.grid_visible;

                if let Ok((mut visibility, mut overlay)) = grid_query.single_mut() {
                    *visibility = if state.grid_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                    if state.grid_visible {
                        overlay.needs_regeneration = true;
                    }
                }

                if state.grid_visible {
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

                info!("Grid overlay: {}", if state.grid_visible { "ON" } else { "OFF" });
            }
            DevConsoleAction::SetLightingTime(ms_of_day) => {
                state.lighting.hold(game, *ms_of_day);
                info!("Lighting clock: held at {}", state.lighting.held_at().unwrap_or_default());
            }
            DevConsoleAction::ScrubLightingClock(delta) => {
                state.lighting.scrub(game, *delta);
            }
            DevConsoleAction::StepLightingDate(field, steps) => {
                state.lighting.step(game, *field, *steps);
                let at = state.lighting.at(game);
                info!("Lighting clock: held on {}", common_bevy::systems::Date::of(at));
            }
            DevConsoleAction::SyncLightingClock => {
                state.lighting.sync();
                info!("Lighting clock: game time");
            }
            DevConsoleAction::ToggleCameraEnvelope => {
                state.camera_envelope_off = !state.camera_envelope_off;
                info!("Camera envelope: {}", if state.camera_envelope_off { "LIFTED" } else { "ON" });
            }
            DevConsoleAction::ToggleTerrainHidden => {
                state.terrain_hidden = !state.terrain_hidden;
                let shown = if state.terrain_hidden { Visibility::Hidden } else { Visibility::Inherited };
                for mut visibility in terrain.iter_mut() {
                    *visibility = shown;
                }
                info!("Terrain: {}", if state.terrain_hidden { "HIDDEN" } else { "shown" });
            }
            DevConsoleAction::ToggleCoverHidden => {
                state.cover_hidden = !state.cover_hidden;
                let shown = if state.cover_hidden { Visibility::Hidden } else { Visibility::Inherited };
                for mut visibility in cover.iter_mut() {
                    *visibility = shown;
                }
                info!("Cover: {}", if state.cover_hidden { "HIDDEN" } else { "shown" });
            }
            DevConsoleAction::ToggleCameraCloseup => {
                state.camera_closeup = !state.camera_closeup;
                info!("Camera close-up: {}", if state.camera_closeup { "ON" } else { "off" });
            }
            DevConsoleAction::ToggleCanopyParts => {
                state.canopy_parts_off = !state.canopy_parts_off;
                info!("Canopy: {}", if state.canopy_parts_off { "vertices" } else { "parts" });
            }

            DevConsoleAction::GotoWorldUnits(_, _) => {}
            DevConsoleAction::GotoQR(_, _) => {}
            DevConsoleAction::ViewTarget | DevConsoleAction::StopViewing => {}
            DevConsoleAction::SpawnParty { .. } => {}
            DevConsoleAction::AddLatency(ms) => {
                let most = crate::network::AddedLatency::MOST.as_millis() as i64;
                let total = (added.0.as_millis() as i64 + ms).clamp(0, most);
                added.0 = std::time::Duration::from_millis(total as u64);
                info!("Added latency: {total}ms round trip");
            }
        }
    }
}


/// Asks the server to see the world as the target of the actor the client
/// sees as, or stops the view.
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

/// Asks the server to teleport the player to the tile the console named.
pub fn send_goto(
    mut reader: MessageReader<DevConsoleAction>,
    mut writer: MessageWriter<common_bevy::message::Try>,
    player: Query<Entity, (With<Actor>, With<common_bevy::components::behaviour::PlayerControlled>)>,
) {
    for action in reader.read() {
        let (q, r) = match *action {
            DevConsoleAction::GotoWorldUnits(wx, wy) => common::world_to_hex(wx, wy),
            DevConsoleAction::GotoQR(q, r) => (q, r),
            _ => continue,
        };
        let Ok(ent) = player.single() else { continue };
        writer.write(common_bevy::message::Try { event: common_bevy::message::Event::Teleport { ent, q, r } });
        info!("Goto: qr ({q}, {r})");
    }
}

/// The level and size of a party the console stages: one fighter at the
/// balance arena's level.
const PARTY: (u8, u8) = (10, 1);

/// Levels below an actor each of a den's pair stands: the level rule puts
/// a pair this far below about even with one of the actor's level.
const PAIR_DEFICIT: u8 = 3;

/// A den one actor at `level` can beat alone, as `(level, size)`: one NPC
/// at its level, or with `pair` two `PAIR_DEFICIT` levels below it.
fn den_for(level: u8, pair: bool) -> (u8, u8) {
    if pair { (level.saturating_sub(PAIR_DEFICIT), 2) } else { (level, 1) }
}

/// Asks the server for the party the console picked, ahead of the actor
/// the client sees as.
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

#[cfg(test)]
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
