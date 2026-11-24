use bevy::prelude::*;

use super::state::{GotoCoordType, GotoInputState, Staging};
use common_bevy::settings::Setting;

use super::{
    state::{DevConsole, MenuPath},
    actions::DevConsoleAction,
};

/// Reads the numpad for the console. The console is marked changed only
/// when a key changed it: the handlers work on the state unwatched, and
/// the menu is rebuilt (`ui::update_console_menu`) for a change, never for
/// a frame.
pub fn handle_console_input(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut console: ResMut<DevConsole>,
    panel: Res<crate::systems::character_panel::CharacterPanelState>,
    mut action_writer: MessageWriter<DevConsoleAction>,
    time: Res<Time>,
) {
    let before = console.clone();
    let state = console.bypass_change_detection();
    handle(&mut keyboard, state, &panel, &mut action_writer, time.delta_secs());
    if *state != before {
        console.set_changed();
    }
}

fn handle(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    panel: &crate::systems::character_panel::CharacterPanelState,
    action_writer: &mut MessageWriter<DevConsoleAction>,
    dt: f32,
) {
    // Toggle console visibility with NumpadDivide. The open character panel
    // has the numpad, so the console does not open over it.
    let panel_has_numpad = panel.visible && !console.visible;
    if keyboard.just_pressed(KeyCode::NumpadDivide) && !panel_has_numpad {
        console.visible = !console.visible;

        if console.visible {
            console.current_menu = MenuPath::Root;
            console.history.clear();
        }

        keyboard.clear_just_pressed(KeyCode::NumpadDivide);
        return;
    }

    if !console.visible {
        return;
    }

    // Back key: Numpad0 normally, Escape when numpad digits have other meaning
    let uses_escape_back = matches!(
        console.current_menu,
        MenuPath::GotoInput | MenuPath::WorldClock | MenuPath::Setting(_)
    );

    let back_pressed = if uses_escape_back {
        keyboard.just_pressed(KeyCode::Escape)
    } else {
        keyboard.just_pressed(KeyCode::Numpad0)
    };

    if back_pressed {
        if console.current_menu == MenuPath::Root {
            console.visible = false;
        } else {
            if matches!(console.current_menu, MenuPath::GotoInput) {
                console.goto_input = None;
            }
            if matches!(console.current_menu, MenuPath::WorldClock) {
                console.clock_buf.clear();
            }
            if matches!(console.current_menu, MenuPath::Setting(_)) {
                console.setting_buf.clear();
            }
            console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        }

        if uses_escape_back {
            keyboard.clear_just_pressed(KeyCode::Escape);
        } else {
            keyboard.clear_just_pressed(KeyCode::Numpad0);
        }
        return;
    }

    // Handle menu-specific inputs
    match console.current_menu {
        MenuPath::Root | MenuPath::World | MenuPath::Travel | MenuPath::Stages => handle_group_menu(keyboard, console),
        MenuPath::Settings => handle_settings_menu(keyboard, console),
        MenuPath::Setting(setting) => handle_setting(keyboard, console, action_writer, setting),
        MenuPath::Client => handle_client_menu(keyboard, console, action_writer),
        MenuPath::WorldClock => handle_world_clock(keyboard, console, action_writer, dt),
        MenuPath::Rollback => handle_rollback_menu(keyboard, console, action_writer),
        MenuPath::GotoSelect => handle_goto_select_menu(keyboard, console),
        MenuPath::GotoInput => handle_goto_input(keyboard, console, action_writer),
        MenuPath::View => handle_view_menu(keyboard, action_writer),
        MenuPath::Latency => handle_latency_menu(keyboard, action_writer),
        MenuPath::Stage(staging) => handle_stage_menu(keyboard, action_writer, staging),
    }
}

/// The menus a group's rows open, in row order from numpad 1.
pub fn rows(menu: &MenuPath) -> &'static [MenuPath] {
    match menu {
        MenuPath::Root => &[MenuPath::World, MenuPath::Travel, MenuPath::Stages, MenuPath::Settings, MenuPath::Client],
        MenuPath::World => &[MenuPath::WorldClock, MenuPath::Rollback],
        MenuPath::Travel => &[MenuPath::GotoSelect, MenuPath::View],
        MenuPath::Stages => &[MenuPath::Stage(Staging::Den), MenuPath::Stage(Staging::Party), MenuPath::Stage(Staging::Opposition)],
        _ => &[],
    }
}

