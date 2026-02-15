use bevy::prelude::*;
use bevy::math::Rot2;
use bevy::ui::UiTransform;
use std::f32::consts::PI;

use crate::{
    client::{
        components::*,
        resources::Server,
        systems::camera::CameraOrbitAngle,
    },
    common::{
        components::{Actor, Loc},
        spatial_difficulty::*,
        systems::*,
    },
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

        // Distance indicator (ADR-014 Phase 4) - shows below time display
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

        // Compass container - circular with gold border, rotates with camera
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.),
                left: Val::Percent(50.),
                width: Val::Px(100.),
                height: Val::Px(100.),
                border: UiRect::all(Val::Px(4.0)),
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
            // Content area is 92x92 after 4px border on each side
            let content = 92.0_f32;
            let center = content / 2.0;
            let diameter = 72.0_f32;

            // 3 full-diameter lines through center (absolute positioning = independent centering)
            let line_w = 2.0_f32;
            for angle_deg in [0.0_f32, 60.0, 120.0] {
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

            // Red north indicator (half-line, bottom edge at center, extends upward)
            let north_w = 3.0_f32;
            let north_h = diameter / 2.0;
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(center - north_w / 2.0),
                    top: Val::Px(center - north_h),
                    width: Val::Px(north_w),
                    height: Val::Px(north_h),
                    ..default()
                },
                BackgroundColor(Color::srgb(1.0, 0.2, 0.2)),
            ));
        });
    });
}

pub fn update_compass(
    mut compass_container: Query<&mut UiTransform, With<CompassContainer>>,
    camera_angle: Res<CameraOrbitAngle>,
) {
    if let Ok(mut ui_transform) = compass_container.single_mut() {
        // Rotate the entire compass based on camera angle
        // Negative because we want it to counter-rotate (stay oriented to world)
        ui_transform.rotation = Rot2::radians(-camera_angle.0);
    }
}

pub fn update(
    mut query: Query<(&mut Text, &Info)>,
    player_query: Query<&Loc, With<Actor>>,
    server: Res<Server>,
    time: Res<Time>,
    camera_angle: Res<CameraOrbitAngle>,
) {
    for (mut span, info) in &mut query {
        **span = match info {
            Info::Time => {
                let dt = server.current_time(time.elapsed().as_millis());
                let season = match dt % YEAR_MS / SEASON_MS {
                    0 => "Thaw",
                    1 => "Blaze",
                    2 => "Ash",
                    _ => "Freeze",
                };
                let week = match dt % SEASON_MS / WEEK_MS {
                    0 => "Mon",
                    1 => "Tus",
                    2 => "Wed",
                    3 => "Tur",
                    4 => "Fid",
                    5 => "Sat",
                    _ => "Sun",
                };
                let day = dt % WEEK_MS / DAY_MS;
                let hour = dt % DAY_MS / HOUR_MS;
                let minute = dt % HOUR_MS / MINUTE_MS;
                format!("{hour:02}:{minute:02} {day}.{week}.{season}")
            }
            Info::DistanceIndicator => {
                // ADR-014 Phase 4: Distance indicator showing haven distance, zone, and enemy level
                if let Ok(player_loc) = player_query.single() {
                    let distance = HAVEN_LOCATION.flat_distance(&**player_loc);
                    let zone = get_directional_zone(**player_loc, HAVEN_LOCATION);
                    let level = calculate_enemy_level(**player_loc, HAVEN_LOCATION);

                    let zone_name = match zone {
                        DirectionalZone::North => "North",
                        DirectionalZone::East => "East",
                        DirectionalZone::South => "South",
                        DirectionalZone::West => "West",
                    };

                    format!("Haven: {} tiles | Zone: {} | Enemy Lv. {}", distance, zone_name, level)
                } else {
                    String::from("Haven: -- tiles | Zone: -- | Enemy Lv. --")
                }
            }
        };
    }
}