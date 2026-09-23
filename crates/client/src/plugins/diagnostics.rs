pub mod census;
mod config;
pub mod dump;
pub mod grid;
pub mod metrics_overlay;
pub mod network_ui;

use std::time::Instant;

use bevy::{
    diagnostic::*,
    prelude::*,
    render::{Render, RenderApp, RenderSystems},
};
#[cfg(not(feature = "trace"))]
use bevy::render::diagnostic::*;
use bevy_egui::EguiPlugin;

pub use census::RenderCensus;
pub use dump::MetricsDump;
pub use config::{DateField, DiagnosticsState, LightingClock, Samples, Shadows};

pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            EguiPlugin::default(),
        ));
        // trace_tracy auto-registers RenderDiagnosticsPlugin; skip when active.
        #[cfg(not(feature = "trace"))]
        app.add_plugins(RenderDiagnosticsPlugin);

        app.init_resource::<crate::resources::ClientTimers>();
        share_timers(app);

        app.init_resource::<DiagnosticsState>();
        app.init_resource::<RenderCensus>();
        app.insert_resource(MetricsDump::from_args());
        app.add_systems(Update, (census::take_census, dump::dump_metrics).chain());
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
            ),
        );

        // Egui builds its UI inside its context's own pass, never in
        // Update: a pass run outside it leaves an output nothing applies,
        // and the textures it made are dropped with it.
        app.add_systems(
            bevy_egui::EguiPrimaryContextPass,
            metrics_overlay::update_metrics_overlay,
        );
    }
}

/// When the render thread took up this frame.
#[derive(Resource)]
struct RenderStart(Instant);

/// The render app records into the same timers as the main world, and
/// brackets its whole schedule as `render`: the frame is that thread's
/// work or the main thread's, and which one it is decides where to look.
fn share_timers(app: &mut App) {
    let timers = app.world().resource::<crate::resources::ClientTimers>().clone();
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app.insert_resource(timers);
    render_app.insert_resource(RenderStart(Instant::now()));
    render_app.add_systems(
        Render,
        (
            (|mut start: ResMut<RenderStart>| start.0 = Instant::now()).before(RenderSystems::ExtractCommands),
            (|start: Res<RenderStart>, timers: Res<crate::resources::ClientTimers>| {
                timers.0.record("render", start.0.elapsed().as_secs_f32() * 1000.0);
            })
            .after(RenderSystems::PostCleanup),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::grid::HexGridOverlay;
    use crate::resources::SummaryMeshes;

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
                needs_regeneration: true,
            },
        ));

        app.insert_resource(SummaryMeshes::default());
        app.world_mut().resource_mut::<SummaryMeshes>().set_changed();

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

        app.init_resource::<SummaryMeshes>();

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
