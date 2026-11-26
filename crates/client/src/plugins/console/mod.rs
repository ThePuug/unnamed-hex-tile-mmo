#[cfg(feature = "admin")]
mod actions;
#[cfg(feature = "admin")]
mod navigation;
mod state;
#[cfg(feature = "admin")]
mod ui;

use bevy::prelude::*;

pub use state::{DevConsole, MenuPath};

/// The dev console: menus the numpad walks, drawn over the game, in
/// admin builds only. It is not the admin console, the program of its
/// own (`crates/console`) the server's settings are changed from. Every build holds its state (`DevConsole`), which
/// the keys and panels read to stand aside while it is open.
///
/// NumpadDivide opens and closes it, except over an open panel
/// (`focus::NumpadFocus`), which has the numpad. A digit picks the row it numbers. Numpad0
/// goes back a menu, or closes the console from the main menu; where the
/// digits type instead — the world clock's hour and the goto coordinates —
/// Escape goes back.
///
/// The main menu groups by what is acted on. World (1): the world's
/// clock, for every player — moved to an hour typed as HHMM, scrubbed by
/// the left and right arrows, its day, week or season stepped by the up
/// and down ones, held or run by Space — and the rollback. Travel (2):
/// goto by world units or q,r, and the view. Stage (3): a den, a party
/// or an opposition ahead of the actor seen as. Client (4): what
/// this client alone draws and sends: the camera close-up, the weather
/// held in each pattern in turn or the forecast, and added latency.
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
                (actions::send_view, actions::send_goto, actions::send_spawn_party, actions::send_calendar, actions::send_rollback).after(actions::execute_console_actions),
            );
        }
    }
}
