#[cfg(feature = "admin")]
mod actions;
#[cfg(feature = "admin")]
mod navigation;
mod state;
#[cfg(feature = "admin")]
mod ui;

use bevy::prelude::*;

pub use state::{DevConsole, MenuPath};

/// The developer console: menus the numpad walks, drawn over the game, in
/// admin builds only. Every build holds its state (`DevConsole`), which
/// the keys and panels read to stand aside while it is open.
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
/// from its parts or its vertices. Goto by world units or q,r (2), added
/// latency (3), a den spawned (5), the view (6), and a party or an
/// opposition staged (7, 8).
pub struct DevConsolePlugin;

impl Plugin for DevConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DevConsole>();

        #[cfg(feature = "admin")]
        {
            app.add_message::<actions::DevConsoleAction>();
            app.add_systems(Startup, ui::setup_dev_console);
            app.add_systems(
                Update,
                (
                    navigation::handle_console_input,
                    ui::update_console_visibility,
                    ui::update_console_menu,
                    ui::refresh_date_line,
                    actions::execute_console_actions,
                )
                    .chain(),
            );
            app.add_systems(
                Update,
                (actions::send_view, actions::send_goto, actions::send_spawn_party).after(actions::execute_console_actions),
            );
        }
    }
}
