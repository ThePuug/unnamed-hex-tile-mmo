use bevy::prelude::*;
use crate::{
    components::*,
    resources::Server,
};
use common_bevy::{
    components::Loc,
    haven::HAVEN_LOCATION,
    systems::*,
};

pub fn setup(
    mut commands: Commands,
    query: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let camera = query.single().expect("query did not return exactly one result");
    commands.spawn((
        UiTargetCamera(camera),
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|parent| {
        parent.spawn((
            Text::new(""),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.),
                left: Val::Px(12.),
                ..default()
            },
            Info::Time,
        ));

        // Distance indicator - shows below time display
        parent.spawn((
            Text::new(""),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(32.),  // Below time display (20px gap)
                left: Val::Px(12.),
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
            Info::DistanceIndicator,
        ));

    });
}

pub fn update(
    mut query: Query<(&mut Text, &Info)>,
    player_query: Query<&Loc, With<crate::components::Viewed>>,
    server: Res<Server>,
    time: Res<Time>,
    mut time_cache: Local<Option<u128>>,
    mut dist_cache: Local<Option<i32>>,
) {
    for (mut span, info) in &mut query {
        match info {
            Info::Time => {
                let wall = server.wall(server.now(time.elapsed().as_millis()));
                let tick = wall / MINUTE_MS;
                if *time_cache == Some(tick) { continue; }
                *time_cache = Some(tick);
                let hour = wall % DAY_MS / HOUR_MS;
                let minute = wall % HOUR_MS / MINUTE_MS;
                **span = format!("{hour:02}:{minute:02} {}", Date::of(wall));
            }
            Info::DistanceIndicator => {
                if let Ok(player_loc) = player_query.single() {
                    let distance = HAVEN_LOCATION.flat_distance(&**player_loc);
                    if *dist_cache == Some(distance) { continue; }
                    *dist_cache = Some(distance);
                    **span = format!("Haven: {} tiles", distance);
                } else {
                    *dist_cache = None;
                    **span = String::from("Haven: -- tiles");
                }
            }
        }
    }
}

/// The UI's scale for a window `window_height` physical pixels tall at
/// scale factor `window_scale`, on a monitor `monitor_height` tall whose
/// desktop draws at `desktop_scale`: the window's share of the monitor, so
/// the HUD covers the same share of a window as of the full screen. Bevy
/// multiplies the UI by the window's scale factor, and Windows keeps one per
/// resolution, so a fullscreen mode below the desktop's draws at its own;
/// the ratio of the two undoes that.
pub fn ui_share(window_height: u32, monitor_height: u32, desktop_scale: f32, window_scale: f32) -> f32 {
    if monitor_height == 0 || window_scale <= 0.0 {
        return 1.0;
    }
    (window_height as f32 / monitor_height as f32).min(1.0) * desktop_scale / window_scale
}

/// Scales the UI to the window's share of the monitor it is on: a smaller
/// window shrinks the HUD with the world instead of crowding it.
pub fn scale_to_window(
    windows: Query<(&Window, Option<&bevy::window::OnMonitor>), With<bevy::window::PrimaryWindow>>,
    monitors: Query<&bevy::window::Monitor>,
    primary: Query<&bevy::window::Monitor, With<bevy::window::PrimaryMonitor>>,
    mut scale: ResMut<UiScale>,
) {
    let Ok((window, on)) = windows.single() else { return };
    // Minimised, a window is zero tall; its HUD keeps the scale it had.
    if window.resolution.physical_height() == 0 {
        return;
    }
    let Some(monitor) = on.and_then(|on| monitors.get(on.0).ok()).or_else(|| primary.single().ok()) else { return };
    // Bevy reads a monitor's size and scale factor as it first meets it, the
    // desktop's, and keeps them while a fullscreen window switches the mode:
    // a lower mode keeps the HUD at the share of the screen it took there.
    let share = ui_share(
        window.resolution.physical_height(),
        monitor.physical_height,
        monitor.scale_factor as f32,
        window.resolution.scale_factor(),
    );
    if (scale.0 - share).abs() > 1e-3 {
        scale.0 = share;
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn the_hud_takes_the_windows_share_of_the_screen() {
        assert_eq!(ui_share(1440, 1440, 1.5, 1.5), 1.0, "full screen as drawn");
        assert_eq!(ui_share(720, 1440, 1.5, 1.5), 0.5, "half the height, half the HUD");
        assert_eq!(ui_share(2000, 1440, 1.5, 1.5), 1.0, "never past full size");
    }

    /// A fullscreen mode below the desktop's draws at a scale factor of its
    /// own; the HUD's share of the screen stays as at the desktop's.
    #[test]
    fn the_hud_keeps_its_share_when_a_mode_changes_the_scale_factor() {
        let share_of_screen = |height: u32, scale: f32| ui_share(height, 1440, 1.5, scale) * scale / height as f32;
        let desktop = share_of_screen(1440, 1.5);
        assert!((share_of_screen(900, 1.0) - desktop).abs() < 1e-6);
        assert!((share_of_screen(900, 1.25) - desktop).abs() < 1e-6);
    }
}
