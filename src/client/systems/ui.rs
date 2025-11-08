use bevy::prelude::*;

use crate::{
    client::{
        components::*,
        resources::Server
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
    });
}

pub fn update(
    mut query: Query<(&mut Text, &Info)>,
    player_query: Query<&Loc, With<Actor>>,
    server: Res<Server>,
    time: Res<Time>,
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
                if let Ok(player_loc) = player_query.get_single() {
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