/// The numpad's digits, 1 on.
const ROW_KEYS: [KeyCode; 9] = [
    KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3, KeyCode::Numpad4, KeyCode::Numpad5,
    KeyCode::Numpad6, KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9,
];

/// Opens `menu` from the one showing, as it is first shown.
fn open(console: &mut DevConsole, menu: MenuPath) {
    match menu {
        MenuPath::WorldClock => console.clock_buf.clear(),
        MenuPath::Setting(_) => console.setting_buf.clear(),
        MenuPath::Rollback => console.rollback.armed = false,
        _ => {}
    }
    console.history.push(console.current_menu.clone());
    console.current_menu = menu;
}

/// A menu of menus: a digit opens the row it numbers.
fn handle_group_menu(keyboard: &mut ButtonInput<KeyCode>, console: &mut DevConsole) {
    for (key, menu) in ROW_KEYS.into_iter().zip(rows(&console.current_menu.clone())) {
        if keyboard.just_pressed(key) {
            keyboard.clear_just_pressed(key);
            open(console, menu.clone());
            return;
        }
    }
}

/// A digit opens the setting its row lists.
fn handle_settings_menu(keyboard: &mut ButtonInput<KeyCode>, console: &mut DevConsole) {
    // Every setting has a key, or the zip drops the rows past the last
    const _: () = assert!(Setting::ALL.len() <= ROW_KEYS.len());
    for (key, setting) in ROW_KEYS.into_iter().zip(Setting::ALL) {
        if keyboard.just_pressed(key) {
            keyboard.clear_just_pressed(key);
            open(console, MenuPath::Setting(setting));
            return;
        }
    }
}

/// One setting: digits typed and Enter ask the server to set it to that
/// value, or with nothing typed to what it is where no admin has changed
/// it. Either way the console goes back to the settings.
fn handle_setting(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
    setting: Setting,
) {
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        let buf = console.setting_buf.trim();
        let value = if buf.is_empty() { Some(setting.default()) } else { buf.parse().ok() };
        let Some(value) = value else {
            info!("Setting: invalid input '{buf}'");
            return;
        };
        action_writer.write(DevConsoleAction::Set(setting, value));
        console.setting_buf.clear();
        console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        return;
    }

    for &(key, ch) in DIGIT_KEYS {
        if keyboard.just_pressed(key) {
            console.setting_buf.push(ch);
            keyboard.clear_just_pressed(key);
        }
    }

    if keyboard.just_pressed(KeyCode::Backspace) {
        console.setting_buf.pop();
        keyboard.clear_just_pressed(KeyCode::Backspace);
    }
}

fn handle_client_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let toggles = [DevConsoleAction::ToggleCameraCloseup, DevConsoleAction::CycleWeather];
    let latency = ROW_KEYS[toggles.len()];
    for (key, action) in ROW_KEYS.into_iter().zip(toggles) {
        if keyboard.just_pressed(key) {
            action_writer.write(action);
            keyboard.clear_just_pressed(key);
            return;
        }
    }
    if keyboard.just_pressed(latency) {
        keyboard.clear_just_pressed(latency);
        open(console, MenuPath::Latency);
    }
}

/// A held arrow scrubs the world's clock at this many times game time's
/// pace, eased up to from that pace over this many seconds of holding.
const SCRUB_MAX_RATE: f32 = 60.0;
const SCRUB_EASE_SECS: f32 = 3.0;

