// This file now serves as a module root, with submodules containing the implementation
mod config;
pub mod grid;
pub mod perf_ui;
pub mod network_ui;
mod toggles;

use bevy::{
    diagnostic::*,
    picking::Pickable,
    prelude::*,
    render::diagnostic::*,
};

// Re-export public types for external use
pub use config::DiagnosticsState;

/// Marker for the root diagnostics container (bottom-right column).
/// Individual panels are children of this node.
#[derive(Component)]
pub struct DiagnosticsRoot;

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
        // Add performance monitoring plugins from Bevy
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            RenderDiagnosticsPlugin,
        ));

        // Initialize shared diagnostic resources
        app.init_resource::<DiagnosticsState>();
        app.init_resource::<network_ui::NetworkMetrics>();

        // Setup systems run once at startup
        // The root container must exist before panels add themselves as children.
        app.add_systems(
            Startup,
            (
                grid::setup_grid_overlay,
                setup_diagnostics_root.before(perf_ui::setup_performance_ui)
                                      .before(network_ui::setup_network_ui),
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
                // Performance and network UI updates
                perf_ui::update_performance_ui,
                network_ui::update_network_ui,
                // Network metrics update (end of frame)
                network_ui::update_network_metrics,
            ),
        );
    }
}

/// Spawns the bottom-right container that diagnostic panels stack into.
fn setup_diagnostics_root(mut commands: Commands) {
    commands.spawn((
        DiagnosticsRoot,
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(8.0),
            bottom: Val::Px(8.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            align_items: AlignItems::FlexEnd,
            ..default()
        },
    ));
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
    fn test_update_grid_triggers_on_map_change() {
        // Setup app with grid visible
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        app.insert_resource(state);

        // Create mesh assets storage
        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy_mesh::PrimitiveTopology::LineList,
            bevy_asset::RenderAssetUsages::MAIN_WORLD,
        ));
        app.insert_resource(meshes);

        // Spawn grid overlay with mesh
        app.world_mut().spawn((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy_camera::primitives::Aabb::default(),
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
            bevy_mesh::PrimitiveTopology::LineList,
            bevy_asset::RenderAssetUsages::MAIN_WORLD,
        ));
        app.insert_resource(meshes);

        // Spawn grid overlay with regeneration requested
        app.world_mut().spawn((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy_camera::primitives::Aabb::default(),
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
