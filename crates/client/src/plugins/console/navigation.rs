use bevy::prelude::*;

use super::{
    state::{DevConsole, MenuPath, GotoCoordType, GotoInputState},
    actions::DevConsoleAction,
};

/// System that handles numpad input for console navigation
pub fn handle_console_input(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut console: ResMut<DevConsole>,
    mut action_writer: MessageWriter<DevConsoleAction>,
    time: Res<Time>,
    #[cfg(feature = "admin")] flyover: Res<crate::plugins::flyover::FlyoverState>,
) {
    // Toggle console visibility with NumpadDivide
    if keyboard.just_pressed(KeyCode::NumpadDivide) {
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
    #[cfg(feature = "admin")]
    let uses_escape_back = matches!(
        console.current_menu,
        MenuPath::GotoInput | MenuPath::SummaryRadius | MenuPath::LightingTime
    );
    #[cfg(not(feature = "admin"))]
    let uses_escape_back = matches!(console.current_menu, MenuPath::LightingTime);

    let back_pressed = if uses_escape_back {
        keyboard.just_pressed(KeyCode::Escape)
    } else {
        keyboard.just_pressed(KeyCode::Numpad0)
    };

    if back_pressed {
        if console.current_menu == MenuPath::Root {
            console.visible = false;
        } else {
            #[cfg(feature = "admin")]
            if matches!(console.current_menu, MenuPath::GotoInput) {
                console.goto_input = None;
            }
            #[cfg(feature = "admin")]
            if matches!(console.current_menu, MenuPath::SummaryRadius) {
                console.summary_radius_buf.clear();
            }
            if matches!(console.current_menu, MenuPath::LightingTime) {
                console.lighting_time_buf.clear();
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
        MenuPath::Root => {
            #[cfg(feature = "admin")]
            handle_root_menu(&mut keyboard, &mut console, &mut action_writer);
            #[cfg(not(feature = "admin"))]
            handle_root_menu(&mut keyboard, &mut console, &mut action_writer);
        }
        MenuPath::Terrain => handle_terrain_menu(&mut keyboard, &mut console, &mut action_writer),
        MenuPath::LightingTime => handle_lighting_time(&mut keyboard, &mut console, &mut action_writer, time.delta_secs()),
        MenuPath::Video => handle_video_menu(&mut keyboard, &mut action_writer),
        #[cfg(feature = "admin")]
        MenuPath::Flyover => handle_flyover_menu(&mut keyboard, &mut console, &mut action_writer, &flyover),
        #[cfg(feature = "admin")]
        MenuPath::GotoSelect => handle_goto_select_menu(&mut keyboard, &mut console),
        #[cfg(feature = "admin")]
        MenuPath::GotoInput => handle_goto_input(&mut keyboard, &mut console, &mut action_writer),
        #[cfg(feature = "admin")]
        MenuPath::SummaryRadius => handle_summary_radius(&mut keyboard, &mut console, &mut action_writer),
    }
}

fn handle_root_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    // Submenus first
    if keyboard.just_pressed(KeyCode::Numpad1) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Terrain;
        consumed = Some(KeyCode::Numpad1);
    }

    #[cfg(feature = "admin")]
    if consumed.is_none() && keyboard.just_pressed(KeyCode::Numpad2) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Flyover;
        consumed = Some(KeyCode::Numpad2);
    }

    let video_key = if cfg!(feature = "admin") { KeyCode::Numpad3 } else { KeyCode::Numpad2 };
    if consumed.is_none() && keyboard.just_pressed(video_key) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Video;
        consumed = Some(video_key);
    }

    // Toggles after submenus
    let toggle_key = if cfg!(feature = "admin") { KeyCode::Numpad4 } else { KeyCode::Numpad3 };
    if consumed.is_none() && keyboard.just_pressed(toggle_key) {
        action_writer.write(DevConsoleAction::ToggleMetricsOverlay);
        consumed = Some(toggle_key);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_terrain_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.write(DevConsoleAction::ToggleGrid);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::LightingTime;
        console.lighting_time_buf.clear();
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.write(DevConsoleAction::ToggleCameraEnvelope);
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        action_writer.write(DevConsoleAction::ToggleTerrainHidden);
        consumed = Some(KeyCode::Numpad4);
    } else if keyboard.just_pressed(KeyCode::Numpad5) {
        action_writer.write(DevConsoleAction::ToggleCameraCloseup);
        consumed = Some(KeyCode::Numpad5);
    } else if keyboard.just_pressed(KeyCode::Numpad6) {
        action_writer.write(DevConsoleAction::ToggleForestHidden);
        consumed = Some(KeyCode::Numpad6);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

/// A held arrow scrubs the lighting clock at this many times game time's
/// pace, eased up to from that pace over this many seconds of holding.
const SCRUB_MAX_RATE: f32 = 60.0;
const SCRUB_EASE_SECS: f32 = 3.0;

/// Digits typed into the lighting hour; Enter holds the clock there, or
/// with nothing typed returns it to game time. Left and right arrows
/// rewind and forward it, quickening the longer they are held. Tab picks a
/// field of the date and up and down step it.
fn handle_lighting_time(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
    dt: f32,
) {
    use crate::plugins::diagnostics::LightingClock;

    let dir = keyboard.pressed(KeyCode::ArrowRight) as i32 - keyboard.pressed(KeyCode::ArrowLeft) as i32;
    if dir != 0 {
        console.lighting_scrub_secs += dt;
        let eased = (console.lighting_scrub_secs / SCRUB_EASE_SECS).min(1.0);
        let rate = 1.0 + (SCRUB_MAX_RATE - 1.0) * eased * eased;
        action_writer.write(DevConsoleAction::ScrubLightingClock((dir as f32 * rate * dt * 1000.0) as i128));
    } else if console.lighting_scrub_secs != 0.0 {
        console.lighting_scrub_secs = 0.0;
    }

    if keyboard.just_pressed(KeyCode::Tab) {
        console.lighting_date_field = console.lighting_date_field.next();
        keyboard.clear_just_pressed(KeyCode::Tab);
    }
    let steps = keyboard.just_pressed(KeyCode::ArrowUp) as i32 - keyboard.just_pressed(KeyCode::ArrowDown) as i32;
    if steps != 0 {
        action_writer.write(DevConsoleAction::StepLightingDate(console.lighting_date_field, steps));
    }

    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        let buf = console.lighting_time_buf.trim().to_string();
        if buf.is_empty() {
            action_writer.write(DevConsoleAction::SyncLightingClock);
        } else if let Some(ms_of_day) = LightingClock::parse_time(&buf) {
            action_writer.write(DevConsoleAction::SetLightingTime(ms_of_day));
        } else {
            info!("Lighting time: invalid input '{buf}'");
            return;
        }
        console.lighting_time_buf.clear();
        console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        return;
    }

    for &(key, ch) in DIGIT_KEYS {
        if keyboard.just_pressed(key) {
            console.lighting_time_buf.push(ch);
            keyboard.clear_just_pressed(key);
        }
    }

    if keyboard.just_pressed(KeyCode::Backspace) {
        console.lighting_time_buf.pop();
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

fn handle_video_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.write(DevConsoleAction::ToggleMsaa);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.write(DevConsoleAction::ToggleShadowFilter);
        consumed = Some(KeyCode::Numpad2);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

#[cfg(feature = "admin")]
fn handle_flyover_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
    flyover: &crate::plugins::flyover::FlyoverState,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.write(DevConsoleAction::ToggleFlyover);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::GotoSelect;
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) && flyover.active {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::SummaryRadius;
        console.summary_radius_buf.clear();
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) && flyover.active {
        action_writer.write(DevConsoleAction::ReportTerrain);
        consumed = Some(KeyCode::Numpad4);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

#[cfg(feature = "admin")]
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

#[cfg(feature = "admin")]
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
                    console.current_menu = MenuPath::Flyover;
                    console.history.retain(|p| !matches!(p, MenuPath::Flyover | MenuPath::GotoSelect | MenuPath::GotoInput));
                } else {
                    info!("Goto: invalid world unit coordinates");
                }
            }
            GotoCoordType::QR => {
                if let (Ok(q), Ok(r)) = (a.parse::<i32>(), b.parse::<i32>()) {
                    action_writer.write(DevConsoleAction::GotoQR(q, r));
                    console.goto_input = None;
                    console.current_menu = MenuPath::Flyover;
                    console.history.retain(|p| !matches!(p, MenuPath::Flyover | MenuPath::GotoSelect | MenuPath::GotoInput));
                } else {
                    info!("Goto: invalid QR coordinates");
                }
            }
        }

        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        return;
    }

    let digit_keys: &[(KeyCode, char)] = &[
        (KeyCode::Digit0, '0'), (KeyCode::Digit1, '1'), (KeyCode::Digit2, '2'),
        (KeyCode::Digit3, '3'), (KeyCode::Digit4, '4'), (KeyCode::Digit5, '5'),
        (KeyCode::Digit6, '6'), (KeyCode::Digit7, '7'), (KeyCode::Digit8, '8'),
        (KeyCode::Digit9, '9'),
        (KeyCode::Numpad0, '0'), (KeyCode::Numpad1, '1'), (KeyCode::Numpad2, '2'),
        (KeyCode::Numpad3, '3'), (KeyCode::Numpad4, '4'), (KeyCode::Numpad5, '5'),
        (KeyCode::Numpad6, '6'), (KeyCode::Numpad7, '7'), (KeyCode::Numpad8, '8'),
        (KeyCode::Numpad9, '9'),
        (KeyCode::Minus, '-'), (KeyCode::NumpadSubtract, '-'),
        (KeyCode::Period, '.'), (KeyCode::NumpadDecimal, '.'),
    ];

    for &(key, ch) in digit_keys {
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

#[cfg(feature = "admin")]
fn handle_summary_radius(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    // Enter: submit. Empty → Auto (None). Number → Some(r).
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        let buf = console.summary_radius_buf.trim().to_string();
        let value = if buf.is_empty() {
            None // Auto
        } else if let Ok(r) = buf.parse::<u32>() {
            Some(r)
        } else {
            info!("Summary radius: invalid input '{buf}'");
            keyboard.clear_just_pressed(KeyCode::Enter);
            keyboard.clear_just_pressed(KeyCode::NumpadEnter);
            return;
        };
        action_writer.write(DevConsoleAction::SetForcedSummaryRadius(value));
        console.summary_radius_buf.clear();
        console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        return;
    }

    for &(key, ch) in DIGIT_KEYS {
        if keyboard.just_pressed(key) {
            console.summary_radius_buf.push(ch);
            keyboard.clear_just_pressed(key);
        }
    }

    if keyboard.just_pressed(KeyCode::Backspace) {
        console.summary_radius_buf.pop();
        keyboard.clear_just_pressed(KeyCode::Backspace);
    }
}
