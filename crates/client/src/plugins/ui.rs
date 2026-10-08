use bevy::prelude::*;
use crate::systems::{action_bar, bag_panel, character_panel, character_panel_respec, closeup, combat_log, combat_ui, drop_panel, equipment_panel, highway, resolved_threats, resource_bars, target_frame, target_indicator, ui};

/// Plugin that handles game UI elements

/// This plugin provides:
/// - Character panel (C key) for viewing and adjusting attributes
/// - HUD elements (time display, etc.)
/// - Target indicator (red hex showing which entity will be targeted)
/// - The highway (the player's reaction queue as rhythm-game lanes)
/// - Combat feedback (floating damage numbers, health bars)
/// - Other game UI elements as they are added
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Initialize UI resources
        app.init_resource::<character_panel::CharacterPanelState>();
        app.init_resource::<closeup::Turn>();
        app.init_resource::<bag_panel::BagCursor>();
        app.init_resource::<drop_panel::DropChoice>();

        // Setup systems run once at startup
        app.add_systems(
            Startup,
            (
                ui::setup.after(crate::systems::camera::setup),
                closeup::setup,
                character_panel::setup,
                resource_bars::setup.after(crate::systems::camera::setup),
                action_bar::setup.after(crate::systems::camera::setup),
                highway::setup.after(crate::systems::camera::setup),
                target_frame::setup.after(crate::systems::camera::setup),
                target_indicator::setup,
                combat_ui::setup_health_bars.after(crate::systems::camera::setup),
                combat_log::setup.after(crate::systems::camera::setup),
            ),
        );

        // HUD update systems (registered individually due to complex query types)
        app.add_systems(Update, ui::update);
        app.add_systems(Update, ui::update_compass);  // Compass rotation
        app.add_systems(Update, ui::scale_to_window);
        app.add_systems(Update, resource_bars::update);
        app.add_systems(Update, ((action_bar::sync_loadout, action_bar::update).chain(), action_bar::update_dismiss));
        app.add_systems(Update, target_frame::update);
        app.add_systems(Update, target_indicator::update);

        // Character panel systems
        app.add_systems(
            Update,
            (
                character_panel::toggle_panel,
                character_panel::update_tabs,
                equipment_panel::handle_numpad,
                equipment_panel::rebuild_bag,
                bag_panel::update,
                equipment_panel::update_bag,
                equipment_panel::update_slots,
                closeup::show,
                closeup::sync_figure,
                closeup::stage_layers,
                closeup::activate,
                closeup::turn,
                character_panel::update_attributes,
                character_panel::update_pair_cursor,
                character_panel::update_apply_label,
                character_panel_respec::handle_numpad,
                character_panel_respec::handle_respec_confirmed,
                character_panel_respec::toggle_apply_label,
            ),
        );

        // Dropping from the bag
        app.add_systems(Update, (bag_panel::handle_numpad, (drop_panel::handle_keys, drop_panel::update).chain()));

        app.add_plugins(UiMaterialPlugin::<highway::HighwayMaterial>::default());
        app.init_resource::<highway::Landings>();
        app.add_systems(Update, (highway::record_landings, highway::update, highway::update_pulses, highway::update_shards));

        // Combat UI feedback systems (floating damage numbers, health bars, recovery bars, threat dots)
        app.add_systems(
            Update,
            (
                combat_ui::update_floating_text,
                combat_ui::update_world_bars,
            ),
        );

        // Combat feedback enhancements (resolved threats + combat log)
        app.add_systems(
            Update,
            (
                resolved_threats::on_damage_resolved,
                resolved_threats::update_entries,
                combat_log::on_damage_applied,
                combat_log::on_queue_cleared,
                combat_log::maintain_log,
                combat_log::handle_scroll,
                combat_log::auto_scroll_to_bottom,
            ),
        );
    }
}
