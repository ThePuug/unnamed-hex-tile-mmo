mod actions;
mod navigation;
mod state;
mod ui_simple;

use ui_simple as ui;

use bevy::prelude::*;

// Re-export public types
pub use state::DevConsole;
pub use actions::DevConsoleAction;

/// Plugin that provides a hierarchical, numpad-navigable developer console
///
/// This plugin consolidates all debug capabilities into a contextual menu system:
/// - Performance monitoring (FPS, stats, profiling)
/// - Terrain debugging (grid, slopes, lighting)
/// - Combat testing (resource drains, reaction queue)
/// - Visualization toggles (spawner markers, future debug viz)
/// - Developer tools (future: teleport, spawn NPCs)
///
/// Navigation:
/// - **NumpadDivide (/)**: Open/close console
/// - **Numpad 0-9**: Select menu options
/// - **Numpad 0**: Back to previous menu / Close from root
///
/// The console provides an alternative to scattered keybindings (J/H/G/V/F3/Digit1-3),
/// making debug features discoverable and organized.
pub struct DevConsolePlugin;

impl Plugin for DevConsolePlugin {
    fn build(&self, app: &mut App) {
        // Register console resources
        app.init_resource::<DevConsole>();

        // Register console events
        app.add_message::<DevConsoleAction>();

        // Setup systems (run once at startup)
        app.add_systems(Startup, ui::setup_dev_console);

        // Update systems (run every frame)
        app.add_systems(
            Update,
            (
                // Input handling
                navigation::handle_console_input,
                // UI updates
                ui::update_console_visibility,
                ui::update_console_menu,
                // Action execution
                actions::execute_console_actions,
            )
                .chain(), // Run in sequence
        );
    }
}