/// The world's clock, for every player: digits typed as an hour and Enter
/// move the clock to that hour of its day, or with nothing typed run it at
/// the wall clock again. Left and right arrows rewind and forward it,
/// quickening the longer they are held. Tab picks a field of the date and
/// up and down step the clock by it. Space holds it, or lets it run on.
fn handle_world_clock(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
    dt: f32,
) {
    let dir = keyboard.pressed(KeyCode::ArrowRight) as i32 - keyboard.pressed(KeyCode::ArrowLeft) as i32;
    if dir != 0 {
        console.clock_scrub_secs += dt;
        let eased = (console.clock_scrub_secs / SCRUB_EASE_SECS).min(1.0);
        let rate = 1.0 + (SCRUB_MAX_RATE - 1.0) * eased * eased;
        action_writer.write(DevConsoleAction::ShiftClock((dir as f32 * rate * dt * 1000.0) as i64));
    } else if console.clock_scrub_secs != 0.0 {
        console.clock_scrub_secs = 0.0;
    }

    if keyboard.just_pressed(KeyCode::Tab) {
        console.clock_date_field = console.clock_date_field.next();
        keyboard.clear_just_pressed(KeyCode::Tab);
    }
    let steps = keyboard.just_pressed(KeyCode::ArrowUp) as i32 - keyboard.just_pressed(KeyCode::ArrowDown) as i32;
    if steps != 0 {
        action_writer.write(DevConsoleAction::ShiftClock(steps as i64 * console.clock_date_field.unit() as i64));
    }
    if keyboard.just_pressed(KeyCode::Space) {
        action_writer.write(DevConsoleAction::ToggleClockHold);
        keyboard.clear_just_pressed(KeyCode::Space);
    }

    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        let buf = console.clock_buf.trim().to_string();
        if buf.is_empty() {
            action_writer.write(DevConsoleAction::ResetClock);
        } else if let Some(ms_of_day) = crate::plugins::diagnostics::parse_time(&buf) {
            action_writer.write(DevConsoleAction::SetClockHour(ms_of_day));
        } else {
            info!("World clock: invalid input '{buf}'");
            return;
        }
        console.clock_buf.clear();
        console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        return;
    }

    for &(key, ch) in DIGIT_KEYS {
        if keyboard.just_pressed(key) {
            console.clock_buf.push(ch);
            keyboard.clear_just_pressed(key);
        }
    }

    if keyboard.just_pressed(KeyCode::Backspace) {
        console.clock_buf.pop();
        keyboard.clear_just_pressed(KeyCode::Backspace);
    }
}

/// Digit keys, numpad and top row, and the digit each types.
const DIGIT_KEYS: &[(KeyCode, char)] = &[
    (KeyCode::Digit0, '0'), (KeyCode::Digit1, '1'), (KeyCode::Digit2, '2'),
    (KeyCode::Digit3, '3'), (KeyCode::Digit4, '4'), (KeyCode::Digit5, '5'),
    (KeyCode::Digit6, '6'), (KeyCode::Digit7, '7'), (KeyCode::Digit8, '8'),
    (KeyCode::Digit9, '9'),
    (KeyCode::Numpad0, '0'), (KeyCode::Numpad1, '1'), (KeyCode::Numpad2, '2'),
    (KeyCode::Numpad3, '3'), (KeyCode::Numpad4, '4'), (KeyCode::Numpad5, '5'),
    (KeyCode::Numpad6, '6'), (KeyCode::Numpad7, '7'), (KeyCode::Numpad8, '8'),
    (KeyCode::Numpad9, '9'),
];

/// The sign and the point a coordinate may carry, and the character each
/// types.
const SIGN_KEYS: &[(KeyCode, char)] = &[
    (KeyCode::Minus, '-'), (KeyCode::NumpadSubtract, '-'),
    (KeyCode::Period, '.'), (KeyCode::NumpadDecimal, '.'),
];

/// Numpad 1 on stages the archetype `DENS` lists in that row, and stays in
/// the menu so another can follow.
fn handle_stage_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut MessageWriter<DevConsoleAction>,
    staging: Staging,
) {
    // Every archetype listed has a key, or the zip drops the rows past the last
    const _: () = assert!(super::state::DENS.len() <= ROW_KEYS.len());
    for (key, (_, archetype)) in ROW_KEYS.into_iter().zip(super::state::DENS) {
        if keyboard.just_pressed(key) {
            action_writer.write(DevConsoleAction::SpawnParty { archetype, staging });
            keyboard.clear_just_pressed(key);
        }
    }
}

fn handle_view_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    for (key, action) in [(KeyCode::Numpad1, DevConsoleAction::ViewTarget), (KeyCode::Numpad2, DevConsoleAction::StopViewing)] {
        if keyboard.just_pressed(key) {
            action_writer.write(action);
            keyboard.clear_just_pressed(key);
        }
    }
}

/// How far one press moves the added latency
pub const LATENCY_STEP_MS: i64 = 25;

