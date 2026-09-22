use bevy::prelude::*;
use bevy::math::Rot2;
use bevy::ui::UiTransform;
use crate::{
    components::*,
    resources::Server,
    systems::camera::CameraOrbit,
};
use common_bevy::{
    components::{Actor, Loc},
    spatial_difficulty::*,
    systems::*,
};

pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
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
    camera_angle: Res<CameraOrbit>,
) {
    if let Ok(mut ui_transform) = compass_container.single_mut() {
        // Rotate the entire compass to counter-rotate the camera orbit (stay oriented to world).
        // Positive because Rot2 in UI (Y-down) is visually clockwise for positive angles.
        ui_transform.rotation = Rot2::radians(camera_angle.current);
    }
}

pub fn update(
    mut query: Query<(&mut Text, &Info)>,
    player_query: Query<&Loc, With<Actor>>,
    server: Res<Server>,
    time: Res<Time>,
    _camera_angle: Res<CameraOrbit>,
    mut time_cache: Local<Option<u128>>,
    mut dist_cache: Local<Option<(i32, DirectionalZone, u8)>>,
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
                    let zone = get_directional_zone(**player_loc, HAVEN_LOCATION);
                    let level = calculate_enemy_level(**player_loc, HAVEN_LOCATION);
                    let val = (distance, zone, level);
                    if *dist_cache == Some(val) { continue; }
                    *dist_cache = Some(val);
                    let zone_name = match zone {
                        DirectionalZone::North => "North",
                        DirectionalZone::East => "East",
                        DirectionalZone::South => "South",
                        DirectionalZone::West => "West",
                    };
                    **span = format!("Haven: {} tiles | Zone: {} | Enemy Lv. {}", distance, zone_name, level);
                } else {
                    *dist_cache = None;
                    **span = String::from("Haven: -- tiles | Zone: -- | Enemy Lv. --");
                }
            }
        }
    }
}