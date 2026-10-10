use bevy::prelude::*;
use bevy::math::Rot2;
use bevy::ui::UiTransform;
use crate::{
    components::*,
    resources::Server,
    systems::camera::CameraPose,
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

/// The compass: a gold-ringed disc of `size` px with a `border` px ring,
/// its three hex axes and a red N, counter-rotated to the camera by
/// `update_compass`. Spawned as a child of whatever lays it out.
pub fn spawn_compass(parent: &mut ChildSpawnerCommands, size: f32, border: f32) {
    parent.spawn((
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            border: UiRect::all(Val::Px(border)),
            border_radius: BorderRadius::all(Val::Percent(50.)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.7)),
        BorderColor::all(Color::srgb(0.85, 0.65, 0.13)),
        UiTransform::default(),
        CompassContainer,
    ))
    .with_children(|parent| {
        let content = size - 2.0 * border;
        let center = content / 2.0;
        let diameter = content * 0.8;

        // 3 full-diameter lines through center, rotated 30° for pointy-top hex directions
        let line_w = 2.0_f32;
        for angle_deg in [30.0_f32, 90.0, 150.0] {
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(center - line_w / 2.0),
                    top: Val::Px(center - diameter / 2.0),
                    width: Val::Px(line_w),
                    height: Val::Px(diameter),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.7, 0.7, 0.7)),
                UiTransform {
                    rotation: Rot2::radians(angle_deg.to_radians()),
                    ..default()
                },
            ));
        }

        // Red "N" label at true north (top of compass)
        let font_size = 16.0_f32;
        parent.spawn((
            Text::new("N"),
            TextFont {
                font_size: FontSize::Px(font_size),
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.2, 0.2)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(center - font_size / 2.0 + 1.0),
                top: Val::Px(2.0),
                ..default()
            },
        ));
    });
}

pub fn update_compass(
    mut compass_container: Query<&mut UiTransform, With<CompassContainer>>,
    camera: Res<CameraPose>,
) {
    if let Ok(mut ui_transform) = compass_container.single_mut() {
        // Rotate the entire compass to counter-rotate the camera orbit (stay oriented to world).
        // Positive because Rot2 in UI (Y-down) is visually clockwise for positive angles.
        ui_transform.rotation = Rot2::radians(camera.pose.yaw);
    }
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
                let dt = server.current_time(time.elapsed().as_millis());
                let tick = dt / MINUTE_MS;
                if *time_cache == Some(tick) { continue; }
                *time_cache = Some(tick);
                let hour = dt % DAY_MS / HOUR_MS;
                let minute = dt % HOUR_MS / MINUTE_MS;
                **span = format!("{hour:02}:{minute:02} {}", Date::of(dt));
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

/// The UI's scale for a window `window_height` physical pixels tall on a
/// monitor `monitor_height` tall: the window's share of the monitor, so the
/// HUD covers the same share of a window as of the full screen.
pub fn ui_share(window_height: u32, monitor_height: u32) -> f32 {
    if monitor_height == 0 {
        return 1.0;
    }
    (window_height as f32 / monitor_height as f32).min(1.0)
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
    let Some(monitor) = on.and_then(|on| monitors.get(on.0).ok()).or_else(|| primary.single().ok()) else { return };
    let share = ui_share(window.resolution.physical_height(), monitor.physical_height);
    if (scale.0 - share).abs() > 1e-3 {
        scale.0 = share;
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn the_hud_takes_the_windows_share_of_the_screen() {
        assert_eq!(ui_share(1440, 1440), 1.0, "full screen as drawn");
        assert_eq!(ui_share(720, 1440), 0.5, "half the height, half the HUD");
        assert_eq!(ui_share(2000, 1440), 1.0, "never past full size");
    }
}
