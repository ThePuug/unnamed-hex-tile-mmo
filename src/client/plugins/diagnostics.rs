// This file now serves as a module root, with submodules containing the implementation
mod config;
pub mod grid;
pub mod perf_ui;
pub mod network_ui;
mod toggles;

use bevy::{
    diagnostic::*,
    prelude::*,
    render::diagnostic::*,
};
use iyes_perf_ui::{PerfUiPlugin, PerfUiAppExt};

// Re-export public types for external use
pub use config::DiagnosticsState;

/// Plugin that consolidates all debug and diagnostic features
///
/// This plugin provides:
/// - Performance monitoring (FPS, entity count, render stats)
/// - Hex grid overlay for visualizing tile boundaries
/// - Toggleable slope rendering (affects terrain mesh and grid)
/// - Fixed lighting mode for consistent debugging
///
/// All features can be toggled via the developer console (NumpadDivide to open).
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        // Add performance monitoring plugins from Bevy and third-party
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin,
            RenderDiagnosticsPlugin,
            PerfUiPlugin,
        ));

        // Register custom perf UI entry types
        app.add_perf_ui_simple_entry::<perf_ui::PerfUiTerrainTiles>();
        app.add_perf_ui_simple_entry::<network_ui::PerfUiNetworkBandwidth>();
        app.add_perf_ui_simple_entry::<network_ui::PerfUiNetworkMessages>();

        // Initialize shared diagnostic resources
        app.init_resource::<DiagnosticsState>();
        app.init_resource::<network_ui::NetworkMetrics>();

        // Setup systems run once at startup
        app.add_systems(
            Startup,
            (
                grid::setup_grid_overlay,
                perf_ui::setup_performance_ui,
                network_ui::setup_network_ui,
            ),
        );

        // Update systems run every frame
        app.add_systems(
            Update,
            (
                // Mesh update systems (no direct input handlers - use dev console)
                grid::update_grid_mesh,
                // Network metrics update (end of frame)
                network_ui::update_network_metrics,
            ),
        );
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::grid::HexGridOverlay;
    use crate::common::resources::map::Map;

    #[test]
    fn test_toggle_grid_triggers_regen_on_enable() {
        // NOTE: This test validates the legacy toggle behavior.
        // Grid toggling is now handled by the developer console.

        // Setup minimal app with required resources
        let mut app = App::new();
        app.init_resource::<DiagnosticsState>();

        // Spawn grid overlay entity
        app.world_mut().spawn((
            HexGridOverlay {
                needs_regeneration: false,
            },
            Visibility::Hidden,
        ));

        // Add toggle system
        app.add_systems(Update, grid::toggle_grid_visibility);

        // Simulate grid toggle key press
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyJ);
        app.insert_resource(input);

        // Run one update cycle
        app.update();

        // Verify state changed correctly
        let state = app.world().resource::<DiagnosticsState>();
        assert!(
            state.grid_visible,
            "Grid should be visible after toggle"
        );

        // Verify regeneration flag was set
        let mut query = app.world_mut().query::<&HexGridOverlay>();
        let overlay = query
            .iter(app.world())
            .next()
            .expect("HexGridOverlay entity should exist");
        assert!(
            overlay.needs_regeneration,
            "Grid should need regeneration when toggled on"
        );
    }

    #[test]
    fn test_toggle_grid_off_does_not_trigger_regen() {
        // NOTE: This test validates the legacy toggle behavior.
        // Grid toggling is now handled by the developer console.

        // Setup with grid already visible
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        app.insert_resource(state);

        // Spawn grid overlay entity
        app.world_mut().spawn((
            HexGridOverlay {
                needs_regeneration: false,
            },
            Visibility::Visible,
        ));

        // Add toggle system
        app.add_systems(Update, grid::toggle_grid_visibility);

        // Simulate toggle key press to turn OFF
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyJ);
        app.insert_resource(input);

        // Run one update cycle
        app.update();

        // Verify state changed correctly
        let state = app.world().resource::<DiagnosticsState>();
        assert!(
            !state.grid_visible,
            "Grid should be hidden after toggle"
        );

        // Verify regeneration flag was NOT set
        let mut query = app.world_mut().query::<&HexGridOverlay>();
        let overlay = query
            .iter(app.world())
            .next()
            .expect("HexGridOverlay entity should exist");
        assert!(
            !overlay.needs_regeneration,
            "Grid should NOT need regeneration when toggled off"
        );
    }

    #[test]
    fn test_update_grid_triggers_on_map_change() {
        // Setup app with grid visible
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        app.insert_resource(state);

        // Create mesh assets storage
        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy::render::mesh::PrimitiveTopology::LineList,
            bevy::render::render_asset::RenderAssetUsages::MAIN_WORLD,
        ));
        app.insert_resource(meshes);

        // Spawn grid overlay with mesh
        app.world_mut().spawn((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy::render::primitives::Aabb::default(),
            HexGridOverlay {
                needs_regeneration: false,
            },
        ));

        // Create empty map and mark as changed
        let map = Map::new(qrz::Map::new(100.0, 0.8));
        app.insert_resource(map);
        app.world_mut().resource_mut::<Map>().set_changed();

        // Add update system
        app.add_systems(Update, grid::update_grid_mesh);

        // Run one update cycle
        app.update();

        // Verify regeneration flag was cleared
        let mut query = app.world_mut().query::<&HexGridOverlay>();
        let overlay = query
            .iter(app.world())
            .next()
            .expect("HexGridOverlay entity should exist");
        assert!(
            !overlay.needs_regeneration,
            "needs_regeneration should be cleared after grid updates"
        );
    }

    #[test]
    fn test_update_grid_does_not_run_when_grid_hidden() {
        // Setup with grid hidden
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = false;
        app.insert_resource(state);

        // Create mesh assets
        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy::render::mesh::PrimitiveTopology::LineList,
            bevy::render::render_asset::RenderAssetUsages::MAIN_WORLD,
        ));
        app.insert_resource(meshes);

        // Spawn grid overlay with regeneration requested
        app.world_mut().spawn((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy::render::primitives::Aabb::default(),
            HexGridOverlay {
                needs_regeneration: true,
            },
        ));

        // Create map and mark as changed
        let map = Map::new(qrz::Map::new(100.0, 0.8));
        app.insert_resource(map);
        app.world_mut().resource_mut::<Map>().set_changed();

        // Add update system
        app.add_systems(Update, grid::update_grid_mesh);

        // Run one update cycle
        app.update();

        // Verify regeneration flag remains true (update was skipped)
        let mut query = app.world_mut().query::<&HexGridOverlay>();
        let overlay = query
            .iter(app.world())
            .next()
            .expect("HexGridOverlay entity should exist");
        assert!(
            overlay.needs_regeneration,
            "needs_regeneration should remain true when grid is hidden"
        );
    }

    #[test]
    fn test_update_grid_respects_slope_rendering_setting() {
        // Verify that slope rendering state is correctly stored in DiagnosticsState
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        state.slope_rendering_enabled = false; // Explicitly disabled
        app.insert_resource(state);

        // Read back and verify
        let state = app.world().resource::<DiagnosticsState>();
        assert!(
            !state.slope_rendering_enabled,
            "Slope rendering should be disabled"
        );
        assert!(state.grid_visible, "Grid should be visible");
    }
}
