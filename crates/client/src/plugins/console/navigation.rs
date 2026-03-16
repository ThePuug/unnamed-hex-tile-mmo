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
) {
    // Toggle console visibility with NumpadDivide
    if keyboard.just_pressed(KeyCode::NumpadDivide) {
        console.visible = !console.visible;

        // Reset to root menu when opening
        if console.visible {
            console.current_menu = MenuPath::Root;
            console.history.clear();
        }

        // Consume the divide key
        keyboard.clear_just_pressed(KeyCode::NumpadDivide);
        return;
    }

    // Only process menu navigation if console is visible
    if !console.visible {
        return;
    }

    // Back key: Numpad0 normally, Escape when in text input mode
    #[cfg(feature = "admin")]
    let in_text_input = matches!(console.current_menu, MenuPath::GotoInput);
    #[cfg(not(feature = "admin"))]
    let in_text_input = false;

    let back_pressed = if in_text_input {
        keyboard.just_pressed(KeyCode::Escape)
    } else {
        keyboard.just_pressed(KeyCode::Numpad0)
    };

    if back_pressed {
        if console.current_menu == MenuPath::Root {
            console.visible = false;
        } else {
            #[cfg(feature = "admin")]
            if in_text_input {
                console.goto_input = None;
            }
            console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        }

        if in_text_input {
            keyboard.clear_just_pressed(KeyCode::Escape);
        } else {
            keyboard.clear_just_pressed(KeyCode::Numpad0);
        }
        return;
    }

    // Handle menu-specific inputs
    match console.current_menu {
        MenuPath::Root => handle_root_menu(&mut keyboard, &mut console),
        MenuPath::Terrain => handle_terrain_menu(&mut keyboard, &mut action_writer),
        MenuPath::Performance => handle_performance_menu(&mut keyboard, &mut action_writer),
        #[cfg(feature = "admin")]
        MenuPath::Admin => handle_admin_menu(&mut keyboard, &mut console, &mut action_writer),
        #[cfg(feature = "admin")]
        MenuPath::GotoSelect => handle_goto_select_menu(&mut keyboard, &mut console),
        #[cfg(feature = "admin")]
        MenuPath::GotoInput => handle_goto_input(&mut keyboard, &mut console, &mut action_writer),
    }
}

fn handle_root_menu(keyboard: &mut ButtonInput<KeyCode>, console: &mut DevConsole) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Terrain;
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Performance;
        consumed = Some(KeyCode::Numpad2);
    }

    #[cfg(feature = "admin")]
    if consumed.is_none() && keyboard.just_pressed(KeyCode::Numpad3) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Admin;
        consumed = Some(KeyCode::Numpad3);
    }

    // Consume the input
    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_terrain_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.write(DevConsoleAction::ToggleGrid);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.write(DevConsoleAction::ToggleSlopeRendering);
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.write(DevConsoleAction::ToggleFixedLighting);
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        action_writer.write(DevConsoleAction::RegenerateMesh);
        consumed = Some(KeyCode::Numpad4);
    } else if keyboard.just_pressed(KeyCode::Numpad5) {
        action_writer.write(DevConsoleAction::ToggleTerrainDetail);
        consumed = Some(KeyCode::Numpad5);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_performance_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.write(DevConsoleAction::TogglePerfUI);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.write(DevConsoleAction::ToggleNetworkUI);
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.write(DevConsoleAction::ToggleMetricsOverlay);
        consumed = Some(KeyCode::Numpad3);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

#[cfg(feature = "admin")]
fn handle_admin_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    console: &mut DevConsole,
    action_writer: &mut MessageWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.write(DevConsoleAction::ToggleFlyover);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::GotoSelect;
        consumed = Some(KeyCode::Numpad2);
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

    // Tab switches active field
    if keyboard.just_pressed(KeyCode::Tab) {
        input.active_field = 1 - input.active_field;
        keyboard.clear_just_pressed(KeyCode::Tab);
        return;
    }

    // Enter submits
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        let a = input.buffers[0].trim().to_string();
        let b = input.buffers[1].trim().to_string();

        match input.coord_type {
            GotoCoordType::WorldUnits => {
                if let (Ok(x), Ok(y)) = (a.parse::<f64>(), b.parse::<f64>()) {
                    action_writer.write(DevConsoleAction::GotoWorldUnits(x, y));
                    // Navigate back to admin menu, clearing goto entries from history
                    console.goto_input = None;
                    console.current_menu = MenuPath::Admin;
                    console.history.retain(|p| !matches!(p, MenuPath::Admin | MenuPath::GotoSelect | MenuPath::GotoInput));
                } else {
                    info!("Goto: invalid world unit coordinates");
                }
            }
            GotoCoordType::QR => {
                if let (Ok(q), Ok(r)) = (a.parse::<i32>(), b.parse::<i32>()) {
                    action_writer.write(DevConsoleAction::GotoQR(q, r));
                    console.goto_input = None;
                    console.current_menu = MenuPath::Admin;
                    console.history.retain(|p| !matches!(p, MenuPath::Admin | MenuPath::GotoSelect | MenuPath::GotoInput));
                } else {
                    info!("Goto: invalid QR coordinates");
                }
            }
        }

        keyboard.clear_just_pressed(KeyCode::Enter);
        keyboard.clear_just_pressed(KeyCode::NumpadEnter);
        return;
    }

    // Digit input (keyboard row and numpad)
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
            // Minus only valid at start of buffer
            if ch == '-' && !input.buffers[input.active_field].is_empty() {
                keyboard.clear_just_pressed(key);
                continue;
            }
            // Period only valid once per buffer (and only for world units)
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

    // Backspace
    if keyboard.just_pressed(KeyCode::Backspace) {
        input.buffers[input.active_field].pop();
        keyboard.clear_just_pressed(KeyCode::Backspace);
    }
}
