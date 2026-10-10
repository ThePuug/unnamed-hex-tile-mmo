//! The client's progress, logged as it happens so a run can be followed
//! from outside it: started, connected, in the world, playing, and every
//! change to the settings a measurement depends on.
//!
//! Each line starts with `milestone:`, so something driving the client
//! waits on the line it needs rather than for a length of time it
//! guessed. A step that never comes shows as a line that never appears.

use bevy::prelude::*;

use crate::plugins::settings::VideoSettings;
use crate::plugins::shell::{Loading, Stage};

/// Log that the app is up, so a wait on the next step starts its clock
/// here and not at a build that may take minutes.
pub fn log_started() {
    info!("milestone: started");
}

/// Log each change of stage. Entering `Playing` carries the terrain count
/// it entered with, since the loading screen also gives up after a limit:
/// fewer chunks than wanted, or builds still running, means it gave up.
pub fn log_stage(mut transitions: MessageReader<StateTransitionEvent<Stage>>, loading: Res<Loading>) {
    for transition in transitions.read() {
        let Some(entered) = transition.entered else { continue };
        if entered == Stage::Playing {
            let (have, want) = loading.chunks;
            info!(
                "milestone: stage {:?} -> {entered:?} ({have}/{want} chunks, {} builds running)",
                transition.exited,
                loading.building,
            );
        } else {
            info!("milestone: stage {:?} -> {entered:?}", transition.exited);
        }
    }
}

/// Log the window's size and scale factor and the UI's scale whenever one
/// of them changes, so a mode switch can be shown to keep the HUD's share
/// of the screen.
pub fn log_window(
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    ui: Res<UiScale>,
    mut last: Local<Option<(UVec2, f32, f32)>>,
) {
    let Ok(window) = window.single() else { return };
    let now = (window.resolution.physical_size(), window.resolution.scale_factor(), ui.0);
    if *last != Some(now) {
        *last = Some(now);
        info!("milestone: window {}x{} scale={} ui={:.3}", now.0.x, now.0.y, now.1, now.2);
    }
}

/// Log the video settings whenever they change, so a measurement taken
/// after a toggle can be shown to follow it.
pub fn log_video(video: Res<VideoSettings>) {
    if video.is_changed() {
        info!(
            "milestone: video display={} resolution={} msaa={} shadows={} vsync={}",
            video.display.label(),
            video.resolution.filter(|_| video.display.takes_resolution()).map_or_else(|| "desktop".into(), |r| r.label()),
            video.samples.label(),
            video.shadows.label(),
            video.vsync.label()
        );
    }
}
