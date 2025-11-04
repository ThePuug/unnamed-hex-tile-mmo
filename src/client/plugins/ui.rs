use bevy::prelude::*;
use crate::client::systems::{action_bar, character_panel, combat_ui, resource_bars, target_frame, target_indicator, threat_icons, ui};

/// Plugin that handles game UI elements
///
/// This plugin provides:
/// - Character panel (C key) for viewing and adjusting attributes
/// - HUD elements (time display, etc.)
/// - Target indicator (red hex showing which entity will be targeted)
/// - Threat icons (circular display around player showing queued threats)
/// - Combat feedback (floating damage numbers, health bars)
/// - Other game UI elements as they are added
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Initialize UI resources
        app.init_resource::<character_panel::CharacterPanelState>();
        app.init_resource::<target_frame::AllyTarget>();

        // Setup systems run once at startup
        app.add_systems(
            Startup,
            (
                ui::setup.after(crate::client::systems::camera::setup),
                character_panel::setup,
                resource_bars::setup.after(crate::client::systems::camera::setup),
                action_bar::setup.after(crate::client::systems::camera::setup),
                threat_icons::setup.after(crate::client::systems::camera::setup),
                target_frame::setup.after(crate::client::systems::camera::setup),
                target_indicator::setup,
                combat_ui::setup_health_bars.after(crate::client::systems::camera::setup),
            ),
        );

        // HUD update systems (registered individually due to complex query types)
        app.add_systems(Update, ui::update);
        app.add_systems(Update, resource_bars::update);
        app.add_systems(Update, action_bar::update);
        app.add_systems(Update, target_frame::update);
        app.add_systems(Update, target_frame::update_ally_frame);
        app.add_systems(Update, target_frame::update_queue);
        app.add_systems(Update, target_frame::update_ally_queue);
        app.add_systems(Update, target_indicator::update);

        // Character panel systems
        app.add_systems(
            Update,
            (
                character_panel::toggle_panel,
                character_panel::handle_shift_drag,
                character_panel::update_attributes,
            ),
        );

        // Threat icon systems can run in parallel - no ordering needed
        app.add_systems(
            Update,
            (
                threat_icons::update,
                threat_icons::animate_clear,
            ),
        );

        // Combat UI feedback systems (floating damage numbers, health bars, threat dots)
        app.add_systems(
            Update,
            (
                combat_ui::update_floating_text,
                combat_ui::update_health_bars,
                combat_ui::update_threat_queue_dots, // Threat queue capacity dots above health bars
            ),
        );
    }
}
