mod config;
pub mod grid;
pub mod metrics_overlay;
pub mod network_ui;

use bevy::{
    diagnostic::*,
    prelude::*,
    render::diagnostic::*,
};
use bevy_egui::EguiPlugin;

pub use config::DiagnosticsState;

pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            RenderDiagnosticsPlugin,
            EguiPlugin::default(),
        ));

        app.init_resource::<DiagnosticsState>();
        app.init_resource::<network_ui::NetworkMetrics>();
        app.init_resource::<grid::PendingGridMesh>();
        app.init_resource::<metrics_overlay::MetricsHistory>();

        app.add_systems(
            Startup,
            (
                grid::setup_grid_overlay,
                metrics_overlay::setup_overlay_camera,
                metrics_overlay::setup_overlay_font
                    .after(metrics_overlay::setup_overlay_camera),
            ),
        );

        app.add_systems(
            Update,
            (
                grid::spawn_grid_mesh_task,
                grid::poll_grid_mesh_task,
                network_ui::update_network_metrics,
                metrics_overlay::sample_metrics,
                metrics_overlay::update_metrics_overlay,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::grid::HexGridOverlay;
    use common_bevy::resources::map::Map;

    #[test]
    fn test_update_grid_triggers_on_map_change() {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(|| {
            bevy::tasks::TaskPool::new()
        });

        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = true;
        app.insert_resource(state);

        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy_mesh::PrimitiveTopology::LineList,
            bevy_asset::RenderAssetUsages::MAIN_WORLD,
        ));
        app.insert_resource(meshes);

        app.world_mut().spawn((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy_camera::primitives::Aabb::default(),
            HexGridOverlay {
                needs_regeneration: false,
            },
        ));

        let map = Map::new(qrz::Map::new(100.0, 0.8, qrz::HexOrientation::FlatTop));
        app.insert_resource(map);
        app.world_mut().resource_mut::<Map>().set_changed();

        app.insert_resource(grid::PendingGridMesh::default());
        app.add_systems(Update, grid::spawn_grid_mesh_task);

        app.update();

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
        let mut app = App::new();
        let mut state = DiagnosticsState::default();
        state.grid_visible = false;
        app.insert_resource(state);

        let mut meshes = Assets::<Mesh>::default();
        let mesh_handle = meshes.add(Mesh::new(
            bevy_mesh::PrimitiveTopology::LineList,
            bevy_asset::RenderAssetUsages::MAIN_WORLD,
        ));
        app.insert_resource(meshes);

        app.world_mut().spawn((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy_camera::primitives::Aabb::default(),
            HexGridOverlay {
                needs_regeneration: true,
            },
        ));

        let map = Map::new(qrz::Map::new(100.0, 0.8, qrz::HexOrientation::FlatTop));
        app.insert_resource(map);
        app.world_mut().resource_mut::<Map>().set_changed();

        app.insert_resource(grid::PendingGridMesh::default());
        app.add_systems(Update, grid::spawn_grid_mesh_task);

        app.update();

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

}
