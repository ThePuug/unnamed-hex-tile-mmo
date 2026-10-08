//! The grid, the lighting clock and the milestones in every build; in
//! admin builds, the metrics too. Publishing them joins a multicast group
//! the overlay reads back, and listening on a port prompts a player's
//! firewall, so a player's build has no metrics at all.

#[cfg(feature = "admin")]
pub mod census;
mod config;
#[cfg(feature = "admin")]
mod feed;
#[cfg(all(feature = "admin", debug_assertions))]
mod heap;
pub mod grid;
#[cfg(feature = "admin")]
pub mod metrics_overlay;
mod milestones;
pub mod network_ui;
#[cfg(feature = "admin")]
mod publish;

use bevy::{prelude::*, render::RenderApp};

#[cfg(feature = "admin")]
pub use census::RenderCensus;
pub use config::{DateField, DiagnosticsState, LightingClock};

pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        // The render app's systems time themselves too, from a clone.
        app.init_resource::<crate::resources::ClientTimers>();
        let timers = app.world().resource::<crate::resources::ClientTimers>().clone();
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(timers);
        }

        app.init_resource::<DiagnosticsState>();
        app.add_systems(PostStartup, milestones::log_started);
        app.add_systems(Update, (milestones::log_stage, milestones::log_video));
        app.init_resource::<network_ui::NetworkMetrics>();
        app.init_resource::<grid::PendingGridMesh>();
        app.add_systems(Startup, grid::setup_grid_overlay);
        app.add_systems(
            Update,
            (
                grid::spawn_grid_mesh_task,
                grid::poll_grid_mesh_task,
                network_ui::update_network_metrics,
            ),
        );

        #[cfg(feature = "admin")]
        metrics(app);
    }
}

/// What the metrics are measured from, their publication, and the overlay
/// that reads them back.
#[cfg(feature = "admin")]
fn metrics(app: &mut App) {
    use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};

    app.add_plugins((FrameTimeDiagnosticsPlugin::default(), EntityCountDiagnosticsPlugin::default(), bevy_egui::EguiPlugin::default()));
    // trace_tracy auto-registers RenderDiagnosticsPlugin; skip when active.
    #[cfg(not(feature = "trace"))]
    app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    time_render_phases(app);

    app.init_resource::<RenderCensus>();
    app.init_resource::<publish::Publication>();
    app.init_resource::<feed::Feed>();
    app.add_systems(Update, (census::take_census, publish::publish, feed::receive).chain());

    app.add_systems(
        Startup,
        (
            metrics_overlay::setup_overlay_camera,
            metrics_overlay::setup_overlay_font.after(metrics_overlay::setup_overlay_camera),
        ),
    );
    // Egui builds its UI inside its context's own pass, never in
    // Update: a pass run outside it leaves an output nothing applies,
    // and the textures it made are dropped with it.
    app.add_systems(bevy_egui::EguiPrimaryContextPass, metrics_overlay::update_metrics_overlay);
}

/// When the render thread took up this frame, and when it last passed
/// one of the schedule's phases.
#[cfg(feature = "admin")]
#[derive(Resource)]
struct RenderStart(std::time::Instant);
#[cfg(feature = "admin")]
#[derive(Resource)]
struct RenderMark(std::time::Instant);

/// The render app brackets its whole schedule as `render`: the frame is
/// that thread's work or the main thread's, and which one it is decides
/// where to look. Within it, each phase of the schedule is timed from the
/// end of the one before, as `render_<phase>`. Presenting the frame falls
/// inside the render phase, so under vsync that phase holds the wait for
/// the display.
#[cfg(feature = "admin")]
fn time_render_phases(app: &mut App) {
    use bevy::render::{Render, RenderSystems as S};
    use std::time::Instant;
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app.insert_resource(RenderStart(Instant::now()));
    render_app.insert_resource(RenderMark(Instant::now()));
    render_app.add_systems(
        Render,
        (
            (|mut start: ResMut<RenderStart>, mut mark: ResMut<RenderMark>| {
                start.0 = Instant::now();
                mark.0 = start.0;
            })
            .before(S::ExtractCommands),
            (|start: Res<RenderStart>, timers: Res<crate::resources::ClientTimers>| {
                timers.record("render", start.0.elapsed().as_secs_f32() * 1000.0);
            })
            .after(S::PostCleanup),
        ),
    );
    // The schedule's phases run as one chain; a mark between each pair.
    let phases: [(S, S, &'static str); 10] = [
        (S::ExtractCommands, S::PrepareMeshes, "render_extract_commands"),
        (S::PrepareMeshes, S::CreateViews, "render_prepare_meshes"),
        (S::CreateViews, S::Specialize, "render_create_views"),
        (S::Specialize, S::PrepareViews, "render_specialize"),
        (S::PrepareViews, S::Queue, "render_prepare_views"),
        (S::Queue, S::PhaseSort, "render_queue"),
        (S::PhaseSort, S::Prepare, "render_phase_sort"),
        (S::Prepare, S::Render, "render_prepare"),
        (S::Render, S::Cleanup, "render_render"),
        (S::Cleanup, S::PostCleanup, "render_cleanup"),
    ];
    for (done, next, name) in phases {
        render_app.add_systems(
            Render,
            (move |mut mark: ResMut<RenderMark>, timers: Res<crate::resources::ClientTimers>| {
                let now = Instant::now();
                timers.record(name, (now - mark.0).as_secs_f32() * 1000.0);
                mark.0 = now;
            })
            .after(done)
            .before(next),
        );
    }
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
