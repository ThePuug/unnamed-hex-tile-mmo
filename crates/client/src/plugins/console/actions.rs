use bevy::prelude::*;

use crate::plugins::diagnostics::DiagnosticsState;
use common_bevy::components::Actor;

/// Events that can be triggered from the developer console
#[derive(Event, Message, Debug)]
pub enum DevConsoleAction {
    /// Move the world's clock to an hour of its day, in ms.
    SetClockHour(u128),
    /// Move the world's clock this many ms, either way.
    ShiftClock(i64),
    /// Hold the world's clock where it is, or let it run on.
    ToggleClockHold,
    /// Run the world's clock at the wall clock again.
    ResetClock,
    /// Roll the whole world back this many ms, of game time or of real.
    Rollback { game: bool, ms: u64 },
    ToggleCameraCloseup,
    /// Hold the sky in the next weather, round again to the forecast.
    CycleWeather,

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
    /// Have the server set a setting to a value.
    Set(common_bevy::settings::Setting, i32),
}

/// System that executes console actions
#[allow(clippy::too_many_arguments)]
pub fn execute_console_actions(
    mut state: ResMut<DiagnosticsState>,
    mut added: ResMut<crate::network::AddedLatency>,
    mut reader: MessageReader<DevConsoleAction>,
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::ToggleCameraCloseup => {
                state.camera_closeup = !state.camera_closeup;
                info!("Camera close-up: {}", if state.camera_closeup { "ON" } else { "off" });
            }
            DevConsoleAction::CycleWeather => {
                state.weather = state.weather.next();
                info!("Weather: {:?}", state.weather);
            }

            DevConsoleAction::GotoWorldUnits(_, _) => {}
            DevConsoleAction::GotoQR(_, _) => {}
            DevConsoleAction::ViewTarget | DevConsoleAction::StopViewing => {}
            DevConsoleAction::SpawnParty { .. } => {}
            DevConsoleAction::Rollback { .. } => {}
            DevConsoleAction::Set(..) => {}
            DevConsoleAction::SetClockHour(_)
            | DevConsoleAction::ShiftClock(_)
            | DevConsoleAction::ToggleClockHold
            | DevConsoleAction::ResetClock => {}
            DevConsoleAction::AddLatency(ms) => {
                let most = crate::network::AddedLatency::MOST.as_millis() as i64;
                let total = (added.0.as_millis() as i64 + ms).clamp(0, most);
                added.0 = std::time::Duration::from_millis(total as u64);
                info!("Added latency: {total}ms round trip");
            }
        }
    }
}


/// Asks the server to set what the Settings menu set.
pub fn send_settings(mut reader: MessageReader<DevConsoleAction>, mut writer: MessageWriter<common_bevy::message::Try>) {
    for action in reader.read() {
        let DevConsoleAction::Set(setting, value) = *action else { continue };
        info!("Setting asked: {} = {value}", setting.name());
        writer.write(common_bevy::message::Try { event: common_bevy::message::Event::Set { setting, value } });
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

/// Asks the server to roll the world back as the console's rollback menu
/// says.
pub fn send_rollback(mut reader: MessageReader<DevConsoleAction>, mut writer: MessageWriter<common_bevy::message::Try>) {
    for action in reader.read() {
        if let DevConsoleAction::Rollback { game, ms } = *action {
            info!("Rollback asked: {ms} ms of {} time", if game { "game" } else { "real" });
            writer.write(common_bevy::message::Try { event: common_bevy::message::Event::Rollback { game, ms } });
        }
    }
}

/// Asks the server to change the world's calendar as the console's clock
/// keys say, for every player.
pub fn send_calendar(
    mut reader: MessageReader<DevConsoleAction>,
    mut writer: MessageWriter<common_bevy::message::Try>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
) {
    use common_bevy::systems::{CalendarChange, DAY_MS};
    let now = server.wall(server.now(time.elapsed().as_millis()));
    for action in reader.read() {
        let change = match *action {
            DevConsoleAction::SetClockHour(ms_of_day) => CalendarChange::Shift((now - now % DAY_MS + ms_of_day) as i64 - now as i64),
            DevConsoleAction::ShiftClock(delta) => CalendarChange::Shift(delta),
            DevConsoleAction::ToggleClockHold if server.calendar.held.is_some() => CalendarChange::Run,
            DevConsoleAction::ToggleClockHold => CalendarChange::HoldAt(now),
            DevConsoleAction::ResetClock => CalendarChange::Reset,
            _ => continue,
        };
        writer.write(common_bevy::message::Try { event: common_bevy::message::Event::ChangeCalendar { change } });
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
        writer.write(common_bevy::message::Try { event: common_bevy::message::Event::Teleport { ent, qrz: qrz::Qrz { q, r, z: 0 } } });
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
