//! The grid, the lighting clock and the milestones in every build; in
//! admin builds, the metrics too, published for the console to read
//! (`publish`). A player's build measures nothing for no one.

#[cfg(feature = "admin")]
pub mod census;
mod config;
#[cfg(all(feature = "admin", debug_assertions))]
mod heap;
pub mod grid;
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

/// What the metrics are measured from, and their publication.
#[cfg(feature = "admin")]
fn metrics(app: &mut App) {
    use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};

    app.add_plugins((FrameTimeDiagnosticsPlugin::default(), EntityCountDiagnosticsPlugin::default()));
    // trace_tracy auto-registers RenderDiagnosticsPlugin; skip when active.
    #[cfg(not(feature = "trace"))]
    app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    time_render_phases(app);

    app.init_resource::<RenderCensus>();
    app.init_resource::<publish::Publication>();
    app.add_systems(Update, (census::take_census, publish::publish).chain());
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
