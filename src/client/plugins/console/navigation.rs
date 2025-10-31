use bevy::prelude::*;

use super::{
    state::{DevConsole, MenuPath},
    actions::DevConsoleAction,
};

/// System that handles numpad input for console navigation
pub fn handle_console_input(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut console: ResMut<DevConsole>,
    mut action_writer: EventWriter<DevConsoleAction>,
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

    // Handle Numpad0 (Back/Close)
    if keyboard.just_pressed(KeyCode::Numpad0) {
        if console.current_menu == MenuPath::Root {
            // Close console from root
            console.visible = false;
        } else {
            // Go back to previous menu
            console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        }

        // Consume the input to prevent it from reaching gameplay systems
        keyboard.clear_just_pressed(KeyCode::Numpad0);
        return;
    }

    // Handle menu-specific inputs
    match console.current_menu {
        MenuPath::Root => handle_root_menu(&mut keyboard, &mut console),
        MenuPath::Terrain => handle_terrain_menu(&mut keyboard, &mut action_writer),
        MenuPath::Combat => handle_combat_menu(&mut keyboard, &mut action_writer),
        MenuPath::Performance => handle_performance_menu(&mut keyboard, &mut action_writer),
        MenuPath::Tools => handle_tools_menu(&mut keyboard, &mut action_writer),
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
        console.current_menu = MenuPath::Combat;
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Performance;
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        console.history.push(console.current_menu.clone());
        console.current_menu = MenuPath::Tools;
        consumed = Some(KeyCode::Numpad4);
    }

    // Consume the input
    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_terrain_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut EventWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.send(DevConsoleAction::ToggleGrid);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.send(DevConsoleAction::ToggleSlopeRendering);
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.send(DevConsoleAction::ToggleFixedLighting);
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        action_writer.send(DevConsoleAction::RegenerateMesh);
        consumed = Some(KeyCode::Numpad4);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_combat_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut EventWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.send(DevConsoleAction::QueueDamageThreat);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.send(DevConsoleAction::DrainStamina);
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.send(DevConsoleAction::DrainMana);
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        action_writer.send(DevConsoleAction::ClearReactionQueue);
        consumed = Some(KeyCode::Numpad4);
    } else if keyboard.just_pressed(KeyCode::Numpad5) {
        action_writer.send(DevConsoleAction::RefillResources);
        consumed = Some(KeyCode::Numpad5);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_performance_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut EventWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.send(DevConsoleAction::TogglePerfUI);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.send(DevConsoleAction::ToggleFPSCounter);
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.send(DevConsoleAction::ToggleDetailedStats);
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        action_writer.send(DevConsoleAction::LogFrameReport);
        consumed = Some(KeyCode::Numpad4);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}

fn handle_tools_menu(
    keyboard: &mut ButtonInput<KeyCode>,
    action_writer: &mut EventWriter<DevConsoleAction>,
) {
    let mut consumed = None;

    if keyboard.just_pressed(KeyCode::Numpad1) {
        action_writer.send(DevConsoleAction::TeleportToCursor);
        consumed = Some(KeyCode::Numpad1);
    } else if keyboard.just_pressed(KeyCode::Numpad2) {
        action_writer.send(DevConsoleAction::SpawnNPCAtCursor);
        consumed = Some(KeyCode::Numpad2);
    } else if keyboard.just_pressed(KeyCode::Numpad3) {
        action_writer.send(DevConsoleAction::ClearAllEntities);
        consumed = Some(KeyCode::Numpad3);
    } else if keyboard.just_pressed(KeyCode::Numpad4) {
        action_writer.send(DevConsoleAction::PlaceTestSpawner);
        consumed = Some(KeyCode::Numpad4);
    }

    if let Some(key) = consumed {
        keyboard.clear_just_pressed(key);
    }
}
