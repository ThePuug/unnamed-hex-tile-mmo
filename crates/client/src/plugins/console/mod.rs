mod actions;
mod navigation;
mod state;
mod ui;

use bevy::prelude::*;

// Re-export public types
pub use state::{DevConsole, MenuPath};
pub use actions::DevConsoleAction;

/// The developer console: menus the numpad walks, drawn over the game.
///
/// NumpadDivide opens and closes it, except over the open character
/// panel, which has the numpad. A digit picks the row it numbers. Numpad0
/// goes back a menu, or closes the console from the main menu; where the
/// digits type instead — the lighting hour and the goto coordinates —
/// Escape goes back.
///
/// Terrain (1): the grid overlay; the lighting clock, held at an hour
/// typed as HHMM, scrubbed by the left and right arrows and its day, week
/// or season stepped by the up and down ones; the camera envelope; the
/// terrain and the cover hidden; the camera close-up; and the canopy drawn
/// from its parts or its vertices. Admin builds add goto by world units or
/// q,r (2), added latency (3), a den spawned (5), the view (6), and a
/// party or an opposition staged (7, 8).
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
        #[cfg(feature = "admin")]
        app.add_systems(Update, actions::send_view.after(actions::execute_console_actions));
        #[cfg(feature = "admin")]
        app.add_systems(Update, actions::send_goto.after(actions::execute_console_actions));
        #[cfg(feature = "admin")]
        app.add_systems(Update, actions::send_spawn_party.after(actions::execute_console_actions));
    }
}
