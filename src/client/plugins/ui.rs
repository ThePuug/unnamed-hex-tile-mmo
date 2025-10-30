use bevy::prelude::*;
use crate::client::systems::{character_panel, debug_resources, resource_bars, target_cursor, ui};

/// Plugin that handles game UI elements
///
/// This plugin provides:
/// - Character panel (C key) for viewing and adjusting attributes
/// - HUD elements (time display, etc.)
/// - Target cursor (red hex showing where player is looking)
/// - Other game UI elements as they are added
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Initialize UI resources
        app.init_resource::<character_panel::CharacterPanelState>();

        // Setup systems run once at startup
        app.add_systems(
            Startup,
            (
                ui::setup.after(crate::client::systems::camera::setup),
                character_panel::setup,
                resource_bars::setup.after(crate::client::systems::camera::setup),
                target_cursor::setup,
            ),
        );

        // Update systems run every frame
        app.add_systems(
            Update,
            (
                ui::update,
                character_panel::toggle_panel,
                character_panel::handle_shift_drag,
                character_panel::update_attributes,
                resource_bars::update,
                target_cursor::update,
                debug_resources::debug_drain_resources, // DEBUG: Remove after testing
                debug_resources::debug_process_expired_threats, // DEBUG: Remove after server integration
            ),
        );
    }
}