/// The rollback: 1 picks game or real time, 2 the unit, 3 and 4 fewer
/// and more of it. Enter arms it and Enter again asks the server for it;
/// any other key disarms. Asked for, the console shuts: the server stops
/// under it, and an Enter pressed to play again must not ask twice.
fn handle_rollback_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut chosen = console.rollback;
    if keyboard.just_pressed(KeyCode::NumpadEnter) {
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        if chosen.armed {
            action_writer.write(DevConsoleAction::Rollback { game: chosen.game, ms: chosen.ms() });
            console.rollback.armed = false;
            console.visible = false;
            console.current_menu = MenuPath::Root;
            console.history.clear();
            return;
        }
        chosen.armed = true;
    }
    for (key, change) in [
        (KeyCode::Numpad1, (|r: &mut super::state::Rollback| r.game = !r.game) as fn(&mut super::state::Rollback)),
        (KeyCode::Numpad2, |r| r.unit = (r.unit + 1) % super::state::Rollback::UNITS.len()),
        (KeyCode::Numpad3, |r| r.amount = r.amount.saturating_sub(1).max(1)),
        (KeyCode::Numpad4, |r| r.amount = (r.amount + 1).min(999)),
    ] {
        if keyboard.just_pressed(key) {
            keyboard.clear_just_pressed(key);
            change(&mut chosen);
            chosen.armed = false;
        }
    }
    if chosen != console.rollback {
        console.rollback = chosen;
    }
}

fn handle_latency_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let most = crate::network::AddedLatency::MOST.as_millis() as i64;
    for (key, ms) in [(KeyCode::Numpad1, LATENCY_STEP_MS), (KeyCode::Numpad2, -LATENCY_STEP_MS), (KeyCode::Numpad3, -most)] {
        if keyboard.just_pressed(key) {
            action_writer.write(DevConsoleAction::AddLatency(ms));
            keyboard.clear_just_pressed(key);
        }
    }
}

fn handle_goto_select_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::GotoInput;
        console.goto_input = Some(GotoInputState::new(GotoCoordType::WorldUnits));
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::GotoInput;
        console.goto_input = Some(GotoInputState::new(GotoCoordType::QR));
        consumed = Some(KeyCode::Numpad2);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_goto_input(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let Some(ref mut input) = console.goto_input else { return };

    if keyboard.just_pressed(KeyCode::Tab) {
        input.active_field = 1 - input.active_field;
        keyboard.clear_just_pressed(KeyCode::Tab);
        return;
    }

    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        let a = input.buffers[0].trim().to_string();
        let b = input.buffers[1].trim().to_string();

        match input.coord_type {
            GotoCoordType::WorldUnits => {
                if let (Ok(x), Ok(y)) = (a.parse::<f64>(), b.parse::<f64>()) {
                    action_writer.write(DevConsoleAction::GotoWorldUnits(x, y));
                    console.goto_input = None;
                    console.current_menu = MenuPath::Root;
                    console.history.clear();
                } else {
                    info!("Goto: invalid world unit coordinates");
                }
            }
            GotoCoordType::QR => {
                if let (Ok(q), Ok(r)) = (a.parse::<i32>(), b.parse::<i32>()) {
                    action_writer.write(DevConsoleAction::GotoQR(q, r));
                    console.goto_input = None;
                    console.current_menu = MenuPath::Root;
                    console.history.clear();
                } else {
                    info!("Goto: invalid QR coordinates");
                }
            }
        }

        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        return;
    }

    for &(key, ch) in DIGIT_KEYS.iter().chain(SIGN_KEYS) {
        if keyboard.just_pressed(key) {
            if ch == '-' && !input.buffers[input.active_field].is_empty() {
                keyboard.clear_just_pressed(key);
                continue;
            }
            if ch == '.' {
                if input.coord_type == GotoCoordType::QR || input.buffers[input.active_field].contains('.') {
                    keyboard.clear_just_pressed(key);
                    continue;
                }
            }
            input.buffers[input.active_field].push(ch);
            keyboard.clear_just_pressed(key);
        }
    }

    if keyboard.just_pressed(KeyCode::Backspace) {
        input.buffers[input.active_field].pop();
        keyboard.clear_just_pressed(KeyCode::Backspace);
    }
}
