use bevy::prelude::*;
use crate::client::systems::{action_bar, character_panel, combat_ui, debug_resources, resource_bars, target_frame, target_indicator, threat_icons, ui};

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
        app.init_resource::<target_frame::LockedTarget>();

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
            ),
        );

        // HUD update systems (registered individually due to complex query types)
        app.add_systems(Update, ui::update);
        app.add_systems(Update, resource_bars::update);
        app.add_systems(Update, action_bar::update);
        app.add_systems(Update, target_frame::update);
        app.add_systems(Update, target_frame::update_queue);
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
                debug_resources::debug_drain_resources, // DEBUG: Remove after testing
                debug_resources::debug_process_expired_threats, // DEBUG: Remove after server integration
            ),
        );

        // Combat UI feedback systems (floating damage numbers, health bars)
        app.add_systems(
            Update,
            (
                combat_ui::update_floating_text,
                combat_ui::spawn_health_bars,
                combat_ui::update_health_bars,
            ),
        );
    }
}
